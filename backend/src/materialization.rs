use crate::{
    agents::run_invoice_reconciliation_workflow,
    error::{AppError, AppResult},
    normalization::{normalize_date, normalize_money, normalize_percentage, normalize_unit, NormalizationOutcome},
    state::AppState,
};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterializedLineDraft {
    pub line_number: i32,
    pub item_code: Option<String>,
    pub description: String,
    pub quantity: Option<Decimal>,
    pub unit: String,
    pub unit_price: Option<Decimal>,
    pub discount_pct: Decimal,
    pub tax_rate: Option<Decimal>,
    pub tax_amount: Decimal,
    pub line_total: Option<Decimal>,
    pub page_number: Option<i32>,
    pub bounding_box: Option<serde_json::Value>,
    pub ambiguous: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterializationPreview {
    pub document_id: Uuid,
    pub effective_category: String,
    pub already_materialized: bool,
    pub materialized_entity_type: Option<String>,
    pub materialized_entity_id: Option<Uuid>,
    pub ready_for_auto_materialize: bool,
    pub missing_or_ambiguous_fields: Vec<String>,
    pub matched_vendor_id: Option<Uuid>,
    pub matched_vendor_name: Option<String>,
    pub draft: serde_json::Value,
    pub line_items: Vec<MaterializedLineDraft>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct MaterializeLineOverride {
    pub item_code: Option<String>,
    pub description: String,
    pub quantity: Decimal,
    pub unit: String,
    pub unit_price: Decimal,
    pub discount_pct: Option<Decimal>,
    pub discount_amount: Option<Decimal>,
    pub tax_rate: Option<Decimal>,
    pub tax_amount: Option<Decimal>,
    pub stated_line_subtotal: Option<Decimal>,
    pub stated_line_total: Decimal,
    pub is_additional_charge: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct MaterializeRequest {
    pub entity_type: Option<String>,
    pub vendor_id: Option<Uuid>,
    pub document_number: Option<String>,
    pub document_date: Option<NaiveDate>,
    pub effective_date: Option<NaiveDate>,
    pub expiration_date: Option<NaiveDate>,
    pub currency: Option<String>,
    pub contract_number: Option<String>,
    pub po_number: Option<String>,
    pub payment_terms: Option<String>,
    pub subtotal: Option<Decimal>,
    pub discount_total: Option<Decimal>,
    pub tax_total: Option<Decimal>,
    pub additional_charges: Option<Decimal>,
    pub total_amount: Option<Decimal>,
    pub related_invoice_number: Option<String>,
    pub bank_reference: Option<String>,
    pub title: Option<String>,
    pub line_items: Option<Vec<MaterializeLineOverride>>,
    pub auto_reconcile: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MaterializationResult {
    pub document_id: Uuid,
    pub entity_type: String,
    pub entity_id: Uuid,
    pub reconciliation_job_id: Option<Uuid>,
    pub reconciliation_status: Option<String>,
}

fn parse_dec_from_field(
    raw: Option<&str>,
    default_currency: Option<&str>,
    field_label: &str,
    issues: &mut Vec<String>,
    required: bool,
) -> Option<Decimal> {
    match raw {
        Some(s) if !s.trim().is_empty() => match normalize_money(s, default_currency) {
            NormalizationOutcome::Normalized { value, .. } => Some(value.amount),
            NormalizationOutcome::Ambiguous { reason, .. } => {
                issues.push(format!("{}: ambiguous ({})", field_label, reason));
                None
            }
            NormalizationOutcome::Invalid { reason, .. } => {
                issues.push(format!("{}: invalid ({})", field_label, reason));
                None
            }
        },
        _ => {
            if required {
                issues.push(format!("{}: missing", field_label));
            }
            None
        }
    }
}

fn parse_date_from_field(
    raw: Option<&str>,
    field_label: &str,
    issues: &mut Vec<String>,
    required: bool,
) -> Option<NaiveDate> {
    match raw {
        Some(s) if !s.trim().is_empty() => match normalize_date(s) {
            NormalizationOutcome::Normalized { value, .. } => Some(value),
            NormalizationOutcome::Ambiguous { reason, .. } => {
                issues.push(format!("{}: ambiguous ({})", field_label, reason));
                None
            }
            NormalizationOutcome::Invalid { reason, .. } => {
                issues.push(format!("{}: invalid ({})", field_label, reason));
                None
            }
        },
        _ => {
            if required {
                issues.push(format!("{}: missing", field_label));
            }
            None
        }
    }
}

pub async fn preview_materialization(
    state: &AppState,
    document_id: Uuid,
) -> AppResult<MaterializationPreview> {
    let doc = sqlx::query(
        r#"
        SELECT id, effective_category, processing_status, prompt_injection_flagged,
               vendor_id, materialized_entity_type, materialized_entity_id
        FROM documents WHERE id = $1
        "#,
    )
    .bind(document_id)
    .fetch_one(&state.db)
    .await?;

    let category: String = doc.get("effective_category");
    let prompt_flagged: bool = doc.get("prompt_injection_flagged");
    let doc_vendor_id: Option<Uuid> = doc.get("vendor_id");
    let mat_type: Option<String> = doc.get("materialized_entity_type");
    let mat_id: Option<Uuid> = doc.get("materialized_entity_id");

    let latest_ext_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM document_extractions WHERE document_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(document_id)
    .fetch_optional(&state.db)
    .await?;

    type LineFieldValue = (String, Option<i32>, Option<serde_json::Value>);
    let mut header_map: HashMap<String, String> = HashMap::new();
    let mut line_groups: HashMap<i32, HashMap<String, LineFieldValue>> = HashMap::new();
    let mut issues: Vec<String> = Vec::new();

    if prompt_flagged {
        issues.push("prompt_injection_flagged: requires human security review".into());
    }

    if let Some(ext_id) = latest_ext_id {
        let rows = sqlx::query(
            r#"
            SELECT field_name, line_item_index, extracted_value, human_override_value,
                   validation_status, page_number, bounding_box
            FROM extracted_fields
            WHERE extraction_id = $1
            ORDER BY line_item_index NULLS FIRST, field_name ASC
            "#,
        )
        .bind(ext_id)
        .fetch_all(&state.db)
        .await?;

        for r in rows {
            let fname: String = r.get("field_name");
            let lidx: Option<i32> = r.get("line_item_index");
            let val_status: String = r.get("validation_status");
            let effective: Option<String> = r
                .get::<Option<String>, _>("human_override_value")
                .or_else(|| r.get::<Option<String>, _>("extracted_value"));

            if val_status == "ambiguous" {
                issues.push(format!("{}: flagged ambiguous during extraction", fname));
            }

            if let Some(val) = effective {
                if let Some(idx) = lidx {
                    let page_num: Option<i32> = r.get("page_number");
                    let bbox: Option<serde_json::Value> = r.get("bounding_box");
                    line_groups
                        .entry(idx)
                        .or_default()
                        .insert(fname, (val, page_num, bbox));
                } else {
                    header_map.insert(fname, val);
                }
            }
        }
    } else {
        issues.push("extraction: document has not been extracted yet".into());
    }

    // Resolve vendor from document.vendor_id, header vendor_id code, or vendor_name
    let mut matched_vendor: Option<(Uuid, String)> = None;
    if let Some(vid) = doc_vendor_id {
        if let Some(name) =
            sqlx::query_scalar::<_, String>("SELECT legal_name FROM vendors WHERE id = $1")
                .bind(vid)
                .fetch_optional(&state.db)
                .await?
        {
            matched_vendor = Some((vid, name));
        }
    }
    if matched_vendor.is_none() {
        if let Some(vcode) = header_map.get("vendor_id") {
            if let Some(row) = sqlx::query(
                "SELECT id, legal_name FROM vendors WHERE LOWER(vendor_code) = LOWER($1) LIMIT 1",
            )
            .bind(vcode.trim())
            .fetch_optional(&state.db)
            .await?
            {
                matched_vendor = Some((row.get("id"), row.get("legal_name")));
            }
        }
    }
    if matched_vendor.is_none() {
        if let Some(vname) = header_map.get("vendor_name") {
            let rows = sqlx::query(
                "SELECT id, legal_name FROM vendors WHERE LOWER(legal_name) = LOWER($1)",
            )
            .bind(vname.trim())
            .fetch_all(&state.db)
            .await?;
            if rows.len() == 1 {
                matched_vendor = Some((rows[0].get("id"), rows[0].get("legal_name")));
            } else if rows.len() > 1 {
                issues.push("vendor: multiple vendors share that legal name".into());
            }
        }
    }
    if matched_vendor.is_none() {
        issues.push("vendor_id: no unambiguous matching vendor in repository".into());
    }

    let currency = header_map
        .get("currency")
        .map(|s| s.trim().to_ascii_uppercase())
        .filter(|s| s.len() == 3);
    if currency.is_none() {
        issues.push("currency: missing explicit ISO-4217 currency".into());
    }
    let curr_ref = currency.as_deref();

    let doc_number = header_map
        .get("document_number")
        .or_else(|| header_map.get("payment_reference"))
        .or_else(|| header_map.get("contract_number"))
        .or_else(|| header_map.get("po_number"))
        .cloned();
    if doc_number.is_none() {
        issues.push("document_number: missing".into());
    }

    let is_contract = category == "contract" || category == "contract_amendment";
    let is_invoice = category == "invoice" || category == "credit_note" || category == "debit_note";
    let is_po = category == "purchase_order";
    let is_payment = category == "payment_record";

    let doc_date = parse_date_from_field(
        header_map.get("document_date").map(|s| s.as_str()),
        "document_date",
        &mut issues,
        is_invoice || is_po || is_payment,
    );
    let effective_date = parse_date_from_field(
        header_map.get("effective_date").map(|s| s.as_str()),
        "effective_date",
        &mut issues,
        is_contract,
    );
    let expiration_date = parse_date_from_field(
        header_map.get("expiration_date").map(|s| s.as_str()),
        "expiration_date",
        &mut issues,
        is_contract,
    );

    let subtotal = parse_dec_from_field(
        header_map.get("subtotal").map(|s| s.as_str()),
        curr_ref,
        "subtotal",
        &mut issues,
        false,
    );
    let discount_total = parse_dec_from_field(
        header_map.get("discount_total").map(|s| s.as_str()),
        curr_ref,
        "discount_total",
        &mut issues,
        false,
    )
    .unwrap_or(Decimal::ZERO);
    let tax_amount = parse_dec_from_field(
        header_map.get("tax_amount").map(|s| s.as_str()),
        curr_ref,
        "tax_amount",
        &mut issues,
        false,
    )
    .unwrap_or(Decimal::ZERO);
    let additional_charges = parse_dec_from_field(
        header_map.get("additional_charges").map(|s| s.as_str()),
        curr_ref,
        "additional_charges",
        &mut issues,
        false,
    )
    .unwrap_or(Decimal::ZERO);
    let total_amount = parse_dec_from_field(
        header_map.get("total_amount").map(|s| s.as_str()),
        curr_ref,
        "total_amount",
        &mut issues,
        is_invoice || is_po || is_payment,
    );

    let mut line_drafts = Vec::new();
    let mut sorted_keys: Vec<i32> = line_groups.keys().copied().collect();
    sorted_keys.sort();
    for idx in sorted_keys {
        let map = &line_groups[&idx];
        let get_str = |k: &str| map.get(k).map(|(v, _, _)| v.clone());
        let first_meta = map.values().next();
        let page_num = first_meta.and_then(|(_, p, _)| *p);
        let bbox = first_meta.and_then(|(_, _, b)| b.clone());

        let mut line_issues = Vec::new();
        let qty = parse_dec_from_field(
            get_str("line_item.quantity").as_deref(),
            None,
            &format!("line_{}.quantity", idx),
            &mut line_issues,
            true,
        );
        let unit_price = parse_dec_from_field(
            get_str("line_item.unit_price").as_deref(),
            curr_ref,
            &format!("line_{}.unit_price", idx),
            &mut line_issues,
            true,
        );
        let line_total = parse_dec_from_field(
            get_str("line_item.line_total").as_deref(),
            curr_ref,
            &format!("line_{}.line_total", idx),
            &mut line_issues,
            is_invoice || is_po,
        );
        let disc_pct = get_str("line_item.discount")
            .and_then(|s| match normalize_percentage(&s) {
                NormalizationOutcome::Normalized { value, .. } => Some(value),
                _ => None,
            })
            .unwrap_or(Decimal::ZERO);
        let tax_rate = get_str("line_item.tax_rate").and_then(|s| match normalize_percentage(&s) {
            NormalizationOutcome::Normalized { value, .. } => Some(value),
            _ => None,
        });
        let line_tax_amt = parse_dec_from_field(
            get_str("line_item.tax_amount").as_deref(),
            curr_ref,
            &format!("line_{}.tax_amount", idx),
            &mut line_issues,
            false,
        )
        .unwrap_or(Decimal::ZERO);

        let ambiguous = !line_issues.is_empty();
        issues.extend(line_issues);

        let (unit_norm, _) = normalize_unit(get_str("line_item.unit").as_deref().unwrap_or("unit"));
        line_drafts.push(MaterializedLineDraft {
            line_number: idx,
            item_code: get_str("line_item.item_code"),
            description: get_str("line_item.description").unwrap_or_else(|| format!("Line {}", idx)),
            quantity: qty,
            unit: unit_norm,
            unit_price,
            discount_pct: disc_pct,
            tax_rate,
            tax_amount: line_tax_amt,
            line_total,
            page_number: page_num,
            bounding_box: bbox,
            ambiguous,
        });
    }

    if (is_invoice || is_po || is_contract) && line_drafts.is_empty() {
        issues.push("line_items: no structured line items extracted".into());
    }

    let supported_cat = is_invoice || is_contract || is_po || is_payment;
    if !supported_cat {
        issues.push(format!("effective_category '{}' cannot be auto-materialized", category));
    }

    let ready = mat_id.is_none() && supported_cat && issues.is_empty();

    Ok(MaterializationPreview {
        document_id,
        effective_category: category,
        already_materialized: mat_id.is_some(),
        materialized_entity_type: mat_type,
        materialized_entity_id: mat_id,
        ready_for_auto_materialize: ready,
        missing_or_ambiguous_fields: issues,
        matched_vendor_id: matched_vendor.as_ref().map(|(id, _)| *id),
        matched_vendor_name: matched_vendor.as_ref().map(|(_, n)| n.clone()),
        draft: serde_json::json!({
            "document_number": doc_number,
            "document_date": doc_date,
            "effective_date": effective_date,
            "expiration_date": expiration_date,
            "currency": currency,
            "contract_number": header_map.get("contract_number"),
            "po_number": header_map.get("po_number"),
            "payment_terms": header_map.get("payment_terms"),
            "payment_reference": header_map.get("payment_reference"),
            "bank_reference": header_map.get("bank_reference"),
            "related_invoice_number": header_map.get("related_invoice_number"),
            "vendor_name": header_map.get("vendor_name"),
            "subtotal": subtotal.or(total_amount),
            "discount_total": discount_total,
            "tax_total": tax_amount,
            "additional_charges": additional_charges,
            "total_amount": total_amount
        }),
        line_items: line_drafts,
    })
}

pub async fn materialize_document(
    state: &AppState,
    document_id: Uuid,
    req: MaterializeRequest,
    actor_id: Option<Uuid>,
) -> AppResult<MaterializationResult> {
    let preview = preview_materialization(state, document_id).await?;
    if preview.already_materialized {
        return Err(AppError::Conflict(
            "Document has already been materialized into a financial record.".into(),
        ));
    }

    let target_cat = req
        .entity_type
        .clone()
        .unwrap_or_else(|| preview.effective_category.clone());

    let vendor_id = req
        .vendor_id
        .or(preview.matched_vendor_id)
        .ok_or_else(|| AppError::BadRequest("Vendor ID is required to materialize document.".into()))?;

    let currency = req
        .currency
        .clone()
        .or_else(|| preview.draft["currency"].as_str().map(|s| s.to_string()))
        .ok_or_else(|| AppError::BadRequest("Currency is required to materialize document.".into()))?
        .trim()
        .to_ascii_uppercase();

    let doc_number = req
        .document_number
        .clone()
        .or_else(|| preview.draft["document_number"].as_str().map(|s| s.to_string()))
        .ok_or_else(|| AppError::BadRequest("Document/reference number is required.".into()))?;

    let line_items: Vec<MaterializeLineOverride> = if let Some(custom) = req.line_items.clone() {
        custom
    } else {
        preview
            .line_items
            .iter()
            .filter_map(|l| {
                let qty = l.quantity?;
                let price = l.unit_price?;
                let total = l.line_total.unwrap_or_else(|| qty * price);
                Some(MaterializeLineOverride {
                    item_code: l.item_code.clone(),
                    description: l.description.clone(),
                    quantity: qty,
                    unit: l.unit.clone(),
                    unit_price: price,
                    discount_pct: Some(l.discount_pct),
                    discount_amount: None,
                    tax_rate: l.tax_rate,
                    tax_amount: Some(l.tax_amount),
                    stated_line_subtotal: Some(qty * price),
                    stated_line_total: total,
                    is_additional_charge: Some(false),
                })
            })
            .collect()
    };

    let mut tx = state.db.begin().await?;
    let (entity_type, entity_id): (String, Uuid) = match target_cat.as_str() {
        "invoice" | "credit_note" | "debit_note" => {
            let inv_date = req
                .document_date
                .or_else(|| {
                    preview.draft["document_date"]
                        .as_str()
                        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                })
                .ok_or_else(|| AppError::BadRequest("Invoice date is required.".into()))?;
            let total = req
                .total_amount
                .or_else(|| {
                    preview.draft["total_amount"]
                        .as_str()
                        .and_then(|s| s.parse().ok())
                })
                .ok_or_else(|| AppError::BadRequest("Total amount is required for invoice.".into()))?;
            let subtotal = req
                .subtotal
                .or_else(|| {
                    preview.draft["subtotal"]
                        .as_str()
                        .and_then(|s| s.parse().ok())
                })
                .unwrap_or(total);
            let disc_total = req.discount_total.unwrap_or(Decimal::ZERO);
            let tax_total = req.tax_total.unwrap_or(Decimal::ZERO);
            let add_charges = req.additional_charges.unwrap_or(Decimal::ZERO);
            let contract_ref = req
                .contract_number
                .clone()
                .or_else(|| preview.draft["contract_number"].as_str().map(|s| s.to_string()));
            let po_ref = req
                .po_number
                .clone()
                .or_else(|| preview.draft["po_number"].as_str().map(|s| s.to_string()));
            let pay_terms = req
                .payment_terms
                .clone()
                .or_else(|| preview.draft["payment_terms"].as_str().map(|s| s.to_string()));

            let inv_id: Uuid = sqlx::query_scalar(
                r#"
                INSERT INTO invoices (
                    document_id, invoice_number, vendor_id, invoice_date, currency,
                    referenced_contract_number, referenced_po_number, stated_subtotal,
                    stated_discount_total, stated_tax_total, stated_additional_charges,
                    stated_total, payment_terms, is_credit_note
                )
                VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
                RETURNING id
                "#,
            )
            .bind(document_id)
            .bind(doc_number.trim())
            .bind(vendor_id)
            .bind(inv_date)
            .bind(&currency)
            .bind(&contract_ref)
            .bind(&po_ref)
            .bind(subtotal)
            .bind(disc_total)
            .bind(tax_total)
            .bind(add_charges)
            .bind(total)
            .bind(&pay_terms)
            .bind(target_cat == "credit_note")
            .fetch_one(&mut *tx)
            .await?;

            for (idx, l) in line_items.iter().enumerate() {
                sqlx::query(
                    r#"
                    INSERT INTO invoice_line_items (
                        invoice_id, line_number, item_code, description, quantity, unit, unit_price,
                        discount_pct, discount_amount, tax_rate, tax_amount,
                        stated_line_subtotal, stated_line_total, is_additional_charge, source_page
                    )
                    VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,1)
                    "#,
                )
                .bind(inv_id)
                .bind((idx + 1) as i32)
                .bind(&l.item_code)
                .bind(&l.description)
                .bind(l.quantity)
                .bind(&l.unit)
                .bind(l.unit_price)
                .bind(l.discount_pct.unwrap_or(Decimal::ZERO))
                .bind(l.discount_amount.unwrap_or(Decimal::ZERO))
                .bind(l.tax_rate)
                .bind(l.tax_amount.unwrap_or(Decimal::ZERO))
                .bind(l.stated_line_subtotal.unwrap_or(l.stated_line_total))
                .bind(l.stated_line_total)
                .bind(l.is_additional_charge.unwrap_or(false))
                .execute(&mut *tx)
                .await?;
            }
            ("invoice".into(), inv_id)
        }
        "contract" | "contract_amendment" => {
            let eff = req
                .effective_date
                .or_else(|| {
                    preview.draft["effective_date"]
                        .as_str()
                        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                })
                .ok_or_else(|| AppError::BadRequest("Contract effective_date is required.".into()))?;
            let exp = req
                .expiration_date
                .or_else(|| {
                    preview.draft["expiration_date"]
                        .as_str()
                        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                })
                .ok_or_else(|| AppError::BadRequest("Contract expiration_date is required.".into()))?;
            let contract_num = req.contract_number.clone().unwrap_or_else(|| doc_number.clone());
            let title = req
                .title
                .clone()
                .unwrap_or_else(|| format!("Contract {}", contract_num));

            let contract_id: Uuid = sqlx::query_scalar(
                "INSERT INTO contracts (contract_number, vendor_id, title, currency) VALUES ($1,$2,$3,$4) RETURNING id",
            )
            .bind(contract_num.trim())
            .bind(vendor_id)
            .bind(title.trim())
            .bind(&currency)
            .fetch_one(&mut *tx)
            .await?;

            let version_id: Uuid = sqlx::query_scalar(
                r#"
                INSERT INTO contract_versions (
                    contract_id, document_id, version_label, version_number, is_amendment,
                    effective_date, expiration_date, payment_terms
                )
                VALUES ($1,$2,'v1.0',1,$3,$4,$5,$6)
                RETURNING id
                "#,
            )
            .bind(contract_id)
            .bind(document_id)
            .bind(target_cat == "contract_amendment")
            .bind(eff)
            .bind(exp)
            .bind(&req.payment_terms)
            .fetch_one(&mut *tx)
            .await?;

            for l in &line_items {
                sqlx::query(
                    "INSERT INTO contract_line_items (contract_version_id, item_code, description, unit, unit_price, currency, discount_pct, tax_rate) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
                )
                .bind(version_id)
                .bind(&l.item_code)
                .bind(&l.description)
                .bind(&l.unit)
                .bind(l.unit_price)
                .bind(&currency)
                .bind(l.discount_pct.unwrap_or(Decimal::ZERO))
                .bind(l.tax_rate)
                .execute(&mut *tx)
                .await?;
            }
            ("contract".into(), contract_id)
        }
        "purchase_order" => {
            let issue_date = req
                .document_date
                .or_else(|| {
                    preview.draft["document_date"]
                        .as_str()
                        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                })
                .ok_or_else(|| AppError::BadRequest("PO issue date is required.".into()))?;
            let total = req
                .total_amount
                .or_else(|| {
                    preview.draft["total_amount"]
                        .as_str()
                        .and_then(|s| s.parse().ok())
                })
                .unwrap_or_else(|| line_items.iter().map(|l| l.stated_line_total).sum());

            let po_id: Uuid = sqlx::query_scalar(
                "INSERT INTO purchase_orders (po_number, vendor_id, document_id, issue_date, currency, total_amount) VALUES ($1,$2,$3,$4,$5,$6) RETURNING id",
            )
            .bind(doc_number.trim())
            .bind(vendor_id)
            .bind(document_id)
            .bind(issue_date)
            .bind(&currency)
            .bind(total)
            .fetch_one(&mut *tx)
            .await?;

            for l in &line_items {
                sqlx::query(
                    "INSERT INTO purchase_order_line_items (purchase_order_id, item_code, description, unit, ordered_quantity, unit_price, line_total) VALUES ($1,$2,$3,$4,$5,$6,$7)",
                )
                .bind(po_id)
                .bind(&l.item_code)
                .bind(&l.description)
                .bind(&l.unit)
                .bind(l.quantity)
                .bind(l.unit_price)
                .bind(l.stated_line_total)
                .execute(&mut *tx)
                .await?;
            }
            ("purchase_order".into(), po_id)
        }
        "payment_record" => {
            let pay_date = req
                .document_date
                .or_else(|| {
                    preview.draft["document_date"]
                        .as_str()
                        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                })
                .ok_or_else(|| AppError::BadRequest("Payment date is required.".into()))?;
            let amount = req
                .total_amount
                .or_else(|| {
                    preview.draft["total_amount"]
                        .as_str()
                        .and_then(|s| s.parse().ok())
                })
                .ok_or_else(|| AppError::BadRequest("Payment amount is required.".into()))?;
            let vendor_name: String =
                sqlx::query_scalar("SELECT legal_name FROM vendors WHERE id = $1")
                    .bind(vendor_id)
                    .fetch_one(&mut *tx)
                    .await?;
            let bank_ref = req
                .bank_reference
                .clone()
                .or_else(|| preview.draft["bank_reference"].as_str().map(|s| s.to_string()));
            let rel_inv = req.related_invoice_number.clone().or_else(|| {
                preview.draft["related_invoice_number"]
                    .as_str()
                    .map(|s| s.to_string())
            });

            let pay_id: Uuid = sqlx::query_scalar(
                r#"
                INSERT INTO payments (
                    document_id, payment_reference, bank_reference, vendor_id,
                    beneficiary_name, payment_date, amount, currency, referenced_invoice_number
                )
                VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
                RETURNING id
                "#,
            )
            .bind(document_id)
            .bind(doc_number.trim())
            .bind(&bank_ref)
            .bind(vendor_id)
            .bind(&vendor_name)
            .bind(pay_date)
            .bind(amount)
            .bind(&currency)
            .bind(&rel_inv)
            .fetch_one(&mut *tx)
            .await?;
            ("payment_record".into(), pay_id)
        }
        other => {
            return Err(AppError::BadRequest(format!(
                "Category '{}' cannot be materialized to a financial entity.",
                other
            )))
        }
    };

    sqlx::query(
        r#"
        UPDATE documents
        SET materialized_entity_type = $2,
            materialized_entity_id = $3,
            materialized_at = NOW(),
            vendor_id = COALESCE(vendor_id, $4),
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(document_id)
    .bind(&entity_type)
    .bind(entity_id)
    .bind(vendor_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    state
        .log_audit(
            actor_id,
            None,
            "document.materialized",
            &entity_type,
            Some(&entity_id.to_string()),
            serde_json::json!({ "document_id": document_id, "entity_type": entity_type, "entity_id": entity_id }),
        )
        .await;

    let mut recon_job_id = None;
    let mut recon_status = None;
    if entity_type == "invoice" && req.auto_reconcile.unwrap_or(true) {
        let (job_id, out) =
            run_invoice_reconciliation_workflow(state, entity_id, actor_id, None).await?;
        recon_job_id = Some(job_id);
        recon_status = Some(out.overall_status);
    }

    Ok(MaterializationResult {
        document_id,
        entity_type,
        entity_id,
        reconciliation_job_id: recon_job_id,
        reconciliation_status: recon_status,
    })
}
