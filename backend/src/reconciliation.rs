use crate::normalization::{normalize_unit, round_for_currency};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

pub const RECONCILIATION_ENGINE_VERSION: &str = "abifr-deterministic-engine-v1.0.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeverityRules {
    pub critical_variance_abs: Decimal,
    pub high_variance_abs: Decimal,
    pub medium_variance_abs: Decimal,
    pub duplicate_invoice_severity: String,
    pub contract_expired_severity: String,
    pub currency_mismatch_severity: String,
    pub extraction_uncertain_severity: String,
    pub document_match_ambiguity_severity: String,
}

impl Default for SeverityRules {
    fn default() -> Self {
        Self {
            critical_variance_abs: dec!(5000),
            high_variance_abs: dec!(1000),
            medium_variance_abs: dec!(100),
            duplicate_invoice_severity: "critical".into(),
            contract_expired_severity: "high".into(),
            currency_mismatch_severity: "high".into(),
            extraction_uncertain_severity: "medium".into(),
            document_match_ambiguity_severity: "medium".into(),
        }
    }
}

impl SeverityRules {
    pub fn severity_for_variance(&self, abs_variance: Decimal) -> (&'static str, &'static str) {
        if abs_variance >= self.critical_variance_abs {
            ("critical", "SEVERITY_RULE_VARIANCE_CRITICAL")
        } else if abs_variance >= self.high_variance_abs {
            ("high", "SEVERITY_RULE_VARIANCE_HIGH")
        } else if abs_variance >= self.medium_variance_abs {
            ("medium", "SEVERITY_RULE_VARIANCE_MEDIUM")
        } else {
            ("low", "SEVERITY_RULE_VARIANCE_LOW")
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractVersionInput {
    pub version_id: Uuid,
    pub contract_id: Uuid,
    pub contract_number: String,
    pub vendor_id: Uuid,
    pub currency: String,
    pub version_label: String,
    pub version_number: i32,
    pub is_amendment: bool,
    pub effective_date: NaiveDate,
    pub expiration_date: NaiveDate,
    pub price_tolerance_pct: Option<Decimal>,
    pub price_tolerance_abs: Option<Decimal>,
    pub quantity_tolerance_pct: Option<Decimal>,
    pub default_tax_rate: Option<Decimal>,
    pub allowed_additional_charges: Vec<String>,
    pub document_id: Option<Uuid>,
    pub line_items: Vec<ContractLineItemInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractLineItemInput {
    pub id: Uuid,
    pub item_code: Option<String>,
    pub description: String,
    pub unit: String,
    pub unit_price: Decimal,
    pub currency: String,
    pub min_quantity: Option<Decimal>,
    pub max_quantity: Option<Decimal>,
    pub discount_pct: Decimal,
    pub tax_rate: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurchaseOrderInput {
    pub id: Uuid,
    pub po_number: String,
    pub vendor_id: Uuid,
    pub currency: String,
    pub total_amount: Decimal,
    pub document_id: Option<Uuid>,
    pub line_items: Vec<PoLineItemInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoLineItemInput {
    pub id: Uuid,
    pub item_code: Option<String>,
    pub description: String,
    pub unit: String,
    pub ordered_quantity: Decimal,
    pub received_quantity: Option<Decimal>,
    pub unit_price: Decimal,
    pub line_total: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceInput {
    pub id: Uuid,
    pub document_id: Option<Uuid>,
    pub invoice_number: String,
    pub vendor_id: Uuid,
    pub invoice_date: NaiveDate,
    pub currency: String,
    pub referenced_contract_number: Option<String>,
    pub referenced_po_number: Option<String>,
    pub stated_subtotal: Decimal,
    pub stated_discount_total: Decimal,
    pub stated_tax_total: Decimal,
    pub stated_additional_charges: Decimal,
    pub stated_total: Decimal,
    pub is_credit_note: bool,
    pub line_items: Vec<InvoiceLineItemInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceLineItemInput {
    pub id: Uuid,
    pub line_number: i32,
    pub item_code: Option<String>,
    pub description: String,
    pub quantity: Decimal,
    pub unit: String,
    pub unit_price: Decimal,
    pub discount_pct: Decimal,
    pub discount_amount: Decimal,
    pub tax_rate: Option<Decimal>,
    pub tax_amount: Decimal,
    pub stated_line_subtotal: Decimal,
    pub stated_line_total: Decimal,
    pub is_additional_charge: bool,
    pub human_confirmed_contract_item_id: Option<Uuid>,
    pub source_page: Option<i32>,
    pub source_bbox: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExistingInvoiceSummary {
    pub id: Uuid,
    pub document_id: Option<Uuid>,
    pub sha256_hash: Option<String>,
    pub invoice_number: String,
    pub vendor_id: Uuid,
    pub invoice_date: NaiveDate,
    pub currency: String,
    pub stated_total: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentInput {
    pub id: Uuid,
    pub document_id: Option<Uuid>,
    pub payment_reference: String,
    pub bank_reference: Option<String>,
    pub vendor_id: Uuid,
    pub payment_date: NaiveDate,
    pub amount: Decimal,
    pub currency: String,
    pub referenced_invoice_number: Option<String>,
    pub transaction_status: String,
    pub is_refund_or_credit: bool,
    /// Pre-existing explicit allocations (for one payment covering multiple invoices)
    pub explicit_allocated_to_this_invoice: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageResult {
    pub stage_code: String,
    pub stage_name: String,
    pub status: String, // pass|discrepancy|warning|requires_review|skipped
    pub invoice_line_item_id: Option<Uuid>,
    pub contract_line_item_id: Option<Uuid>,
    pub expected_value: Option<Decimal>,
    pub actual_value: Option<Decimal>,
    pub variance_amount: Option<Decimal>,
    pub variance_pct: Option<Decimal>,
    pub permitted_tolerance: Option<String>,
    pub amount_outside_tolerance: Option<Decimal>,
    pub currency: Option<String>,
    pub rule_applied: String,
    pub calculation_details: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRefOutput {
    pub document_id: Option<Uuid>,
    pub evidence_role: String,
    pub page_number: Option<i32>,
    pub bounding_box: Option<serde_json::Value>,
    pub quoted_text: Option<String>,
    pub source_location: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindingOutput {
    pub category: String,
    pub classification_type: String, // exact_duplicate|probable_duplicate|potential_anomaly|confirmed_rule_violation|unresolved_discrepancy
    pub severity: String,
    pub severity_rule_applied: String,
    pub status: String, // open|requires_review
    pub invoice_line_item_id: Option<Uuid>,
    pub contract_id: Option<Uuid>,
    pub expected_value: Option<Decimal>,
    pub actual_value: Option<Decimal>,
    pub variance_amount: Option<Decimal>,
    pub currency: Option<String>,
    pub rule_applied: String,
    pub explanation: String,
    pub calculation_details: serde_json::Value,
    pub evidence_refs: Vec<EvidenceRefOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineMatchDecision {
    pub invoice_line_item_id: Uuid,
    pub matched_contract_item_id: Option<Uuid>,
    pub matched_po_item_id: Option<Uuid>,
    pub match_method: String,
    pub match_requires_review: bool,
    pub candidate_score: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconciliationOutput {
    pub engine_version: String,
    pub overall_status: String, // reconciled|discrepancy_confirmed|requires_review
    pub matched_contract_id: Option<Uuid>,
    pub matched_contract_version_id: Option<Uuid>,
    pub matched_po_id: Option<Uuid>,
    pub match_confidence_status: String,
    pub recalculated_subtotal: Decimal,
    pub recalculated_discount_total: Decimal,
    pub recalculated_tax_total: Decimal,
    pub recalculated_additional_charges: Decimal,
    pub recalculated_total: Decimal,
    pub total_allocated_payments: Decimal,
    pub payment_status: String, // unpaid|partially_paid|fully_paid|overpaid|requires_review
    pub line_matches: Vec<LineMatchDecision>,
    pub stages: Vec<StageResult>,
    pub findings: Vec<FindingOutput>,
}

/// Token Jaccard similarity (pure deterministic Decimal arithmetic)
fn token_jaccard_score(a: &str, b: &str) -> Decimal {
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

    // Filter versions valid on invoice_date
    let mut valid_at_date: Vec<&ContractVersionInput> = matching
        .iter()
        .copied()
        .filter(|v| invoice.invoice_date >= v.effective_date && invoice.invoice_date <= v.expiration_date)
        .collect();

    if valid_at_date.is_empty() {
        // Return the closest version for audit context while explicitly flagging CONTRACT_EXPIRED
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

    // Check if multiple DISTINCT contracts matched without an explicit contract reference
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

    // Within the same contract, amendments with higher version_number supersede base versions
    // ONLY among versions whose effective window covers invoice_date.
    valid_at_date.sort_by_key(|v| (v.is_amendment, v.version_number));
    let chosen = valid_at_date.last().copied();
    let status = if invoice.referenced_contract_number.is_some() {
        "deterministic_matched"
    } else {
        "ambiguous_needs_review"
    };
    (chosen, status.into(), None)
}

/// Authoritative 12-stage (A..L) deterministic reconciliation engine.
pub fn run_reconciliation(
    invoice: &InvoiceInput,
    invoice_sha256: Option<&str>,
    contract_versions: &[ContractVersionInput],
    purchase_orders: &[PurchaseOrderInput],
    existing_invoices: &[ExistingInvoiceSummary],
    payments: &[PaymentInput],
    severity_rules: &SeverityRules,
) -> ReconciliationOutput {
    let mut stages: Vec<StageResult> = Vec::new();
    let mut findings: Vec<FindingOutput> = Vec::new();
    let mut line_matches: Vec<LineMatchDecision> = Vec::new();
    let mut requires_review = false;

    // -------------------------------------------------------------------------
    // STAGE A & B: Document Relationship Matching & Contract Version Resolution
    // -------------------------------------------------------------------------
    let (resolved_contract, match_status, resolution_note) =
        resolve_effective_contract_version(contract_versions, invoice);

    let mut contract_usable = false;
    let mut matched_contract_id: Option<Uuid> = None;
    let mut matched_contract_version_id: Option<Uuid> = None;

    match (&resolved_contract, match_status.as_str()) {
        (Some(cv), "deterministic_matched") => {
            matched_contract_id = Some(cv.contract_id);
            matched_contract_version_id = Some(cv.version_id);
            contract_usable = true;
            stages.push(StageResult {
                stage_code: "A".into(),
                stage_name: "Document Relationship Matching".into(),
                status: "pass".into(),
                invoice_line_item_id: None,
                contract_line_item_id: None,
                expected_value: None,
                actual_value: None,
                variance_amount: None,
                variance_pct: None,
                permitted_tolerance: None,
                amount_outside_tolerance: None,
                currency: Some(invoice.currency.clone()),
                rule_applied: "CONTRACT_REF_AND_VENDOR_MATCH".into(),
                calculation_details: serde_json::json!({
                    "contract_number": cv.contract_number,
                    "version_label": cv.version_label,
                    "is_amendment": cv.is_amendment,
                    "effective_date": cv.effective_date,
                    "expiration_date": cv.expiration_date
                }),
            });
            stages.push(StageResult {
                stage_code: "B".into(),
                stage_name: "Contract Version Resolution".into(),
                status: "pass".into(),
                invoice_line_item_id: None,
                contract_line_item_id: None,
                expected_value: None,
                actual_value: None,
                variance_amount: None,
                variance_pct: None,
                permitted_tolerance: None,
                amount_outside_tolerance: None,
                currency: Some(cv.currency.clone()),
                rule_applied: "CONTRACT_EFFECTIVE_AT_INVOICE_DATE".into(),
                calculation_details: serde_json::json!({
                    "invoice_date": invoice.invoice_date,
                    "selected_version_number": cv.version_number,
                    "selected_version_label": cv.version_label
                }),
            });

            // Currency mismatch check — NEVER silently convert currencies!
            if !cv.currency.eq_ignore_ascii_case(&invoice.currency) {
                contract_usable = false;
                findings.push(FindingOutput {
                    category: "CURRENCY_MISMATCH".into(),
                    classification_type: "confirmed_rule_violation".into(),
                    severity: severity_rules.currency_mismatch_severity.clone(),
                    severity_rule_applied: "SEVERITY_RULE_CURRENCY_MISMATCH".into(),
                    status: "open".into(),
                    invoice_line_item_id: None,
                    contract_id: Some(cv.contract_id),
                    expected_value: None,
                    actual_value: Some(invoice.stated_total),
                    variance_amount: None,
                    currency: Some(invoice.currency.clone()),
                    rule_applied: "NO_UNVERIFIED_FX_CONVERSION".into(),
                    explanation: format!(
                        "Invoice currency '{}' does not match contract '{}' currency '{}'. No FX conversion applied because no verified exchange rate for {} is configured.",
                        invoice.currency, cv.contract_number, cv.currency, invoice.invoice_date
                    ),
                    calculation_details: serde_json::json!({
                        "contract_currency": cv.currency,
                        "invoice_currency": invoice.currency,
                        "fx_rate_available": false
                    }),
                    evidence_refs: vec![
                        EvidenceRefOutput {
                            document_id: invoice.document_id,
                            evidence_role: "invoice_source".into(),
                            page_number: Some(1),
                            bounding_box: None,
                            quoted_text: Some(format!("Currency: {}", invoice.currency)),
                            source_location: Some("invoice:header:currency".into()),
                        },
                        EvidenceRefOutput {
                            document_id: cv.document_id,
                            evidence_role: "contract_source".into(),
                            page_number: Some(1),
                            bounding_box: None,
                            quoted_text: Some(format!("Contract Currency: {}", cv.currency)),
                            source_location: Some("contract:header:currency".into()),
                        },
                    ],
                });
            }
        }
        (Some(cv), "contract_expired_or_not_effective") => {
            matched_contract_id = Some(cv.contract_id);
            matched_contract_version_id = Some(cv.version_id);
            stages.push(StageResult {
                stage_code: "B".into(),
                stage_name: "Contract Version Resolution".into(),
                status: "discrepancy".into(),
                invoice_line_item_id: None,
                contract_line_item_id: None,
                expected_value: None,
                actual_value: None,
                variance_amount: None,
                variance_pct: None,
                permitted_tolerance: None,
                amount_outside_tolerance: None,
                currency: Some(invoice.currency.clone()),
                rule_applied: "CONTRACT_VALIDITY_WINDOW_CHECK".into(),
                calculation_details: serde_json::json!({
                    "invoice_date": invoice.invoice_date,
                    "contract_effective_date": cv.effective_date,
                    "contract_expiration_date": cv.expiration_date,
                    "note": resolution_note
                }),
            });
            findings.push(FindingOutput {
                category: "CONTRACT_EXPIRED".into(),
                classification_type: "confirmed_rule_violation".into(),
                severity: severity_rules.contract_expired_severity.clone(),
                severity_rule_applied: "SEVERITY_RULE_CONTRACT_EXPIRED".into(),
                status: "open".into(),
                invoice_line_item_id: None,
                contract_id: Some(cv.contract_id),
                expected_value: None,
                actual_value: Some(invoice.stated_total),
                variance_amount: None,
                currency: Some(invoice.currency.clone()),
                rule_applied: "CONTRACT_VALIDITY_WINDOW_CHECK".into(),
                explanation: format!(
                    "Invoice {} dated {} falls outside contract {} ({}) validity window {} to {}.",
                    invoice.invoice_number,
                    invoice.invoice_date,
                    cv.contract_number,
                    cv.version_label,
                    cv.effective_date,
                    cv.expiration_date
                ),
                calculation_details: serde_json::json!({
                    "invoice_date": invoice.invoice_date,
                    "effective_date": cv.effective_date,
                    "expiration_date": cv.expiration_date
                }),
                evidence_refs: vec![
                    EvidenceRefOutput {
                        document_id: invoice.document_id,
                        evidence_role: "invoice_source".into(),
                        page_number: Some(1),
                        bounding_box: None,
                        quoted_text: Some(format!("Invoice Date: {}", invoice.invoice_date)),
                        source_location: Some("invoice:header:invoice_date".into()),
                    },
                    EvidenceRefOutput {
                        document_id: cv.document_id,
                        evidence_role: "contract_source".into(),
                        page_number: Some(1),
                        bounding_box: None,
                        quoted_text: Some(format!("Valid {} to {}", cv.effective_date, cv.expiration_date)),
                        source_location: Some("contract:header:validity".into()),
                    },
                ],
            });
        }
        (opt_cv, _) => {
            requires_review = true;
            if let Some(cv) = opt_cv {
                matched_contract_id = Some(cv.contract_id);
                matched_contract_version_id = Some(cv.version_id);
            }
            stages.push(StageResult {
                stage_code: "A".into(),
                stage_name: "Document Relationship Matching".into(),
                status: "requires_review".into(),
                invoice_line_item_id: None,
                contract_line_item_id: None,
                expected_value: None,
                actual_value: None,
                variance_amount: None,
                variance_pct: None,
                permitted_tolerance: None,
                amount_outside_tolerance: None,
                currency: Some(invoice.currency.clone()),
                rule_applied: "CONTRACT_MATCH_REQUIRES_CONFIRMATION".into(),
                calculation_details: serde_json::json!({ "match_status": match_status, "note": resolution_note }),
            });
            findings.push(FindingOutput {
                category: "DOCUMENT_MATCH_AMBIGUITY".into(),
                classification_type: "unresolved_discrepancy".into(),
                severity: severity_rules.document_match_ambiguity_severity.clone(),
                severity_rule_applied: "SEVERITY_RULE_MATCH_AMBIGUITY".into(),
                status: "requires_review".into(),
                invoice_line_item_id: None,
                contract_id: matched_contract_id,
                expected_value: None,
                actual_value: Some(invoice.stated_total),
                variance_amount: None,
                currency: Some(invoice.currency.clone()),
                rule_applied: "CONTRACT_MATCH_REQUIRES_CONFIRMATION".into(),
                explanation: resolution_note
                    .unwrap_or_else(|| "Contract relationship requires explicit human confirmation.".into()),
                calculation_details: serde_json::json!({ "match_status": match_status }),
                evidence_refs: vec![EvidenceRefOutput {
                    document_id: invoice.document_id,
                    evidence_role: "invoice_source".into(),
                    page_number: Some(1),
                    bounding_box: None,
                    quoted_text: invoice.referenced_contract_number.clone(),
                    source_location: Some("invoice:header:contract_number".into()),
                }],
            });
        }
    }

    // -------------------------------------------------------------------------
    // STAGE C: Purchase Order Matching
    // -------------------------------------------------------------------------
    let matched_po: Option<&PurchaseOrderInput> = invoice
        .referenced_po_number
        .as_ref()
        .and_then(|po_num| {
            purchase_orders
                .iter()
                .find(|po| po.vendor_id == invoice.vendor_id && po.po_number.eq_ignore_ascii_case(po_num))
        });

    if let Some(po) = matched_po {
        let po_status = if po.currency.eq_ignore_ascii_case(&invoice.currency) {
            "pass"
        } else {
            "discrepancy"
        };
        stages.push(StageResult {
            stage_code: "C".into(),
            stage_name: "Purchase Order Matching".into(),
            status: po_status.into(),
            invoice_line_item_id: None,
            contract_line_item_id: None,
            expected_value: Some(po.total_amount),
            actual_value: Some(invoice.stated_total),
            variance_amount: Some(invoice.stated_total - po.total_amount),
            variance_pct: if po.total_amount != Decimal::ZERO {
                Some((((invoice.stated_total - po.total_amount) / po.total_amount) * dec!(100)).round_dp(4))
            } else {
                None
            },
            permitted_tolerance: Some("PO_REFERENCE_MATCH".into()),
            amount_outside_tolerance: None,
            currency: Some(invoice.currency.clone()),
            rule_applied: "PO_NUMBER_AND_VENDOR_MATCH".into(),
            calculation_details: serde_json::json!({ "po_number": po.po_number, "po_currency": po.currency }),
        });
    } else if invoice.referenced_po_number.is_some() {
        requires_review = true;
        stages.push(StageResult {
            stage_code: "C".into(),
            stage_name: "Purchase Order Matching".into(),
            status: "requires_review".into(),
            invoice_line_item_id: None,
            contract_line_item_id: None,
            expected_value: None,
            actual_value: None,
            variance_amount: None,
            variance_pct: None,
            permitted_tolerance: None,
            amount_outside_tolerance: None,
            currency: Some(invoice.currency.clone()),
            rule_applied: "PO_REFERENCE_MISSING_IN_REPOSITORY".into(),
            calculation_details: serde_json::json!({ "referenced_po_number": invoice.referenced_po_number }),
        });
    }

    // -------------------------------------------------------------------------
    // STAGES D, E, F, G, H, J: Line-Item Matching & Deterministic Line Validation
    // -------------------------------------------------------------------------
    let mut recalc_subtotal = Decimal::ZERO;
    let mut recalc_discount_total = Decimal::ZERO;
    let mut recalc_tax_total = Decimal::ZERO;
    let mut recalc_additional_charges = Decimal::ZERO;

    // Track repeated line items within the same invoice (Stage L helper)
    let mut seen_line_keys: HashMap<String, i32> = HashMap::new();

    for line in &invoice.line_items {
        // Stage I per-line exact Decimal recalculation
        let gross = line.quantity * line.unit_price;
        let line_discount = if line.discount_amount != Decimal::ZERO {
            line.discount_amount
        } else if line.discount_pct != Decimal::ZERO {
            round_for_currency(gross * (line.discount_pct / dec!(100)), &invoice.currency)
        } else {
            Decimal::ZERO
        };
        let expected_line_subtotal = round_for_currency(gross - line_discount, &invoice.currency);

        if line.is_additional_charge {
            recalc_additional_charges += expected_line_subtotal;
        } else {
            recalc_subtotal += expected_line_subtotal;
            recalc_discount_total += line_discount;
        }

        // Check line-level subtotal arithmetic
        let line_sub_var = line.stated_line_subtotal - expected_line_subtotal;
        if line_sub_var != Decimal::ZERO {
            let (sev, sev_rule) = severity_rules.severity_for_variance(line_sub_var.abs());
            findings.push(FindingOutput {
                category: "TOTAL_ARITHMETIC_MISMATCH".into(),
                classification_type: "confirmed_rule_violation".into(),
                severity: sev.into(),
                severity_rule_applied: sev_rule.into(),
                status: "open".into(),
                invoice_line_item_id: Some(line.id),
                contract_id: matched_contract_id,
                expected_value: Some(expected_line_subtotal),
                actual_value: Some(line.stated_line_subtotal),
                variance_amount: Some(line_sub_var),
                currency: Some(invoice.currency.clone()),
                rule_applied: "DETERMINISTIC_LINE_SUBTOTAL_CHECK".into(),
                explanation: format!(
                    "Line {} ('{}') stated subtotal {} differs from qty ({}) * unit_price ({}) - discount ({}) = {}.",
                    line.line_number, line.description, line.stated_line_subtotal, line.quantity, line.unit_price, line_discount, expected_line_subtotal
                ),
                calculation_details: serde_json::json!({
                    "quantity": line.quantity,
                    "unit_price": line.unit_price,
                    "discount": line_discount,
                    "expected_line_subtotal": expected_line_subtotal,
                    "stated_line_subtotal": line.stated_line_subtotal
                }),
                evidence_refs: vec![EvidenceRefOutput {
                    document_id: invoice.document_id,
                    evidence_role: "invoice_source".into(),
                    page_number: line.source_page,
                    bounding_box: line.source_bbox.clone(),
                    quoted_text: Some(format!("{} | qty {} x {} = {}", line.description, line.quantity, line.unit_price, line.stated_line_subtotal)),
                    source_location: Some(format!("invoice:line:{}", line.line_number)),
                }],
            });
        }

        // Repeated line item detection within same invoice
        let dup_line_key = format!(
            "{}|{}|{}",
            line.item_code.clone().unwrap_or_default().to_ascii_lowercase(),
            line.description.trim().to_ascii_lowercase(),
            line.unit_price
        );
        if let Some(prev_line_no) = seen_line_keys.get(&dup_line_key) {
            findings.push(FindingOutput {
                category: "DUPLICATE_INVOICE".into(),
                classification_type: "potential_anomaly".into(),
                severity: "medium".into(),
                severity_rule_applied: "SEVERITY_RULE_REPEATED_LINE_ITEM".into(),
                status: "requires_review".into(),
                invoice_line_item_id: Some(line.id),
                contract_id: matched_contract_id,
                expected_value: None,
                actual_value: Some(line.stated_line_total),
                variance_amount: None,
                currency: Some(invoice.currency.clone()),
                rule_applied: "INTRA_INVOICE_REPEATED_LINE_CHECK".into(),
                explanation: format!(
                    "Line {} ('{}' @ {}) duplicates line {} within invoice {}. Requires human verification.",
                    line.line_number, line.description, line.unit_price, prev_line_no, invoice.invoice_number
                ),
                calculation_details: serde_json::json!({
                    "current_line": line.line_number,
                    "previous_line": prev_line_no
                }),
                evidence_refs: vec![EvidenceRefOutput {
                    document_id: invoice.document_id,
                    evidence_role: "invoice_source".into(),
                    page_number: line.source_page,
                    bounding_box: line.source_bbox.clone(),
                    quoted_text: Some(line.description.clone()),
                    source_location: Some(format!("invoice:line:{}", line.line_number)),
                }],
            });
            requires_review = true;
        } else {
            seen_line_keys.insert(dup_line_key, line.line_number);
        }

        // STAGE J: Additional Charge Verification
        if line.is_additional_charge {
            let mut authorized = false;
            if let Some(cv) = resolved_contract.filter(|_| contract_usable) {
                let desc_low = line.description.to_ascii_lowercase();
                authorized = cv
                    .allowed_additional_charges
                    .iter()
                    .any(|allowed| desc_low.contains(&allowed.to_ascii_lowercase()));
            }
            if !authorized {
                let (sev, sev_rule) = severity_rules.severity_for_variance(expected_line_subtotal.abs());
                stages.push(StageResult {
                    stage_code: "J".into(),
                    stage_name: "Additional Charge Verification".into(),
                    status: "discrepancy".into(),
                    invoice_line_item_id: Some(line.id),
                    contract_line_item_id: None,
                    expected_value: Some(Decimal::ZERO),
                    actual_value: Some(expected_line_subtotal),
                    variance_amount: Some(expected_line_subtotal),
                    variance_pct: None,
                    permitted_tolerance: Some("NONE_DOCUMENTED".into()),
                    amount_outside_tolerance: Some(expected_line_subtotal),
                    currency: Some(invoice.currency.clone()),
                    rule_applied: "CONTRACT_ALLOWED_ADDITIONAL_CHARGES".into(),
                    calculation_details: serde_json::json!({ "charge_description": line.description }),
                });
                findings.push(FindingOutput {
                    category: "UNAUTHORIZED_CHARGE".into(),
                    classification_type: "confirmed_rule_violation".into(),
                    severity: sev.into(),
                    severity_rule_applied: sev_rule.into(),
                    status: "open".into(),
                    invoice_line_item_id: Some(line.id),
                    contract_id: matched_contract_id,
                    expected_value: Some(Decimal::ZERO),
                    actual_value: Some(expected_line_subtotal),
                    variance_amount: Some(expected_line_subtotal),
                    currency: Some(invoice.currency.clone()),
                    rule_applied: "CONTRACT_ALLOWED_ADDITIONAL_CHARGES".into(),
                    explanation: format!(
                        "Additional charge '{}' ({} {}) is not authorized under applicable contract terms.",
                        line.description, expected_line_subtotal, invoice.currency
                    ),
                    calculation_details: serde_json::json!({ "charge_amount": expected_line_subtotal }),
                    evidence_refs: vec![EvidenceRefOutput {
                        document_id: invoice.document_id,
                        evidence_role: "invoice_source".into(),
                        page_number: line.source_page,
                        bounding_box: line.source_bbox.clone(),
                        quoted_text: Some(format!("{} = {}", line.description, expected_line_subtotal)),
                        source_location: Some(format!("invoice:line:{}", line.line_number)),
                    }],
                });
            }
            recalc_tax_total += line.tax_amount;
            continue;
        }

        // STAGE D: Line-Item Matching (Contract + PO)
        let mut matched_contract_item: Option<&ContractLineItemInput> = None;
        let mut match_method = "unmatched".to_string();
        let mut line_needs_review = false;
        let mut candidate_score: Option<Decimal> = None;

        if let Some(cv) = resolved_contract.filter(|_| contract_usable) {
            if let Some(confirmed_id) = line.human_confirmed_contract_item_id {
                matched_contract_item = cv.line_items.iter().find(|ci| ci.id == confirmed_id);
                if matched_contract_item.is_some() {
                    match_method = "human_confirmed".into();
                }
            }
            if matched_contract_item.is_none() {
                if let Some(code) = &line.item_code {
                    if let Some(ci) = cv.line_items.iter().find(|ci| {
                        ci.item_code
                            .as_ref()
                            .map(|cc| cc.eq_ignore_ascii_case(code))
                            .unwrap_or(false)
                    }) {
                        matched_contract_item = Some(ci);
                        match_method = "exact_code".into();
                    }
                }
            }
            if matched_contract_item.is_none() {
                if let Some(ci) = cv
                    .line_items
                    .iter()
                    .find(|ci| ci.description.trim().eq_ignore_ascii_case(line.description.trim()))
                {
                    matched_contract_item = Some(ci);
                    match_method = "exact_description".into();
                }
            }
            // Transparent fuzzy candidate matching — NEVER silently asserts equivalence!
            if matched_contract_item.is_none() {
                let mut best: Option<(&ContractLineItemInput, Decimal)> = None;
                for ci in &cv.line_items {
                    let score = token_jaccard_score(&line.description, &ci.description);
                    if score >= dec!(0.40) {
                        if best.map(|(_, s)| score > s).unwrap_or(true) {
                            best = Some((ci, score));
                        }
                    }
                }
                if let Some((ci, score)) = best {
                    matched_contract_item = Some(ci);
                    match_method = "fuzzy_candidate".into();
                    line_needs_review = true;
                    requires_review = true;
                    candidate_score = Some(score);
                    findings.push(FindingOutput {
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
                    });
                }
            }
        }

        // Match PO line item
        let matched_po_item: Option<&PoLineItemInput> = matched_po.and_then(|po| {
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
        });

        line_matches.push(LineMatchDecision {
            invoice_line_item_id: line.id,
            matched_contract_item_id: matched_contract_item.map(|c| c.id),
            matched_po_item_id: matched_po_item.map(|p| p.id),
            match_method: match_method.clone(),
            match_requires_review: line_needs_review,
            candidate_score,
        });

        // STAGE E: Quantity Verification (PO ordered/received & Contract min/max)
        if let Some(pi) = matched_po_item {
            let expected_qty = pi.received_quantity.unwrap_or(pi.ordered_quantity);
            let qty_var = line.quantity - expected_qty;
            let qty_tol_pct = resolved_contract.and_then(|cv| cv.quantity_tolerance_pct);
            let (permitted_label, outside_tol) = match qty_tol_pct {
                Some(pct) => {
                    let allowed = (expected_qty * (pct / dec!(100))).abs();
                    let over = (qty_var.abs() - allowed).max(Decimal::ZERO);
                    (format!("{}%", pct), Some(over))
                }
                None => ("UNDEFINED".to_string(), if qty_var != Decimal::ZERO { Some(qty_var.abs()) } else { Some(Decimal::ZERO) }),
            };
            let is_violation = outside_tol.unwrap_or(Decimal::ZERO) > Decimal::ZERO;
            stages.push(StageResult {
                stage_code: "E".into(),
                stage_name: "Quantity Verification".into(),
                status: if is_violation { "discrepancy" } else { "pass" }.into(),
                invoice_line_item_id: Some(line.id),
                contract_line_item_id: matched_contract_item.map(|c| c.id),
                expected_value: Some(expected_qty),
                actual_value: Some(line.quantity),
                variance_amount: Some(qty_var),
                variance_pct: if expected_qty != Decimal::ZERO {
                    Some(((qty_var / expected_qty) * dec!(100)).round_dp(4))
                } else {
                    None
                },
                permitted_tolerance: Some(permitted_label.clone()),
                amount_outside_tolerance: outside_tol,
                currency: Some(invoice.currency.clone()),
                rule_applied: "PO_QUANTITY_COMPARISON".into(),
                calculation_details: serde_json::json!({
                    "po_ordered_qty": pi.ordered_quantity,
                    "po_received_qty": pi.received_quantity,
                    "invoiced_qty": line.quantity,
                    "tolerance_policy": permitted_label
                }),
            });
            if is_violation {
                let financial_impact = round_for_currency(qty_var * line.unit_price, &invoice.currency);
                let (sev, sev_rule) = severity_rules.severity_for_variance(financial_impact.abs());
                findings.push(FindingOutput {
                    category: "QUANTITY_MISMATCH".into(),
                    classification_type: "confirmed_rule_violation".into(),
                    severity: sev.into(),
                    severity_rule_applied: sev_rule.into(),
                    status: "open".into(),
                    invoice_line_item_id: Some(line.id),
                    contract_id: matched_contract_id,
                    expected_value: Some(expected_qty),
                    actual_value: Some(line.quantity),
                    variance_amount: Some(financial_impact),
                    currency: Some(invoice.currency.clone()),
                    rule_applied: "PO_QUANTITY_COMPARISON".into(),
                    explanation: format!(
                        "Line {} ('{}') invoiced quantity {} exceeds/differs from PO quantity {} (variance: {} units, financial impact: {} {}, tolerance: {}).",
                        line.line_number, line.description, line.quantity, expected_qty, qty_var, financial_impact, invoice.currency, permitted_label
                    ),
                    calculation_details: serde_json::json!({
                        "expected_qty": expected_qty,
                        "actual_qty": line.quantity,
                        "qty_variance": qty_var,
                        "financial_impact": financial_impact,
                        "tolerance_policy": permitted_label
                    }),
                    evidence_refs: vec![EvidenceRefOutput {
                        document_id: invoice.document_id,
                        evidence_role: "invoice_source".into(),
                        page_number: line.source_page,
                        bounding_box: line.source_bbox.clone(),
                        quoted_text: Some(format!("Qty: {} {}", line.quantity, line.unit)),
                        source_location: Some(format!("invoice:line:{}:quantity", line.line_number)),
                    }],
                });
            }
        }

        // STAGE F: Unit-Price Verification (Contractual price + contract-specific tolerance, NEVER invented!)
        if let (Some(cv), Some(ci)) = (resolved_contract.filter(|_| contract_usable), matched_contract_item) {
            // Check unit compatibility
            let (inv_unit, _) = normalize_unit(&line.unit);
            let (ctr_unit, _) = normalize_unit(&ci.unit);
            let unit_mismatch = inv_unit != ctr_unit;

            let unit_var = line.unit_price - ci.unit_price;
            let total_price_impact = round_for_currency(unit_var * line.quantity, &invoice.currency);
            let var_pct = if ci.unit_price != Decimal::ZERO {
                Some(((unit_var / ci.unit_price) * dec!(100)).round_dp(4))
            } else {
                None
            };

            let (permitted_label, outside_tol, is_price_violation) = match (cv.price_tolerance_pct, cv.price_tolerance_abs) {
                (Some(pct), _) => {
                    let allowed_unit = (ci.unit_price * (pct / dec!(100))).abs();
                    let over_unit = (unit_var.abs() - allowed_unit).max(Decimal::ZERO);
                    (format!("{}%", pct), Some(round_for_currency(over_unit * line.quantity, &invoice.currency)), over_unit > Decimal::ZERO)
                }
                (None, Some(abs_tol)) => {
                    let over_unit = (unit_var.abs() - abs_tol.abs()).max(Decimal::ZERO);
                    (format!("{} {}", abs_tol, cv.currency), Some(round_for_currency(over_unit * line.quantity, &invoice.currency)), over_unit > Decimal::ZERO)
                }
                (None, None) => {
                    // Do NOT invent a default contractual tolerance!
                    ("UNDEFINED".to_string(), if unit_var != Decimal::ZERO { Some(total_price_impact.abs()) } else { Some(Decimal::ZERO) }, unit_var != Decimal::ZERO)
                }
            };

            stages.push(StageResult {
                stage_code: "F".into(),
                stage_name: "Unit-Price Verification".into(),
                status: if is_price_violation || unit_mismatch { "discrepancy" } else { "pass" }.into(),
                invoice_line_item_id: Some(line.id),
                contract_line_item_id: Some(ci.id),
                expected_value: Some(ci.unit_price),
                actual_value: Some(line.unit_price),
                variance_amount: Some(total_price_impact),
                variance_pct: var_pct,
                permitted_tolerance: Some(permitted_label.clone()),
                amount_outside_tolerance: outside_tol,
                currency: Some(invoice.currency.clone()),
                rule_applied: "CONTRACT_UNIT_PRICE_CHECK".into(),
                calculation_details: serde_json::json!({
                    "contract_unit_price": ci.unit_price,
                    "invoice_unit_price": line.unit_price,
                    "unit_variance": unit_var,
                    "quantity": line.quantity,
                    "total_price_impact": total_price_impact,
                    "tolerance_policy": permitted_label,
                    "unit_mismatch": unit_mismatch
                }),
            });

            if is_price_violation {
                let (sev, sev_rule) = severity_rules.severity_for_variance(total_price_impact.abs());
                let classification = if line_needs_review {
                    "unresolved_discrepancy"
                } else {
                    "confirmed_rule_violation"
                };
                findings.push(FindingOutput {
                    category: "CONTRACT_PRICE_MISMATCH".into(),
                    classification_type: classification.into(),
                    severity: sev.into(),
                    severity_rule_applied: sev_rule.into(),
                    status: if line_needs_review { "requires_review" } else { "open" }.into(),
                    invoice_line_item_id: Some(line.id),
                    contract_id: Some(cv.contract_id),
                    expected_value: Some(ci.unit_price),
                    actual_value: Some(line.unit_price),
                    variance_amount: Some(total_price_impact),
                    currency: Some(invoice.currency.clone()),
                    rule_applied: "CONTRACT_UNIT_PRICE_CHECK".into(),
                    explanation: format!(
                        "Line {} ('{}') billed unit price {} {} vs contract ({}) agreed price {} {} (unit variance: {}, total impact across qty {}: {} {}; tolerance policy: {}).",
                        line.line_number, line.description, line.unit_price, invoice.currency, cv.version_label, ci.unit_price, cv.currency, unit_var, line.quantity, total_price_impact, invoice.currency, permitted_label
                    ),
                    calculation_details: serde_json::json!({
                        "expected_unit_price": ci.unit_price,
                        "actual_unit_price": line.unit_price,
                        "quantity": line.quantity,
                        "variance_amount": total_price_impact,
                        "variance_pct": var_pct,
                        "permitted_tolerance": permitted_label,
                        "amount_outside_tolerance": outside_tol
                    }),
                    evidence_refs: vec![
                        EvidenceRefOutput {
                            document_id: invoice.document_id,
                            evidence_role: "invoice_source".into(),
                            page_number: line.source_page,
                            bounding_box: line.source_bbox.clone(),
                            quoted_text: Some(format!("{} @ {}", line.description, line.unit_price)),
                            source_location: Some(format!("invoice:line:{}:unit_price", line.line_number)),
                        },
                        EvidenceRefOutput {
                            document_id: cv.document_id,
                            evidence_role: "contract_source".into(),
                            page_number: Some(1),
                            bounding_box: None,
                            quoted_text: Some(format!("Contract {} item '{}' agreed price {}", cv.version_label, ci.description, ci.unit_price)),
                            source_location: Some(format!("contract:version:{}:item:{}", cv.version_label, ci.id)),
                        },
                    ],
                });
            }

            // STAGE G: Discount Verification
            if ci.discount_pct > Decimal::ZERO {
                let expected_disc = round_for_currency(gross * (ci.discount_pct / dec!(100)), &invoice.currency);
                let disc_var = line_discount - expected_disc;
                let disc_ok = disc_var == Decimal::ZERO;
                stages.push(StageResult {
                    stage_code: "G".into(),
                    stage_name: "Discount Verification".into(),
                    status: if disc_ok { "pass" } else { "discrepancy" }.into(),
                    invoice_line_item_id: Some(line.id),
                    contract_line_item_id: Some(ci.id),
                    expected_value: Some(expected_disc),
                    actual_value: Some(line_discount),
                    variance_amount: Some(expected_disc - line_discount),
                    variance_pct: None,
                    permitted_tolerance: Some("EXACT_CONTRACT_DISCOUNT".into()),
                    amount_outside_tolerance: Some((expected_disc - line_discount).abs()),
                    currency: Some(invoice.currency.clone()),
                    rule_applied: "CONTRACT_DISCOUNT_VERIFICATION".into(),
                    calculation_details: serde_json::json!({
                        "contract_discount_pct": ci.discount_pct,
                        "expected_discount": expected_disc,
                        "actual_discount": line_discount
                    }),
                });
                if !disc_ok {
                    let short_discount = expected_disc - line_discount;
                    let (sev, sev_rule) = severity_rules.severity_for_variance(short_discount.abs());
                    findings.push(FindingOutput {
                        category: "DISCOUNT_MISMATCH".into(),
                        classification_type: "confirmed_rule_violation".into(),
                        severity: sev.into(),
                        severity_rule_applied: sev_rule.into(),
                        status: "open".into(),
                        invoice_line_item_id: Some(line.id),
                        contract_id: Some(cv.contract_id),
                        expected_value: Some(expected_disc),
                        actual_value: Some(line_discount),
                        variance_amount: Some(short_discount),
                        currency: Some(invoice.currency.clone()),
                        rule_applied: "CONTRACT_DISCOUNT_VERIFICATION".into(),
                        explanation: format!(
                            "Line {} ('{}') applied discount {} {} instead of contractual {}% discount ({} {}).",
                            line.line_number, line.description, line_discount, invoice.currency, ci.discount_pct, expected_disc, invoice.currency
                        ),
                        calculation_details: serde_json::json!({
                            "contract_discount_pct": ci.discount_pct,
                            "expected_discount": expected_disc,
                            "actual_discount": line_discount
                        }),
                        evidence_refs: vec![EvidenceRefOutput {
                            document_id: invoice.document_id,
                            evidence_role: "invoice_source".into(),
                            page_number: line.source_page,
                            bounding_box: line.source_bbox.clone(),
                            quoted_text: Some(format!("Discount: {}", line_discount)),
                            source_location: Some(format!("invoice:line:{}:discount", line.line_number)),
                        }],
                    });
                }
            }

            // STAGE H: Tax Verification (ONLY against documented contract/line tax rate — never hardcoded!)
            let applicable_tax_rate = ci.tax_rate.or(cv.default_tax_rate).or(line.tax_rate);
            if let Some(tax_rate) = applicable_tax_rate {
                let expected_tax = round_for_currency(
                    expected_line_subtotal * (tax_rate / dec!(100)),
                    &invoice.currency,
                );
                recalc_tax_total += expected_tax;
                let tax_var = line.tax_amount - expected_tax;
                stages.push(StageResult {
                    stage_code: "H".into(),
                    stage_name: "Tax Verification".into(),
                    status: if tax_var == Decimal::ZERO { "pass" } else { "discrepancy" }.into(),
                    invoice_line_item_id: Some(line.id),
                    contract_line_item_id: Some(ci.id),
                    expected_value: Some(expected_tax),
                    actual_value: Some(line.tax_amount),
                    variance_amount: Some(tax_var),
                    variance_pct: None,
                    permitted_tolerance: Some("DOCUMENTED_TAX_RATE_EXACT".into()),
                    amount_outside_tolerance: Some(tax_var.abs()),
                    currency: Some(invoice.currency.clone()),
                    rule_applied: "DOCUMENTED_CONTRACT_TAX_RULE".into(),
                    calculation_details: serde_json::json!({
                        "taxable_base": expected_line_subtotal,
                        "documented_tax_rate_pct": tax_rate,
                        "expected_tax": expected_tax,
                        "actual_tax": line.tax_amount
                    }),
                });
                if tax_var != Decimal::ZERO {
                    let (sev, sev_rule) = severity_rules.severity_for_variance(tax_var.abs());
                    findings.push(FindingOutput {
                        category: "TAX_MISMATCH".into(),
                        classification_type: "confirmed_rule_violation".into(),
                        severity: sev.into(),
                        severity_rule_applied: sev_rule.into(),
                        status: "open".into(),
                        invoice_line_item_id: Some(line.id),
                        contract_id: Some(cv.contract_id),
                        expected_value: Some(expected_tax),
                        actual_value: Some(line.tax_amount),
                        variance_amount: Some(tax_var),
                        currency: Some(invoice.currency.clone()),
                        rule_applied: "DOCUMENTED_CONTRACT_TAX_RULE".into(),
                        explanation: format!(
                            "Line {} ('{}') billed tax {} {} vs documented {}% tax rule ({} {}).",
                            line.line_number, line.description, line.tax_amount, invoice.currency, tax_rate, expected_tax, invoice.currency
                        ),
                        calculation_details: serde_json::json!({
                            "documented_tax_rate_pct": tax_rate,
                            "expected_tax": expected_tax,
                            "actual_tax": line.tax_amount,
                            "tax_variance": tax_var
                        }),
                        evidence_refs: vec![EvidenceRefOutput {
                            document_id: invoice.document_id,
                            evidence_role: "invoice_source".into(),
                            page_number: line.source_page,
                            bounding_box: line.source_bbox.clone(),
                            quoted_text: Some(format!("Tax: {}", line.tax_amount)),
                            source_location: Some(format!("invoice:line:{}:tax", line.line_number)),
                        }],
                    });
                }
            } else {
                recalc_tax_total += line.tax_amount;
            }
        } else {
            recalc_tax_total += line.tax_amount;
        }
    }

    // If invoice has header-level tax and line items had zero line-level tax, validate against contract default_tax_rate if documented
    if recalc_tax_total == Decimal::ZERO && invoice.stated_tax_total != Decimal::ZERO {
        if let Some(tax_rate) = resolved_contract.filter(|_| contract_usable).and_then(|cv| cv.default_tax_rate) {
            let expected_header_tax = round_for_currency(
                (recalc_subtotal + recalc_additional_charges) * (tax_rate / dec!(100)),
                &invoice.currency,
            );
            recalc_tax_total = expected_header_tax;
            let tax_var = invoice.stated_tax_total - expected_header_tax;
            if tax_var != Decimal::ZERO {
                let (sev, sev_rule) = severity_rules.severity_for_variance(tax_var.abs());
                findings.push(FindingOutput {
                    category: "TAX_MISMATCH".into(),
                    classification_type: "confirmed_rule_violation".into(),
                    severity: sev.into(),
                    severity_rule_applied: sev_rule.into(),
                    status: "open".into(),
                    invoice_line_item_id: None,
                    contract_id: matched_contract_id,
                    expected_value: Some(expected_header_tax),
                    actual_value: Some(invoice.stated_tax_total),
                    variance_amount: Some(tax_var),
                    currency: Some(invoice.currency.clone()),
                    rule_applied: "HEADER_TAX_DOCUMENTED_CONTRACT_RULE".into(),
                    explanation: format!(
                        "Invoice header tax {} {} differs from documented {}% contract tax rule ({} {}).",
                        invoice.stated_tax_total, invoice.currency, tax_rate, expected_header_tax, invoice.currency
                    ),
                    calculation_details: serde_json::json!({
                        "taxable_base": recalc_subtotal + recalc_additional_charges,
                        "documented_tax_rate_pct": tax_rate,
                        "expected_tax": expected_header_tax,
                        "stated_tax": invoice.stated_tax_total
                    }),
                    evidence_refs: vec![EvidenceRefOutput {
                        document_id: invoice.document_id,
                        evidence_role: "invoice_source".into(),
                        page_number: Some(1),
                        bounding_box: None,
                        quoted_text: Some(format!("Tax: {}", invoice.stated_tax_total)),
                        source_location: Some("invoice:header:tax".into()),
                    }],
                });
            }
        } else {
            recalc_tax_total = invoice.stated_tax_total;
        }
    }

    // -------------------------------------------------------------------------
    // STAGE I: Subtotal & Grand Total Verification (Deterministic Decimal)
    // -------------------------------------------------------------------------
    let recalc_total = round_for_currency(
        recalc_subtotal + recalc_additional_charges + recalc_tax_total,
        &invoice.currency,
    );
    let total_var = invoice.stated_total - recalc_total;
    stages.push(StageResult {
        stage_code: "I".into(),
        stage_name: "Subtotal and Total Verification".into(),
        status: if total_var == Decimal::ZERO { "pass" } else { "discrepancy" }.into(),
        invoice_line_item_id: None,
        contract_line_item_id: None,
        expected_value: Some(recalc_total),
        actual_value: Some(invoice.stated_total),
        variance_amount: Some(total_var),
        variance_pct: if recalc_total != Decimal::ZERO {
            Some(((total_var / recalc_total) * dec!(100)).round_dp(4))
        } else {
            None
        },
        permitted_tolerance: Some("EXACT_DECIMAL_ARITHMETIC".into()),
        amount_outside_tolerance: Some(total_var.abs()),
        currency: Some(invoice.currency.clone()),
        rule_applied: "DETERMINISTIC_GRAND_TOTAL_RECALCULATION".into(),
        calculation_details: serde_json::json!({
            "recalculated_subtotal": recalc_subtotal,
            "recalculated_additional_charges": recalc_additional_charges,
            "recalculated_tax_total": recalc_tax_total,
            "recalculated_total": recalc_total,
            "stated_total": invoice.stated_total
        }),
    });

    if total_var != Decimal::ZERO {
        let (sev, sev_rule) = severity_rules.severity_for_variance(total_var.abs());
        findings.push(FindingOutput {
            category: "TOTAL_ARITHMETIC_MISMATCH".into(),
            classification_type: "confirmed_rule_violation".into(),
            severity: sev.into(),
            severity_rule_applied: sev_rule.into(),
            status: "open".into(),
            invoice_line_item_id: None,
            contract_id: matched_contract_id,
            expected_value: Some(recalc_total),
            actual_value: Some(invoice.stated_total),
            variance_amount: Some(total_var),
            currency: Some(invoice.currency.clone()),
            rule_applied: "DETERMINISTIC_GRAND_TOTAL_RECALCULATION".into(),
            explanation: format!(
                "Invoice {} stated total {} {} does not match deterministic recalculated total {} {} (variance: {}).",
                invoice.invoice_number, invoice.stated_total, invoice.currency, recalc_total, invoice.currency, total_var
            ),
            calculation_details: serde_json::json!({
                "recalculated_subtotal": recalc_subtotal,
                "recalculated_additional_charges": recalc_additional_charges,
                "recalculated_tax_total": recalc_tax_total,
                "recalculated_total": recalc_total,
                "stated_total": invoice.stated_total
            }),
            evidence_refs: vec![EvidenceRefOutput {
                document_id: invoice.document_id,
                evidence_role: "invoice_source".into(),
                page_number: Some(1),
                bounding_box: None,
                quoted_text: Some(format!("Total: {} {}", invoice.stated_total, invoice.currency)),
                source_location: Some("invoice:header:total".into()),
            }],
        });
    }

    // -------------------------------------------------------------------------
    // STAGE K: Payment Reconciliation (Explicit Evidence Only!)
    // -------------------------------------------------------------------------
    let mut total_paid = Decimal::ZERO;
    let mut matched_payment_refs: Vec<String> = Vec::new();
    let mut same_amount_same_date_seen: HashMap<(NaiveDate, Decimal), String> = HashMap::new();

    for p in payments {
        if p.vendor_id != invoice.vendor_id || p.transaction_status == "failed" || p.transaction_status == "reversed" {
            continue;
        }
        // Require explicit reference OR explicit human/allocation table mapping — NEVER match solely by equal amount!
        let explicitly_linked = p.explicit_allocated_to_this_invoice.is_some()
            || p.referenced_invoice_number
                .as_ref()
                .map(|r| r.eq_ignore_ascii_case(&invoice.invoice_number))
                .unwrap_or(false);

        if !explicitly_linked {
            continue;
        }

        if !p.currency.eq_ignore_ascii_case(&invoice.currency) {
            findings.push(FindingOutput {
                category: "CURRENCY_MISMATCH".into(),
                classification_type: "confirmed_rule_violation".into(),
                severity: severity_rules.currency_mismatch_severity.clone(),
                severity_rule_applied: "SEVERITY_RULE_PAYMENT_CURRENCY_MISMATCH".into(),
                status: "open".into(),
                invoice_line_item_id: None,
                contract_id: matched_contract_id,
                expected_value: Some(invoice.stated_total),
                actual_value: Some(p.amount),
                variance_amount: None,
                currency: Some(p.currency.clone()),
                rule_applied: "PAYMENT_CURRENCY_MUST_MATCH_INVOICE".into(),
                explanation: format!(
                    "Payment {} currency '{}' does not match invoice {} currency '{}'. No FX conversion invented.",
                    p.payment_reference, p.currency, invoice.invoice_number, invoice.currency
                ),
                calculation_details: serde_json::json!({
                    "payment_reference": p.payment_reference,
                    "payment_currency": p.currency,
                    "invoice_currency": invoice.currency
                }),
                evidence_refs: vec![EvidenceRefOutput {
                    document_id: p.document_id,
                    evidence_role: "payment_source".into(),
                    page_number: Some(1),
                    bounding_box: None,
                    quoted_text: Some(format!("Payment {} {} {}", p.payment_reference, p.amount, p.currency)),
                    source_location: Some("payment:currency".into()),
                }],
            });
            continue;
        }

        let effective_amt = p.explicit_allocated_to_this_invoice.unwrap_or(p.amount);
        let signed_amt = if p.is_refund_or_credit { -effective_amt } else { effective_amt };

        // Check potential duplicate payment
        if !p.is_refund_or_credit {
            if let Some(prev_ref) = same_amount_same_date_seen.get(&(p.payment_date, effective_amt)) {
                findings.push(FindingOutput {
                    category: "DUPLICATE_PAYMENT_CANDIDATE".into(),
                    classification_type: "probable_duplicate".into(),
                    severity: "critical".into(),
                    severity_rule_applied: "SEVERITY_RULE_DUPLICATE_PAYMENT".into(),
                    status: "requires_review".into(),
                    invoice_line_item_id: None,
                    contract_id: matched_contract_id,
                    expected_value: Some(invoice.stated_total),
                    actual_value: Some(effective_amt),
                    variance_amount: Some(effective_amt),
                    currency: Some(invoice.currency.clone()),
                    rule_applied: "DUPLICATE_PAYMENT_SAME_INVOICE_DATE_AMOUNT".into(),
                    explanation: format!(
                        "Payment {} ({} {} on {}) matches prior payment {} for invoice {}. Potential duplicate payment requires investigation.",
                        p.payment_reference, effective_amt, p.currency, p.payment_date, prev_ref, invoice.invoice_number
                    ),
                    calculation_details: serde_json::json!({
                        "payment_reference": p.payment_reference,
                        "prior_payment_reference": prev_ref,
                        "amount": effective_amt
                    }),
                    evidence_refs: vec![EvidenceRefOutput {
                        document_id: p.document_id,
                        evidence_role: "payment_source".into(),
                        page_number: Some(1),
                        bounding_box: None,
                        quoted_text: Some(format!("Payment {} = {}", p.payment_reference, effective_amt)),
                        source_location: Some("payment:duplicate_check".into()),
                    }],
                });
                requires_review = true;
            } else {
                same_amount_same_date_seen.insert((p.payment_date, effective_amt), p.payment_reference.clone());
            }
        }

        total_paid += signed_amt;
        matched_payment_refs.push(p.payment_reference.clone());
    }

    let payment_var = total_paid - invoice.stated_total;
    let payment_status = if total_paid == Decimal::ZERO {
        "unpaid"
    } else if total_paid < invoice.stated_total {
        "partially_paid"
    } else if total_paid == invoice.stated_total {
        "fully_paid"
    } else {
        "overpaid"
    };

    stages.push(StageResult {
        stage_code: "K".into(),
        stage_name: "Payment Reconciliation".into(),
        status: if payment_status == "overpaid" { "discrepancy" } else { "pass" }.into(),
        invoice_line_item_id: None,
        contract_line_item_id: None,
        expected_value: Some(invoice.stated_total),
        actual_value: Some(total_paid),
        variance_amount: Some(payment_var),
        variance_pct: None,
        permitted_tolerance: Some("EXACT_PAYMENT_ALLOCATION".into()),
        amount_outside_tolerance: if payment_var > Decimal::ZERO { Some(payment_var) } else { Some(Decimal::ZERO) },
        currency: Some(invoice.currency.clone()),
        rule_applied: "EXPLICIT_EVIDENCE_PAYMENT_RECONCILIATION".into(),
        calculation_details: serde_json::json!({
            "payment_status": payment_status,
            "matched_payment_refs": matched_payment_refs,
            "total_paid": total_paid,
            "invoice_total": invoice.stated_total
        }),
    });

    if payment_status == "overpaid" {
        let (sev, sev_rule) = severity_rules.severity_for_variance(payment_var.abs());
        findings.push(FindingOutput {
            category: "PAYMENT_MISMATCH".into(),
            classification_type: "confirmed_rule_violation".into(),
            severity: sev.into(),
            severity_rule_applied: sev_rule.into(),
            status: "open".into(),
            invoice_line_item_id: None,
            contract_id: matched_contract_id,
            expected_value: Some(invoice.stated_total),
            actual_value: Some(total_paid),
            variance_amount: Some(payment_var),
            currency: Some(invoice.currency.clone()),
            rule_applied: "OVERPAYMENT_DETECTION".into(),
            explanation: format!(
                "Invoice {} is overpaid: total explicit payments {} {} exceed invoice total {} {} by {} {} (payments: {}).",
                invoice.invoice_number, total_paid, invoice.currency, invoice.stated_total, invoice.currency, payment_var, invoice.currency, matched_payment_refs.join(", ")
            ),
            calculation_details: serde_json::json!({
                "total_paid": total_paid,
                "invoice_total": invoice.stated_total,
                "overpayment": payment_var,
                "payment_refs": matched_payment_refs
            }),
            evidence_refs: vec![EvidenceRefOutput {
                document_id: invoice.document_id,
                evidence_role: "invoice_source".into(),
                page_number: Some(1),
                bounding_box: None,
                quoted_text: Some(format!("Invoice {} Total {}", invoice.invoice_number, invoice.stated_total)),
                source_location: Some("invoice:header:total".into()),
            }],
        });
    }

    // -------------------------------------------------------------------------
    // STAGE L: Duplicate & Anomaly Detection
    // -------------------------------------------------------------------------
    let mut dup_found = false;
    for other in existing_invoices {
        if other.id == invoice.id {
            continue;
        }
        // 1. Exact document SHA-256 duplicate
        if let (Some(h1), Some(h2)) = (invoice_sha256, other.sha256_hash.as_deref()) {
            if h1 == h2 {
                dup_found = true;
                findings.push(FindingOutput {
                    category: "DUPLICATE_INVOICE".into(),
                    classification_type: "exact_duplicate".into(),
                    severity: severity_rules.duplicate_invoice_severity.clone(),
                    severity_rule_applied: "SEVERITY_RULE_EXACT_SHA256_DUPLICATE".into(),
                    status: "open".into(),
                    invoice_line_item_id: None,
                    contract_id: matched_contract_id,
                    expected_value: Some(Decimal::ZERO),
                    actual_value: Some(invoice.stated_total),
                    variance_amount: Some(invoice.stated_total),
                    currency: Some(invoice.currency.clone()),
                    rule_applied: "EXACT_DOCUMENT_SHA256_DUPLICATE".into(),
                    explanation: format!(
                        "Invoice {} has identical SHA-256 document fingerprint to existing invoice {} (ID {}).",
                        invoice.invoice_number, other.invoice_number, other.id
                    ),
                    calculation_details: serde_json::json!({ "duplicate_invoice_id": other.id, "sha256": h1 }),
                    evidence_refs: vec![
                        EvidenceRefOutput {
                            document_id: invoice.document_id,
                            evidence_role: "invoice_source".into(),
                            page_number: Some(1),
                            bounding_box: None,
                            quoted_text: Some(format!("SHA-256: {}", h1)),
                            source_location: Some("document:sha256".into()),
                        },
                        EvidenceRefOutput {
                            document_id: other.document_id,
                            evidence_role: "duplicate_source".into(),
                            page_number: Some(1),
                            bounding_box: None,
                            quoted_text: Some(format!("Original Invoice {}", other.invoice_number)),
                            source_location: Some("document:sha256".into()),
                        },
                    ],
                });
                continue;
            }
        }
        // 2. Duplicate invoice number for same vendor
        if other.vendor_id == invoice.vendor_id
            && other.invoice_number.eq_ignore_ascii_case(&invoice.invoice_number)
        {
            dup_found = true;
            findings.push(FindingOutput {
                category: "DUPLICATE_INVOICE".into(),
                classification_type: "exact_duplicate".into(),
                severity: severity_rules.duplicate_invoice_severity.clone(),
                severity_rule_applied: "SEVERITY_RULE_DUPLICATE_INVOICE_NUMBER".into(),
                status: "open".into(),
                invoice_line_item_id: None,
                contract_id: matched_contract_id,
                expected_value: Some(Decimal::ZERO),
                actual_value: Some(invoice.stated_total),
                variance_amount: Some(invoice.stated_total),
                currency: Some(invoice.currency.clone()),
                rule_applied: "DUPLICATE_VENDOR_INVOICE_NUMBER".into(),
                explanation: format!(
                    "Duplicate invoice number '{}' detected for the same vendor (existing invoice ID {}, dated {}, total {} {}).",
                    invoice.invoice_number, other.id, other.invoice_date, other.stated_total, other.currency
                ),
                calculation_details: serde_json::json!({ "existing_invoice_id": other.id, "existing_total": other.stated_total }),
                evidence_refs: vec![
                    EvidenceRefOutput {
                        document_id: invoice.document_id,
                        evidence_role: "invoice_source".into(),
                        page_number: Some(1),
                        bounding_box: None,
                        quoted_text: Some(format!("Invoice #: {}", invoice.invoice_number)),
                        source_location: Some("invoice:header:invoice_number".into()),
                    },
                    EvidenceRefOutput {
                        document_id: other.document_id,
                        evidence_role: "duplicate_source".into(),
                        page_number: Some(1),
                        bounding_box: None,
                        quoted_text: Some(format!("Prior Invoice #: {}", other.invoice_number)),
                        source_location: Some("invoice:header:invoice_number".into()),
                    },
                ],
            });
            continue;
        }
        // 3. Probable duplicate: same vendor, same currency, identical total, within 7 days
        let day_diff = (invoice.invoice_date - other.invoice_date).num_days().abs();
        if other.vendor_id == invoice.vendor_id
            && other.currency.eq_ignore_ascii_case(&invoice.currency)
            && other.stated_total == invoice.stated_total
            && day_diff <= 7
        {
            dup_found = true;
            requires_review = true;
            findings.push(FindingOutput {
                category: "DUPLICATE_INVOICE".into(),
                classification_type: "probable_duplicate".into(),
                severity: "high".into(),
                severity_rule_applied: "SEVERITY_RULE_PROBABLE_DUPLICATE".into(),
                status: "requires_review".into(),
                invoice_line_item_id: None,
                contract_id: matched_contract_id,
                expected_value: Some(Decimal::ZERO),
                actual_value: Some(invoice.stated_total),
                variance_amount: Some(invoice.stated_total),
                currency: Some(invoice.currency.clone()),
                rule_applied: "PROBABLE_DUPLICATE_SAME_VENDOR_AMOUNT_WINDOW_7D".into(),
                explanation: format!(
                    "Probable duplicate: invoice {} ({} {} on {}) matches vendor, currency, and exact amount of invoice {} ({} days apart). Not confirmed fraud; requires human review.",
                    invoice.invoice_number, invoice.stated_total, invoice.currency, invoice.invoice_date, other.invoice_number, day_diff
                ),
                calculation_details: serde_json::json!({ "similar_invoice_id": other.id, "days_apart": day_diff }),
                evidence_refs: vec![EvidenceRefOutput {
                    document_id: other.document_id,
                    evidence_role: "duplicate_source".into(),
                    page_number: Some(1),
                    bounding_box: None,
                    quoted_text: Some(format!("Similar invoice {} ({} {})", other.invoice_number, other.stated_total, other.currency)),
                    source_location: Some("invoice:header".into()),
                }],
            });
        }
    }

    stages.push(StageResult {
        stage_code: "L".into(),
        stage_name: "Duplicate and Anomaly Detection".into(),
        status: if dup_found { "discrepancy" } else { "pass" }.into(),
        invoice_line_item_id: None,
        contract_line_item_id: None,
        expected_value: None,
        actual_value: None,
        variance_amount: None,
        variance_pct: None,
        permitted_tolerance: None,
        amount_outside_tolerance: None,
        currency: Some(invoice.currency.clone()),
        rule_applied: "DUPLICATE_FINGERPRINT_NUMBER_AND_WINDOW_CHECK".into(),
        calculation_details: serde_json::json!({ "duplicates_or_candidates_detected": dup_found }),
    });

    let has_confirmed = findings
        .iter()
        .any(|f| f.classification_type == "confirmed_rule_violation" || f.classification_type == "exact_duplicate");

    let overall_status = if has_confirmed {
        "discrepancy_confirmed"
    } else if requires_review || !findings.is_empty() {
        "requires_review"
    } else {
        "reconciled"
    };

    ReconciliationOutput {
        engine_version: RECONCILIATION_ENGINE_VERSION.into(),
        overall_status: overall_status.into(),
        matched_contract_id,
        matched_contract_version_id,
        matched_po_id: matched_po.map(|po| po.id),
        match_confidence_status: match_status,
        recalculated_subtotal: recalc_subtotal,
        recalculated_discount_total: recalc_discount_total,
        recalculated_tax_total: recalc_tax_total,
        recalculated_additional_charges: recalc_additional_charges,
        recalculated_total: recalc_total,
        total_allocated_payments: total_paid,
        payment_status: payment_status.into(),
        line_matches,
        stages,
        findings,
    }
}
