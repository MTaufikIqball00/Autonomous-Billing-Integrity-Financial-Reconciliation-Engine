use super::types::{
    EvidenceRefOutput, FindingOutput, InvoiceInput, PaymentInput, SeverityRules, StageResult,
};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use std::collections::HashMap;
use uuid::Uuid;

pub struct PaymentReconciliationSummary {
    pub total_paid: Decimal,
    pub payment_status: String,
    pub requires_review: bool,
    pub stage: StageResult,
    pub findings: Vec<FindingOutput>,
}

/// Stage K: Payment reconciliation using explicit evidence only (never equal-amount guessing).
pub fn reconcile_payments(
    invoice: &InvoiceInput,
    matched_contract_id: Option<Uuid>,
    payments: &[PaymentInput],
    severity_rules: &SeverityRules,
) -> PaymentReconciliationSummary {
    let mut total_paid = Decimal::ZERO;
    let mut matched_payment_refs: Vec<String> = Vec::new();
    let mut same_amount_same_date_seen: HashMap<(NaiveDate, Decimal), String> = HashMap::new();
    let mut findings: Vec<FindingOutput> = Vec::new();
    let mut requires_review = false;

    for p in payments {
        if p.vendor_id != invoice.vendor_id
            || p.transaction_status == "failed"
            || p.transaction_status == "reversed"
        {
            continue;
        }
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
        let signed_amt = if p.is_refund_or_credit {
            -effective_amt
        } else {
            effective_amt
        };

        if !p.is_refund_or_credit {
            if let Some(prev_ref) = same_amount_same_date_seen.get(&(p.payment_date, effective_amt))
            {
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
                same_amount_same_date_seen
                    .insert((p.payment_date, effective_amt), p.payment_reference.clone());
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

    let stage = StageResult {
        stage_code: "K".into(),
        stage_name: "Payment Reconciliation".into(),
        status: if payment_status == "overpaid" {
            "discrepancy"
        } else {
            "pass"
        }
        .into(),
        invoice_line_item_id: None,
        contract_line_item_id: None,
        expected_value: Some(invoice.stated_total),
        actual_value: Some(total_paid),
        variance_amount: Some(payment_var),
        variance_pct: None,
        permitted_tolerance: Some("EXACT_PAYMENT_ALLOCATION".into()),
        amount_outside_tolerance: if payment_var > Decimal::ZERO {
            Some(payment_var)
        } else {
            Some(Decimal::ZERO)
        },
        currency: Some(invoice.currency.clone()),
        rule_applied: "EXPLICIT_EVIDENCE_PAYMENT_RECONCILIATION".into(),
        calculation_details: serde_json::json!({
            "payment_status": payment_status,
            "matched_payment_refs": matched_payment_refs,
            "total_paid": total_paid,
            "invoice_total": invoice.stated_total
        }),
    };

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

    PaymentReconciliationSummary {
        total_paid,
        payment_status: payment_status.into(),
        requires_review,
        stage,
        findings,
    }
}
