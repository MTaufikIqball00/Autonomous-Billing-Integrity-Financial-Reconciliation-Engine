use super::types::{
    EvidenceRefOutput, ExistingInvoiceSummary, FindingOutput, InvoiceInput, SeverityRules,
    StageResult,
};
use rust_decimal::Decimal;
use uuid::Uuid;

pub struct DuplicateCheckSummary {
    pub requires_review: bool,
    pub stage: StageResult,
    pub findings: Vec<FindingOutput>,
}

/// Stage L: Exact SHA-256 fingerprint, duplicate invoice number, and 7-day probable duplicate check.
pub fn detect_duplicate_invoices(
    invoice: &InvoiceInput,
    invoice_sha256: Option<&str>,
    matched_contract_id: Option<Uuid>,
    existing_invoices: &[ExistingInvoiceSummary],
    severity_rules: &SeverityRules,
) -> DuplicateCheckSummary {
    let mut dup_found = false;
    let mut requires_review = false;
    let mut findings = Vec::new();

    for other in existing_invoices {
        if other.id == invoice.id {
            continue;
        }
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

    let stage = StageResult {
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
    };

    DuplicateCheckSummary {
        requires_review,
        stage,
        findings,
    }
}
