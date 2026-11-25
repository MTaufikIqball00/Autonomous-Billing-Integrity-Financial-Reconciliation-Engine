pub mod duplicates;
pub mod matching;
pub mod payments;
pub mod types;

pub use matching::resolve_effective_contract_version;
pub use types::*;

use crate::normalization::{normalize_unit, round_for_currency};
use duplicates::detect_duplicate_invoices;
use matching::{match_line_to_contract, match_line_to_po};
use payments::reconcile_payments;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashMap;
use uuid::Uuid;

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

    // STAGE A & B: Document Relationship Matching & Contract Version Resolution
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

    // STAGE C: Purchase Order Matching
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

    // STAGES D, E, F, G, H, J: Line-Item Matching & Deterministic Validation
    let mut recalc_subtotal = Decimal::ZERO;
    let mut recalc_discount_total = Decimal::ZERO;
    let mut recalc_tax_total = Decimal::ZERO;
    let mut recalc_additional_charges = Decimal::ZERO;
    let mut seen_line_keys: HashMap<String, i32> = HashMap::new();

    for line in &invoice.line_items {
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

        let contract_match = match_line_to_contract(
            line,
            invoice,
            resolved_contract.filter(|_| contract_usable),
            matched_contract_id,
            severity_rules,
        );
        if contract_match.requires_review {
            requires_review = true;
        }
        if let Some(amb_finding) = contract_match.ambiguity_finding {
            findings.push(amb_finding);
        }
        let matched_contract_item = contract_match.contract_item;
        let line_needs_review = contract_match.requires_review;
        let matched_po_item = match_line_to_po(line, matched_po);

        line_matches.push(LineMatchDecision {
            invoice_line_item_id: line.id,
            matched_contract_item_id: matched_contract_item.map(|c| c.id),
            matched_po_item_id: matched_po_item.map(|p| p.id),
            match_method: contract_match.match_method,
            match_requires_review: line_needs_review,
            candidate_score: contract_match.candidate_score,
        });

        // STAGE E: Quantity Verification
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
                None => (
                    "UNDEFINED".to_string(),
                    if qty_var != Decimal::ZERO {
                        Some(qty_var.abs())
                    } else {
                        Some(Decimal::ZERO)
                    },
                ),
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

        // STAGE F, G, H: Price, Discount, Tax Verification
        if let (Some(cv), Some(ci)) = (resolved_contract.filter(|_| contract_usable), matched_contract_item) {
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

            let (permitted_label, outside_tol, is_price_violation) =
                match (cv.price_tolerance_pct, cv.price_tolerance_abs) {
                    (Some(pct), _) => {
                        let allowed_unit = (ci.unit_price * (pct / dec!(100))).abs();
                        let over_unit = (unit_var.abs() - allowed_unit).max(Decimal::ZERO);
                        (
                            format!("{}%", pct),
                            Some(round_for_currency(over_unit * line.quantity, &invoice.currency)),
                            over_unit > Decimal::ZERO,
                        )
                    }
                    (None, Some(abs_tol)) => {
                        let over_unit = (unit_var.abs() - abs_tol.abs()).max(Decimal::ZERO);
                        (
                            format!("{} {}", abs_tol, cv.currency),
                            Some(round_for_currency(over_unit * line.quantity, &invoice.currency)),
                            over_unit > Decimal::ZERO,
                        )
                    }
                    (None, None) => (
                        "UNDEFINED".to_string(),
                        if unit_var != Decimal::ZERO {
                            Some(total_price_impact.abs())
                        } else {
                            Some(Decimal::ZERO)
                        },
                        unit_var != Decimal::ZERO,
                    ),
                };

            stages.push(StageResult {
                stage_code: "F".into(),
                stage_name: "Unit-Price Verification".into(),
                status: if is_price_violation || unit_mismatch {
                    "discrepancy"
                } else {
                    "pass"
                }
                .into(),
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

            if ci.discount_pct > Decimal::ZERO {
                let expected_disc =
                    round_for_currency(gross * (ci.discount_pct / dec!(100)), &invoice.currency);
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
                    status: if tax_var == Decimal::ZERO {
                        "pass"
                    } else {
                        "discrepancy"
                    }
                    .into(),
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

    if recalc_tax_total == Decimal::ZERO && invoice.stated_tax_total != Decimal::ZERO {
        if let Some(tax_rate) = resolved_contract
            .filter(|_| contract_usable)
            .and_then(|cv| cv.default_tax_rate)
        {
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

    // STAGE I: Subtotal & Grand Total Verification
    let recalc_total = round_for_currency(
        recalc_subtotal + recalc_additional_charges + recalc_tax_total,
        &invoice.currency,
    );
    let total_var = invoice.stated_total - recalc_total;
    stages.push(StageResult {
        stage_code: "I".into(),
        stage_name: "Subtotal and Total Verification".into(),
        status: if total_var == Decimal::ZERO {
            "pass"
        } else {
            "discrepancy"
        }
        .into(),
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

    // STAGE K: Payment Reconciliation
    let pay_summary = reconcile_payments(invoice, matched_contract_id, payments, severity_rules);
    if pay_summary.requires_review {
        requires_review = true;
    }
    stages.push(pay_summary.stage);
    findings.extend(pay_summary.findings);

    // STAGE L: Duplicate & Anomaly Detection
    let dup_summary = detect_duplicate_invoices(
        invoice,
        invoice_sha256,
        matched_contract_id,
        existing_invoices,
        severity_rules,
    );
    if dup_summary.requires_review {
        requires_review = true;
    }
    stages.push(dup_summary.stage);
    findings.extend(dup_summary.findings);

    let has_confirmed = findings.iter().any(|f| {
        f.classification_type == "confirmed_rule_violation"
            || f.classification_type == "exact_duplicate"
    });

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
        total_allocated_payments: pay_summary.total_paid,
        payment_status: pay_summary.payment_status,
        line_matches,
        stages,
        findings,
    }
}
