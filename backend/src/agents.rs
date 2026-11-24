use crate::{
    error::AppResult,
    normalization::{normalize_date, normalize_money, NormalizationOutcome},
    reconciliation::{
        run_reconciliation, ContractLineItemInput, ContractVersionInput, ExistingInvoiceSummary,
        InvoiceInput, InvoiceLineItemInput, PaymentInput, PoLineItemInput, PurchaseOrderInput,
        ReconciliationOutput, RECONCILIATION_ENGINE_VERSION,
    },
    security::scan_prompt_injection,
    state::AppState,
};
use chrono::Utc;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::time::Instant;
use uuid::Uuid;

pub const WORKFLOW_RULE_VERSION: &str = "abifr-agent-workflow-v1.0.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStepRecord {
    pub workflow_run_id: Uuid,
    pub step_number: i32,
    pub agent_name: String,
    pub status: String,
    pub model_or_rule_version: String,
    pub output_summary: serde_json::Value,
}

async fn record_agent_run(
    state: &AppState,
    workflow_run_id: Uuid,
    step_number: i32,
    agent_name: &str,
    idempotency_key: &str,
    document_id: Option<Uuid>,
    invoice_id: Option<Uuid>,
    status: &str,
    model_or_rule_version: &str,
    input_payload: serde_json::Value,
    output_payload: serde_json::Value,
    evidence_refs: serde_json::Value,
    error_message: Option<&str>,
    duration_ms: i32,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO agent_runs (
            workflow_run_id, step_number, agent_name, idempotency_key,
            document_id, invoice_id, status, model_or_rule_version,
            input_payload, output_payload, evidence_refs, error_message,
            duration_ms, completed_at
        )
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,NOW())
        ON CONFLICT (idempotency_key) DO UPDATE SET
            status = EXCLUDED.status,
            output_payload = EXCLUDED.output_payload,
            error_message = EXCLUDED.error_message,
            duration_ms = EXCLUDED.duration_ms,
            completed_at = NOW()
        "#,
    )
    .bind(workflow_run_id)
    .bind(step_number)
    .bind(agent_name)
    .bind(idempotency_key)
    .bind(document_id)
    .bind(invoice_id)
    .bind(status)
    .bind(model_or_rule_version)
    .bind(input_payload)
    .bind(output_payload)
    .bind(evidence_refs)
    .bind(error_message)
    .bind(duration_ms)
    .execute(&state.db)
    .await?;
    Ok(())
}

/// Executes the controlled, acyclic document ingestion & extraction agent pipeline (Agents 1, 2/3, 8).
pub async fn run_document_pipeline(
    state: &AppState,
    document_id: Uuid,
    actor_id: Option<Uuid>,
) -> AppResult<Uuid> {
    let workflow_run_id = Uuid::new_v4();

    // Mark document processing
    sqlx::query(
        "UPDATE documents SET processing_status = 'processing', ocr_status = 'processing', extraction_status = 'processing', failure_reason = NULL, updated_at = NOW() WHERE id = $1",
    )
    .bind(document_id)
    .execute(&state.db)
    .await?;

    let doc_row = sqlx::query(
        "SELECT id, original_filename, stored_blob_key, file_extension, sha256_hash, classification_locked_by, effective_category FROM documents WHERE id = $1",
    )
    .bind(document_id)
    .fetch_one(&state.db)
    .await?;

    let blob_key: String = doc_row.get("stored_blob_key");
    let extension: String = doc_row.get("file_extension");
    let locked_by: Option<Uuid> = doc_row.get("classification_locked_by");
    let existing_category: String = doc_row.get("effective_category");

    let bytes = state.storage.read_blob(&blob_key).await?;

    // Check active default model provider from DB
    let active_model = sqlx::query(
        "SELECT provider_name, model_identifier FROM model_metadata WHERE is_default = TRUE AND is_enabled = TRUE LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await?;
    let (provider_name, model_id): (Option<String>, Option<String>) = match active_model {
        Some(r) => (Some(r.get("provider_name")), Some(r.get("model_identifier"))),
        None => (Some(state.config.ai_provider.clone()), None),
    };

    // Call internal Python Document Intelligence worker
    let t0 = Instant::now();
    let extract_res = state
        .doc_intel
        .extract(
            &bytes,
            &extension,
            provider_name.as_deref(),
            model_id.as_deref(),
        )
        .await;

    let extracted = match extract_res {
        Ok(res) => res,
        Err(err) => {
            let err_msg = err.to_string();
            sqlx::query(
                "UPDATE documents SET processing_status = 'failed', ocr_status = 'failed', extraction_status = 'failed', failure_reason = $2, updated_at = NOW() WHERE id = $1",
            )
            .bind(document_id)
            .bind(&err_msg)
            .execute(&state.db)
            .await?;

            let _ = record_agent_run(
                state,
                workflow_run_id,
                1,
                "DOCUMENT_CLASSIFIER",
                &format!("{}:1", workflow_run_id),
                Some(document_id),
                None,
                "failed",
                WORKFLOW_RULE_VERSION,
                serde_json::json!({ "document_id": document_id, "extension": extension }),
                serde_json::json!({}),
                serde_json::json!([]),
                Some(&err_msg),
                t0.elapsed().as_millis() as i32,
            )
            .await;
            return Err(err);
        }
    };

    // Rust-side independent prompt-injection scan (defense in depth!)
    let rust_signals = scan_prompt_injection(&extracted.raw_text);
    let mut combined_signals = serde_json::to_value(&extracted.prompt_injection_signals).unwrap_or_else(|_| serde_json::json!([]));
    if let (Some(arr), Ok(extra)) = (combined_signals.as_array_mut(), serde_json::to_value(&rust_signals)) {
        if let Some(extra_arr) = extra.as_array() {
            for item in extra_arr {
                if !arr.contains(item) {
                    arr.push(item.clone());
                }
            }
        }
    }
    let injection_flagged = combined_signals.as_array().map(|a| !a.is_empty()).unwrap_or(false);

    // Persist immutable extraction record
    let extraction_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO document_extractions (
            document_id, extraction_method, provider_name, model_version,
            raw_text, pages_json, tables_json, structured_payload,
            prompt_injection_signals, redaction_applied, duration_ms
        )
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
        RETURNING id
        "#,
    )
    .bind(document_id)
    .bind(&extracted.extraction_method)
    .bind(&extracted.provider_name)
    .bind(&extracted.model_version)
    .bind(&extracted.raw_text)
    .bind(&extracted.pages)
    .bind(&extracted.tables)
    .bind(&extracted.structured_payload)
    .bind(&combined_signals)
    .bind(extracted.redaction_applied)
    .bind(extracted.duration_ms)
    .fetch_one(&state.db)
    .await?;

    // Persist every extracted field with deterministic Rust normalization alongside raw value (raw value is NEVER overwritten!)
    let header_currency = extracted
        .fields
        .iter()
        .find(|f| f.field_name == "currency")
        .and_then(|f| f.extracted_value.clone());

    let mut ambiguous_fields: Vec<String> = Vec::new();
    let mut low_confidence_fields: Vec<String> = Vec::new();

    for f in &extracted.fields {
        let mut norm_val: Option<String> = None;
        let mut norm_dec: Option<Decimal> = None;
        let mut norm_curr: Option<String> = None;
        let mut norm_rule: Option<String> = None;
        let mut validation_status = f.validation_status.clone();

        if let Some(raw_v) = &f.extracted_value {
            if f.field_name.contains("amount")
                || f.field_name.contains("total")
                || f.field_name.contains("price")
                || f.field_name.contains("subtotal")
                || f.field_name.contains("charges")
            {
                match normalize_money(raw_v, header_currency.as_deref()) {
                    NormalizationOutcome::Normalized { value, rule, .. } => {
                        norm_val = Some(value.amount.to_string());
                        norm_dec = Some(value.amount);
                        norm_curr = value.currency;
                        norm_rule = Some(rule);
                    }
                    NormalizationOutcome::Ambiguous { reason, .. } => {
                        validation_status = "ambiguous".into();
                        norm_rule = Some(format!("AMBIGUOUS: {}", reason));
                        ambiguous_fields.push(f.field_name.clone());
                    }
                    NormalizationOutcome::Invalid { .. } => {
                        validation_status = "ambiguous".into();
                        ambiguous_fields.push(f.field_name.clone());
                    }
                }
            } else if f.field_name.contains("date") {
                match normalize_date(raw_v) {
                    NormalizationOutcome::Normalized { value, rule, .. } => {
                        norm_val = Some(value.format("%Y-%m-%d").to_string());
                        norm_rule = Some(rule);
                    }
                    NormalizationOutcome::Ambiguous { reason, .. } => {
                        validation_status = "ambiguous".into();
                        norm_rule = Some(format!("AMBIGUOUS: {}", reason));
                        ambiguous_fields.push(f.field_name.clone());
                    }
                    NormalizationOutcome::Invalid { .. } => {
                        validation_status = "ambiguous".into();
                        ambiguous_fields.push(f.field_name.clone());
                    }
                }
            }
        }

        if let Some(conf) = f.provider_confidence {
            if conf < 0.75 {
                low_confidence_fields.push(f.field_name.clone());
            }
        }

        let conf_dec = f
            .provider_confidence
            .and_then(|c| Decimal::from_f64_retain(c).map(|d| d.round_dp(4)));

        sqlx::query(
            r#"
            INSERT INTO extracted_fields (
                extraction_id, document_id, field_name, line_item_index,
                extracted_value, original_text, page_number, bounding_box,
                source_location, extraction_method, provider_confidence,
                validation_status, normalized_value, normalized_decimal,
                normalized_currency, normalization_rule
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)
            "#,
        )
        .bind(extraction_id)
        .bind(document_id)
        .bind(&f.field_name)
        .bind(f.line_item_index)
        .bind(&f.extracted_value)
        .bind(&f.original_text)
        .bind(f.page_number)
        .bind(&f.bounding_box)
        .bind(&f.source_location)
        .bind(&f.extraction_method)
        .bind(conf_dec)
        .bind(&validation_status)
        .bind(&norm_val)
        .bind(norm_dec)
        .bind(&norm_curr)
        .bind(&norm_rule)
        .execute(&state.db)
        .await?;
    }

    // AGENT 1 — DOCUMENT_CLASSIFIER
    let ai_cat = extracted.classification.category.clone();
    let effective_cat = if locked_by.is_some() {
        existing_category
    } else {
        ai_cat.clone()
    };
    let cls_conf_dec = extracted
        .classification
        .confidence
        .and_then(|c| Decimal::from_f64_retain(c).map(|d| d.round_dp(4)));

    record_agent_run(
        state,
        workflow_run_id,
        1,
        "DOCUMENT_CLASSIFIER",
        &format!("{}:1:{}", workflow_run_id, document_id),
        Some(document_id),
        None,
        if effective_cat == "unknown" { "routed_to_human" } else { "completed" },
        &extracted.model_version,
        serde_json::json!({ "document_id": document_id, "extension": extension }),
        serde_json::json!({
            "ai_category": ai_cat,
            "effective_category": effective_cat,
            "confidence": extracted.classification.confidence,
            "reasons": extracted.classification.reasons
        }),
        serde_json::json!([{ "document_id": document_id, "extraction_id": extraction_id }]),
        None,
        extracted.duration_ms,
    )
    .await?;

    // AGENT 2 or 3 — CONTRACT_EXTRACTOR / INVOICE_EXTRACTOR
    let extractor_agent = if effective_cat.starts_with("contract") {
        "CONTRACT_EXTRACTOR"
    } else {
        "INVOICE_EXTRACTOR"
    };
    record_agent_run(
        state,
        workflow_run_id,
        2,
        extractor_agent,
        &format!("{}:2:{}", workflow_run_id, document_id),
        Some(document_id),
        None,
        if ambiguous_fields.is_empty() && low_confidence_fields.is_empty() {
            "completed"
        } else {
            "routed_to_human"
        },
        &extracted.model_version,
        serde_json::json!({ "document_id": document_id, "category": effective_cat }),
        serde_json::json!({
            "fields_extracted": extracted.fields.len(),
            "ambiguous_fields": ambiguous_fields,
            "low_confidence_fields": low_confidence_fields,
            "prompt_injection_flagged": injection_flagged,
            "provider_error": extracted.provider_error
        }),
        serde_json::json!([{ "document_id": document_id, "extraction_id": extraction_id }]),
        extracted.provider_error.as_deref(),
        extracted.duration_ms,
    )
    .await?;

    // Route uncertain extractions or prompt-injection signals to human review (AGENT 8 — REVIEW_COORDINATOR)
    let needs_human_review = effective_cat == "unknown"
        || !ambiguous_fields.is_empty()
        || !low_confidence_fields.is_empty()
        || injection_flagged;

    if needs_human_review {
        let reason = if injection_flagged {
            "Prompt injection pattern detected in untrusted document content; isolated and routed for security & human review."
        } else if effective_cat == "unknown" {
            "Document category could not be deterministically classified with sufficient evidence; manual classification required."
        } else {
            "Extraction contains ambiguous dates/amounts or low-confidence OCR fields requiring human verification."
        };

        let finding_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO anomaly_findings (
                category, classification_type, severity, severity_rule_applied, status,
                document_id, rule_applied, explanation, calculation_details, extraction_provenance
            )
            VALUES ($1,$2,$3,$4,'requires_review',$5,$6,$7,$8,$9)
            RETURNING id
            "#,
        )
        .bind("EXTRACTION_UNCERTAIN")
        .bind(if injection_flagged { "potential_anomaly" } else { "unresolved_discrepancy" })
        .bind(if injection_flagged { "high" } else { "medium" })
        .bind(if injection_flagged { "SEVERITY_RULE_PROMPT_INJECTION" } else { "SEVERITY_RULE_EXTRACTION_UNCERTAIN" })
        .bind(document_id)
        .bind(if injection_flagged { "PROMPT_INJECTION_QUARANTINE_POLICY" } else { "NO_SILENT_COERCION_POLICY" })
        .bind(reason)
        .bind(serde_json::json!({
            "ambiguous_fields": ambiguous_fields,
            "low_confidence_fields": low_confidence_fields,
            "prompt_injection_signals": combined_signals
        }))
        .bind(serde_json::json!({
            "extraction_id": extraction_id,
            "extraction_method": extracted.extraction_method,
            "model_version": extracted.model_version
        }))
        .fetch_one(&state.db)
        .await?;

        let _ = sqlx::query(
            r#"
            INSERT INTO evidence_references (finding_id, document_id, evidence_role, page_number, quoted_text, source_location)
            VALUES ($1, $2, 'invoice_source', 1, $3, 'document:extraction')
            "#,
        )
        .bind(finding_id)
        .bind(document_id)
        .bind(reason)
        .execute(&state.db)
        .await;

        record_agent_run(
            state,
            workflow_run_id,
            8,
            "REVIEW_COORDINATOR",
            &format!("{}:8:{}", workflow_run_id, document_id),
            Some(document_id),
            None,
            "routed_to_human",
            WORKFLOW_RULE_VERSION,
            serde_json::json!({ "document_id": document_id }),
            serde_json::json!({ "finding_id": finding_id, "reason": reason }),
            serde_json::json!([{ "finding_id": finding_id, "document_id": document_id }]),
            None,
            1,
        )
        .await?;
    }

    let final_proc_status = if needs_human_review { "requires_review" } else { "completed" };
    sqlx::query(
        r#"
        UPDATE documents
        SET page_count = $2,
            ai_category = $3,
            ai_classification_confidence = $4,
            effective_category = $5,
            processing_status = $6,
            ocr_status = 'completed',
            extraction_status = 'completed',
            prompt_injection_flagged = $7,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(document_id)
    .bind(extracted.page_count)
    .bind(&ai_cat)
    .bind(cls_conf_dec)
    .bind(&effective_cat)
    .bind(final_proc_status)
    .bind(injection_flagged)
    .execute(&state.db)
    .await?;

    state
        .log_audit(
            actor_id,
            None,
            "document.processed",
            "document",
            Some(&document_id.to_string()),
            serde_json::json!({
                "workflow_run_id": workflow_run_id,
                "extraction_id": extraction_id,
                "effective_category": effective_cat,
                "status": final_proc_status,
                "prompt_injection_flagged": injection_flagged
            }),
        )
        .await;

    Ok(workflow_run_id)
}

/// Executes Agents 4, 5, 6, 7, 8 on a persisted invoice using the authoritative deterministic Rust engine.
pub async fn run_invoice_reconciliation_workflow(
    state: &AppState,
    invoice_id: Uuid,
    actor_id: Option<Uuid>,
    idempotency_key: Option<String>,
) -> AppResult<(Uuid, ReconciliationOutput)> {
    let workflow_run_id = Uuid::new_v4();
    let job_idem = idempotency_key.unwrap_or_else(|| {
        format!("recon:{}:{}", invoice_id, Utc::now().timestamp_millis())
    });

    // Load invoice
    let inv_row = sqlx::query(
        r#"
        SELECT i.id, i.document_id, i.invoice_number, i.vendor_id, i.invoice_date,
               i.currency, i.referenced_contract_number, i.referenced_po_number,
               i.stated_subtotal, i.stated_discount_total, i.stated_tax_total,
               i.stated_additional_charges, i.stated_total, i.is_credit_note,
               d.sha256_hash
        FROM invoices i
        LEFT JOIN documents d ON d.id = i.document_id
        WHERE i.id = $1
        "#,
    )
    .bind(invoice_id)
    .fetch_one(&state.db)
    .await?;

    let vendor_id: Uuid = inv_row.get("vendor_id");
    let inv_doc_id: Option<Uuid> = inv_row.get("document_id");
    let inv_sha256: Option<String> = inv_row.get("sha256_hash");

    // Load invoice line items
    let line_rows = sqlx::query(
        r#"
        SELECT id, line_number, item_code, description, quantity, unit, unit_price,
               discount_pct, discount_amount, tax_rate, tax_amount,
               stated_line_subtotal, stated_line_total, is_additional_charge,
               matched_contract_item_id, match_method, source_page, source_bbox
        FROM invoice_line_items
        WHERE invoice_id = $1
        ORDER BY line_number ASC
        "#,
    )
    .bind(invoice_id)
    .fetch_all(&state.db)
    .await?;

    let inv_lines: Vec<InvoiceLineItemInput> = line_rows
        .into_iter()
        .map(|r| {
            let mm: String = r.get("match_method");
            let confirmed = if mm == "human_confirmed" {
                r.get("matched_contract_item_id")
            } else {
                None
            };
            InvoiceLineItemInput {
                id: r.get("id"),
                line_number: r.get("line_number"),
                item_code: r.get("item_code"),
                description: r.get("description"),
                quantity: r.get("quantity"),
                unit: r.get("unit"),
                unit_price: r.get("unit_price"),
                discount_pct: r.get("discount_pct"),
                discount_amount: r.get("discount_amount"),
                tax_rate: r.get("tax_rate"),
                tax_amount: r.get("tax_amount"),
                stated_line_subtotal: r.get("stated_line_subtotal"),
                stated_line_total: r.get("stated_line_total"),
                is_additional_charge: r.get("is_additional_charge"),
                human_confirmed_contract_item_id: confirmed,
                source_page: r.get("source_page"),
                source_bbox: r.get("source_bbox"),
            }
        })
        .collect();

    let invoice_input = InvoiceInput {
        id: inv_row.get("id"),
        document_id: inv_doc_id,
        invoice_number: inv_row.get("invoice_number"),
        vendor_id,
        invoice_date: inv_row.get("invoice_date"),
        currency: inv_row.get("currency"),
        referenced_contract_number: inv_row.get("referenced_contract_number"),
        referenced_po_number: inv_row.get("referenced_po_number"),
        stated_subtotal: inv_row.get("stated_subtotal"),
        stated_discount_total: inv_row.get("stated_discount_total"),
        stated_tax_total: inv_row.get("stated_tax_total"),
        stated_additional_charges: inv_row.get("stated_additional_charges"),
        stated_total: inv_row.get("stated_total"),
        is_credit_note: inv_row.get("is_credit_note"),
        line_items: inv_lines,
    };

    // Load vendor's contract versions & line items
    let cv_rows = sqlx::query(
        r#"
        SELECT cv.id AS version_id, cv.contract_id, c.contract_number, c.vendor_id, c.currency,
               cv.version_label, cv.version_number, cv.is_amendment,
               cv.effective_date, cv.expiration_date,
               cv.price_tolerance_pct, cv.price_tolerance_abs, cv.quantity_tolerance_pct,
               cv.default_tax_rate, cv.allowed_additional_charges, cv.document_id
        FROM contract_versions cv
        JOIN contracts c ON c.id = cv.contract_id
        WHERE c.vendor_id = $1
        ORDER BY cv.version_number ASC
        "#,
    )
    .bind(vendor_id)
    .fetch_all(&state.db)
    .await?;

    let mut contract_versions = Vec::new();
    for r in cv_rows {
        let vid: Uuid = r.get("version_id");
        let ci_rows = sqlx::query(
            "SELECT id, item_code, description, unit, unit_price, currency, min_quantity, max_quantity, discount_pct, tax_rate FROM contract_line_items WHERE contract_version_id = $1",
        )
        .bind(vid)
        .fetch_all(&state.db)
        .await?;

        let items = ci_rows
            .into_iter()
            .map(|cr| ContractLineItemInput {
                id: cr.get("id"),
                item_code: cr.get("item_code"),
                description: cr.get("description"),
                unit: cr.get("unit"),
                unit_price: cr.get("unit_price"),
                currency: cr.get("currency"),
                min_quantity: cr.get("min_quantity"),
                max_quantity: cr.get("max_quantity"),
                discount_pct: cr.get::<Option<Decimal>, _>("discount_pct").unwrap_or(Decimal::ZERO),
                tax_rate: cr.get("tax_rate"),
            })
            .collect();

        let allowed_json: serde_json::Value = r.get("allowed_additional_charges");
        let allowed_charges: Vec<String> = serde_json::from_value(allowed_json).unwrap_or_default();

        contract_versions.push(ContractVersionInput {
            version_id: vid,
            contract_id: r.get("contract_id"),
            contract_number: r.get("contract_number"),
            vendor_id: r.get("vendor_id"),
            currency: r.get("currency"),
            version_label: r.get("version_label"),
            version_number: r.get("version_number"),
            is_amendment: r.get("is_amendment"),
            effective_date: r.get("effective_date"),
            expiration_date: r.get("expiration_date"),
            price_tolerance_pct: r.get("price_tolerance_pct"),
            price_tolerance_abs: r.get("price_tolerance_abs"),
            quantity_tolerance_pct: r.get("quantity_tolerance_pct"),
            default_tax_rate: r.get("default_tax_rate"),
            allowed_additional_charges: allowed_charges,
            document_id: r.get("document_id"),
            line_items: items,
        });
    }

    // Load Purchase Orders for vendor
    let po_rows = sqlx::query(
        "SELECT id, po_number, vendor_id, currency, total_amount, document_id FROM purchase_orders WHERE vendor_id = $1",
    )
    .bind(vendor_id)
    .fetch_all(&state.db)
    .await?;

    let mut pos = Vec::new();
    for r in po_rows {
        let poid: Uuid = r.get("id");
        let pi_rows = sqlx::query(
            "SELECT id, item_code, description, unit, ordered_quantity, received_quantity, unit_price, line_total FROM purchase_order_line_items WHERE purchase_order_id = $1",
        )
        .bind(poid)
        .fetch_all(&state.db)
        .await?;
        let items = pi_rows
            .into_iter()
            .map(|pr| PoLineItemInput {
                id: pr.get("id"),
                item_code: pr.get("item_code"),
                description: pr.get("description"),
                unit: pr.get("unit"),
                ordered_quantity: pr.get("ordered_quantity"),
                received_quantity: pr.get("received_quantity"),
                unit_price: pr.get("unit_price"),
                line_total: pr.get("line_total"),
            })
            .collect();
        pos.push(PurchaseOrderInput {
            id: poid,
            po_number: r.get("po_number"),
            vendor_id: r.get("vendor_id"),
            currency: r.get("currency"),
            total_amount: r.get("total_amount"),
            document_id: r.get("document_id"),
            line_items: items,
        });
    }

    // Load other invoices for duplicate detection
    let other_rows = sqlx::query(
        r#"
        SELECT i.id, i.document_id, d.sha256_hash, i.invoice_number, i.vendor_id, i.invoice_date, i.currency, i.stated_total
        FROM invoices i
        LEFT JOIN documents d ON d.id = i.document_id
        WHERE i.id <> $1 AND (i.vendor_id = $2 OR ($3::text IS NOT NULL AND d.sha256_hash = $3))
        "#,
    )
    .bind(invoice_id)
    .bind(vendor_id)
    .bind(&inv_sha256)
    .fetch_all(&state.db)
    .await?;

    let existing_invoices: Vec<ExistingInvoiceSummary> = other_rows
        .into_iter()
        .map(|r| ExistingInvoiceSummary {
            id: r.get("id"),
            document_id: r.get("document_id"),
            sha256_hash: r.get("sha256_hash"),
            invoice_number: r.get("invoice_number"),
            vendor_id: r.get("vendor_id"),
            invoice_date: r.get("invoice_date"),
            currency: r.get("currency"),
            stated_total: r.get("stated_total"),
        })
        .collect();

    // Load payments & explicit allocations
    let pay_rows = sqlx::query(
        r#"
        SELECT p.id, p.document_id, p.payment_reference, p.bank_reference, p.vendor_id,
               p.payment_date, p.amount, p.currency, p.referenced_invoice_number,
               p.transaction_status, p.is_refund_or_credit,
               pa.allocated_amount AS explicit_allocated
        FROM payments p
        LEFT JOIN payment_allocations pa ON pa.payment_id = p.id AND pa.invoice_id = $2
        WHERE p.vendor_id = $1
        "#,
    )
    .bind(vendor_id)
    .bind(invoice_id)
    .fetch_all(&state.db)
    .await?;

    let payments: Vec<PaymentInput> = pay_rows
        .into_iter()
        .map(|r| PaymentInput {
            id: r.get("id"),
            document_id: r.get("document_id"),
            payment_reference: r.get("payment_reference"),
            bank_reference: r.get("bank_reference"),
            vendor_id: r.get("vendor_id"),
            payment_date: r.get("payment_date"),
            amount: r.get("amount"),
            currency: r.get("currency"),
            referenced_invoice_number: r.get("referenced_invoice_number"),
            transaction_status: r.get("transaction_status"),
            is_refund_or_credit: r.get("is_refund_or_credit"),
            explicit_allocated_to_this_invoice: r.get("explicit_allocated"),
        })
        .collect();

    let severity_rules = state.load_severity_rules().await;

    // AGENT 4 — DOCUMENT_MATCHER + AGENT 5 — RECONCILIATION_AGENT (authoritative deterministic Rust engine)
    let t_start = Instant::now();
    let output = run_reconciliation(
        &invoice_input,
        inv_sha256.as_deref(),
        &contract_versions,
        &pos,
        &existing_invoices,
        &payments,
        &severity_rules,
    );
    let elapsed_ms = t_start.elapsed().as_millis() as i32;

    // Persist reconciliation job, stage results, findings, and evidence atomically in a transaction
    let mut tx = state.db.begin().await?;

    let recon_job_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO reconciliation_jobs (
            idempotency_key, invoice_id, contract_version_id, purchase_order_id,
            engine_version, status, triggered_by, completed_at, summary_json
        )
        VALUES ($1,$2,$3,$4,$5,$6,$7,NOW(),$8)
        RETURNING id
        "#,
    )
    .bind(&job_idem)
    .bind(invoice_id)
    .bind(output.matched_contract_version_id)
    .bind(output.matched_po_id)
    .bind(&output.engine_version)
    .bind(&output.overall_status)
    .bind(actor_id)
    .bind(serde_json::json!({
        "overall_status": output.overall_status,
        "payment_status": output.payment_status,
        "findings_count": output.findings.len(),
        "recalculated_total": output.recalculated_total,
        "stated_total": invoice_input.stated_total
    }))
    .fetch_one(&mut *tx)
    .await?;

    // Clear previous open automated findings for this invoice so re-running reconciliation is idempotent (preserves human-reviewed findings!)
    sqlx::query(
        "DELETE FROM anomaly_findings WHERE invoice_id = $1 AND reviewer_status = 'pending'",
    )
    .bind(invoice_id)
    .execute(&mut *tx)
    .await?;

    for stage in &output.stages {
        sqlx::query(
            r#"
            INSERT INTO reconciliation_results (
                reconciliation_job_id, invoice_id, stage_code, stage_name, status,
                invoice_line_item_id, contract_line_item_id, expected_value, actual_value,
                variance_amount, variance_pct, permitted_tolerance, amount_outside_tolerance,
                currency, rule_applied, calculation_details
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)
            "#,
        )
        .bind(recon_job_id)
        .bind(invoice_id)
        .bind(&stage.stage_code)
        .bind(&stage.stage_name)
        .bind(&stage.status)
        .bind(stage.invoice_line_item_id)
        .bind(stage.contract_line_item_id)
        .bind(stage.expected_value)
        .bind(stage.actual_value)
        .bind(stage.variance_amount)
        .bind(stage.variance_pct)
        .bind(&stage.permitted_tolerance)
        .bind(stage.amount_outside_tolerance)
        .bind(&stage.currency)
        .bind(&stage.rule_applied)
        .bind(&stage.calculation_details)
        .execute(&mut *tx)
        .await?;
    }

    let mut finding_ids = Vec::new();
    for f in &output.findings {
        let fid: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO anomaly_findings (
                reconciliation_job_id, category, classification_type, severity,
                severity_rule_applied, status, vendor_id, invoice_id, contract_id,
                document_id, invoice_line_item_id, expected_value, actual_value,
                variance_amount, currency, rule_applied, explanation, calculation_details,
                extraction_provenance
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)
            RETURNING id
            "#,
        )
        .bind(recon_job_id)
        .bind(&f.category)
        .bind(&f.classification_type)
        .bind(&f.severity)
        .bind(&f.severity_rule_applied)
        .bind(&f.status)
        .bind(vendor_id)
        .bind(invoice_id)
        .bind(f.contract_id)
        .bind(inv_doc_id)
        .bind(f.invoice_line_item_id)
        .bind(f.expected_value)
        .bind(f.actual_value)
        .bind(f.variance_amount)
        .bind(&f.currency)
        .bind(&f.rule_applied)
        .bind(&f.explanation)
        .bind(&f.calculation_details)
        .bind(serde_json::json!({ "engine_version": RECONCILIATION_ENGINE_VERSION }))
        .fetch_one(&mut *tx)
        .await?;

        finding_ids.push(fid);

        for ev in &f.evidence_refs {
            sqlx::query(
                r#"
                INSERT INTO evidence_references (
                    finding_id, document_id, evidence_role, page_number,
                    bounding_box, quoted_text, source_location
                )
                VALUES ($1,$2,$3,$4,$5,$6,$7)
                "#,
            )
            .bind(fid)
            .bind(ev.document_id)
            .bind(&ev.evidence_role)
            .bind(ev.page_number)
            .bind(&ev.bounding_box)
            .bind(&ev.quoted_text)
            .bind(&ev.source_location)
            .execute(&mut *tx)
            .await?;
        }
    }

    // Update invoice line match metadata & invoice totals/status
    for lm in &output.line_matches {
        sqlx::query(
            r#"
            UPDATE invoice_line_items
            SET matched_contract_item_id = $2,
                matched_po_item_id = $3,
                match_method = $4,
                match_requires_review = $5
            WHERE id = $1
            "#,
        )
        .bind(lm.invoice_line_item_id)
        .bind(lm.matched_contract_item_id)
        .bind(lm.matched_po_item_id)
        .bind(&lm.match_method)
        .bind(lm.match_requires_review)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query(
        r#"
        UPDATE invoices
        SET matched_contract_id = $2,
            matched_contract_version_id = $3,
            matched_po_id = $4,
            match_confidence_status = $5,
            recalculated_subtotal = $6,
            recalculated_tax_total = $7,
            recalculated_total = $8,
            reconciliation_status = $9,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(invoice_id)
    .bind(output.matched_contract_id)
    .bind(output.matched_contract_version_id)
    .bind(output.matched_po_id)
    .bind(&output.match_confidence_status)
    .bind(output.recalculated_subtotal)
    .bind(output.recalculated_tax_total)
    .bind(output.recalculated_total)
    .bind(&output.overall_status)
    .execute(&mut *tx)
    .await?;

    if let Some(doc_id) = inv_doc_id {
        sqlx::query(
            "UPDATE documents SET matching_status = $2, reconciliation_status = $3, updated_at = NOW() WHERE id = $1",
        )
        .bind(doc_id)
        .bind(&output.match_confidence_status)
        .bind(&output.overall_status)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    // Record auditable Agent Runs 4, 5, 6, 7, 8
    record_agent_run(
        state,
        workflow_run_id,
        4,
        "DOCUMENT_MATCHER",
        &format!("{}:4:{}", workflow_run_id, invoice_id),
        inv_doc_id,
        Some(invoice_id),
        if output.match_confidence_status == "deterministic_matched" { "completed" } else { "routed_to_human" },
        RECONCILIATION_ENGINE_VERSION,
        serde_json::json!({ "invoice_id": invoice_id }),
        serde_json::json!({
            "matched_contract_id": output.matched_contract_id,
            "matched_contract_version_id": output.matched_contract_version_id,
            "matched_po_id": output.matched_po_id,
            "match_status": output.match_confidence_status
        }),
        serde_json::json!([{ "invoice_id": invoice_id }]),
        None,
        elapsed_ms,
    )
    .await?;

    record_agent_run(
        state,
        workflow_run_id,
        5,
        "RECONCILIATION_AGENT",
        &format!("{}:5:{}", workflow_run_id, invoice_id),
        inv_doc_id,
        Some(invoice_id),
        "completed",
        RECONCILIATION_ENGINE_VERSION,
        serde_json::json!({ "invoice_id": invoice_id, "reconciliation_job_id": recon_job_id }),
        serde_json::json!({
            "overall_status": output.overall_status,
            "stages_evaluated": output.stages.len(),
            "recalculated_total": output.recalculated_total,
            "payment_status": output.payment_status
        }),
        serde_json::json!([{ "reconciliation_job_id": recon_job_id }]),
        None,
        elapsed_ms,
    )
    .await?;

    record_agent_run(
        state,
        workflow_run_id,
        6,
        "ANOMALY_ANALYST",
        &format!("{}:6:{}", workflow_run_id, invoice_id),
        inv_doc_id,
        Some(invoice_id),
        "completed",
        RECONCILIATION_ENGINE_VERSION,
        serde_json::json!({ "reconciliation_job_id": recon_job_id }),
        serde_json::json!({
            "findings_detected": output.findings.len(),
            "confirmed_violations": output.findings.iter().filter(|f| f.classification_type == "confirmed_rule_violation" || f.classification_type == "exact_duplicate").count(),
            "potential_or_unresolved": output.findings.iter().filter(|f| f.classification_type != "confirmed_rule_violation" && f.classification_type != "exact_duplicate").count()
        }),
        serde_json::json!(finding_ids),
        None,
        1,
    )
    .await?;

    record_agent_run(
        state,
        workflow_run_id,
        7,
        "AUDIT_REPORT_GENERATOR",
        &format!("{}:7:{}", workflow_run_id, invoice_id),
        inv_doc_id,
        Some(invoice_id),
        "completed",
        WORKFLOW_RULE_VERSION,
        serde_json::json!({ "reconciliation_job_id": recon_job_id }),
        serde_json::json!({ "finding_ids": finding_ids }),
        serde_json::json!(finding_ids),
        None,
        1,
    )
    .await?;

    if output.overall_status != "reconciled" {
        record_agent_run(
            state,
            workflow_run_id,
            8,
            "REVIEW_COORDINATOR",
            &format!("{}:8:{}", workflow_run_id, invoice_id),
            inv_doc_id,
            Some(invoice_id),
            "routed_to_human",
            WORKFLOW_RULE_VERSION,
            serde_json::json!({ "overall_status": output.overall_status, "findings_count": output.findings.len() }),
            serde_json::json!({ "routed_for_human_review": true, "finding_ids": finding_ids }),
            serde_json::json!(finding_ids),
            None,
            1,
        )
        .await?;
    }

    state
        .log_audit(
            actor_id,
            None,
            "reconciliation.executed",
            "invoice",
            Some(&invoice_id.to_string()),
            serde_json::json!({
                "reconciliation_job_id": recon_job_id,
                "workflow_run_id": workflow_run_id,
                "overall_status": output.overall_status,
                "findings_count": output.findings.len()
            }),
        )
        .await;

    Ok((recon_job_id, output))
}
