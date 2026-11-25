use super::types::{
    ContractLineItemInput, ContractVersionInput, EvidenceRefOutput, FindingOutput, InvoiceInput,
    InvoiceLineItemInput, PoLineItemInput, PurchaseOrderInput, SeverityRules,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashSet;
use uuid::Uuid;

/// Token Jaccard similarity (pure deterministic Decimal arithmetic)
pub fn token_jaccard_score(a: &str, b: &str) -> Decimal {
    let norm_tokens = |s: &str| -> HashSet<String> {
        s.to_ascii_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| t.len() >= 2)
            .map(|t| t.to_string())
            .collect()
    };
    let sa = norm_tokens(a);
    let sb = norm_tokens(b);
    if sa.is_empty() || sb.is_empty() {
        return Decimal::ZERO;
    }
    let inter = sa.intersection(&sb).count() as i64;
    let union = sa.union(&sb).count() as i64;
    if union == 0 {
        Decimal::ZERO
    } else {
        (Decimal::from(inter) / Decimal::from(union)).round_dp(4)
    }
}

/// Select the contract version valid at `invoice_date` (considering amendments).
/// Never blindly picks latest version if an earlier version covered `invoice_date`.
pub fn resolve_effective_contract_version<'a>(
    versions: &'a [ContractVersionInput],
    invoice: &InvoiceInput,
) -> (Option<&'a ContractVersionInput>, String, Option<String>) {
    let mut matching: Vec<&ContractVersionInput> = versions
        .iter()
        .filter(|v| v.vendor_id == invoice.vendor_id)
        .filter(|v| match &invoice.referenced_contract_number {
            Some(ref_num) => v.contract_number.eq_ignore_ascii_case(ref_num),
            None => true,
        })
        .collect();

    if matching.is_empty() {
        return (
            None,
            "unmatched".into(),
            Some("No contract found matching vendor and contract reference.".into()),
        );
    }

    let mut valid_at_date: Vec<&ContractVersionInput> = matching
        .iter()
        .copied()
        .filter(|v| invoice.invoice_date >= v.effective_date && invoice.invoice_date <= v.expiration_date)
        .collect();

    if valid_at_date.is_empty() {
        matching.sort_by_key(|v| v.version_number);
        let latest = matching.last().copied();
        return (
            latest,
            "contract_expired_or_not_effective".into(),
            Some(format!(
                "Invoice date {} falls outside effective period of all matching contract versions.",
                invoice.invoice_date
            )),
        );
    }

    let distinct_contracts: HashSet<Uuid> = valid_at_date.iter().map(|v| v.contract_id).collect();
    if distinct_contracts.len() > 1 {
        return (
            None,
            "ambiguous_needs_review".into(),
            Some(format!(
                "{} active contracts match vendor on invoice date without unambiguous contract reference.",
                distinct_contracts.len()
            )),
        );
    }

    valid_at_date.sort_by_key(|v| (v.is_amendment, v.version_number));
    let chosen = valid_at_date.last().copied();
    let status = if invoice.referenced_contract_number.is_some() {
        "deterministic_matched"
    } else {
        "ambiguous_needs_review"
    };
    (chosen, status.into(), None)
}

pub struct LineContractMatch<'a> {
    pub contract_item: Option<&'a ContractLineItemInput>,
    pub match_method: String,
    pub requires_review: bool,
    pub candidate_score: Option<Decimal>,
    pub ambiguity_finding: Option<FindingOutput>,
}

pub fn match_line_to_contract<'a>(
    line: &InvoiceLineItemInput,
    invoice: &InvoiceInput,
    contract_version: Option<&'a ContractVersionInput>,
    matched_contract_id: Option<Uuid>,
    severity_rules: &SeverityRules,
) -> LineContractMatch<'a> {
    let Some(cv) = contract_version else {
        return LineContractMatch {
            contract_item: None,
            match_method: "unmatched".into(),
            requires_review: false,
            candidate_score: None,
            ambiguity_finding: None,
        };
    };

    if let Some(confirmed_id) = line.human_confirmed_contract_item_id {
        if let Some(ci) = cv.line_items.iter().find(|ci| ci.id == confirmed_id) {
            return LineContractMatch {
                contract_item: Some(ci),
                match_method: "human_confirmed".into(),
                requires_review: false,
                candidate_score: None,
                ambiguity_finding: None,
            };
        }
    }

    if let Some(code) = &line.item_code {
        if let Some(ci) = cv.line_items.iter().find(|ci| {
            ci.item_code
                .as_ref()
                .map(|cc| cc.eq_ignore_ascii_case(code))
                .unwrap_or(false)
        }) {
            return LineContractMatch {
                contract_item: Some(ci),
                match_method: "exact_code".into(),
                requires_review: false,
                candidate_score: None,
                ambiguity_finding: None,
            };
        }
    }

    if let Some(ci) = cv
        .line_items
        .iter()
        .find(|ci| ci.description.trim().eq_ignore_ascii_case(line.description.trim()))
    {
        return LineContractMatch {
            contract_item: Some(ci),
            match_method: "exact_description".into(),
            requires_review: false,
            candidate_score: None,
            ambiguity_finding: None,
        };
    }

    // Transparent fuzzy candidate matching — NEVER silently asserts equivalence!
    let mut best: Option<(&ContractLineItemInput, Decimal)> = None;
    for ci in &cv.line_items {
        let score = token_jaccard_score(&line.description, &ci.description);
        if score >= dec!(0.40) && best.map(|(_, s)| score > s).unwrap_or(true) {
            best = Some((ci, score));
        }
    }
    if let Some((ci, score)) = best {
        let finding = FindingOutput {
            category: "DOCUMENT_MATCH_AMBIGUITY".into(),
            classification_type: "unresolved_discrepancy".into(),
            severity: severity_rules.document_match_ambiguity_severity.clone(),
            severity_rule_applied: "SEVERITY_RULE_FUZZY_LINE_MATCH".into(),
            status: "requires_review".into(),
            invoice_line_item_id: Some(line.id),
            contract_id: matched_contract_id,
            expected_value: Some(ci.unit_price),
            actual_value: Some(line.unit_price),
            variance_amount: Some(line.unit_price - ci.unit_price),
            currency: Some(invoice.currency.clone()),
            rule_applied: "FUZZY_CANDIDATE_REQUIRES_HUMAN_CONFIRMATION".into(),
            explanation: format!(
                "Invoice line {} ('{}') was matched only as a fuzzy candidate (token Jaccard={}) to contract item '{}'. Human confirmation is required before asserting financial equivalence.",
                line.line_number, line.description, score, ci.description
            ),
            calculation_details: serde_json::json!({
                "invoice_description": line.description,
                "candidate_contract_description": ci.description,
                "token_jaccard_score": score
            }),
            evidence_refs: vec![EvidenceRefOutput {
                document_id: invoice.document_id,
                evidence_role: "invoice_source".into(),
                page_number: line.source_page,
                bounding_box: line.source_bbox.clone(),
                quoted_text: Some(line.description.clone()),
                source_location: Some(format!("invoice:line:{}", line.line_number)),
            }],
        };
        return LineContractMatch {
            contract_item: Some(ci),
            match_method: "fuzzy_candidate".into(),
            requires_review: true,
            candidate_score: Some(score),
            ambiguity_finding: Some(finding),
        };
    }

    LineContractMatch {
        contract_item: None,
        match_method: "unmatched".into(),
        requires_review: false,
        candidate_score: None,
        ambiguity_finding: None,
    }
}

pub fn match_line_to_po<'a>(
    line: &InvoiceLineItemInput,
    matched_po: Option<&'a PurchaseOrderInput>,
) -> Option<&'a PoLineItemInput> {
    matched_po.and_then(|po| {
        if let Some(code) = &line.item_code {
            if let Some(pi) = po.line_items.iter().find(|pi| {
                pi.item_code
                    .as_ref()
                    .map(|c| c.eq_ignore_ascii_case(code))
                    .unwrap_or(false)
            }) {
                return Some(pi);
            }
        }
        po.line_items
            .iter()
            .find(|pi| pi.description.trim().eq_ignore_ascii_case(line.description.trim()))
    })
}
