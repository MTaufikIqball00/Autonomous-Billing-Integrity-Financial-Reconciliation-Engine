use super::{pagination_json, AuthenticatedUser, PaginationQuery};
use crate::{
    agents::run_document_pipeline,
    error::{AppError, AppResult},
    materialization::{materialize_document, preview_materialization, MaterializeRequest},
    security::require_permission,
    state::{parse_uuid, AppState},
};
use axum::{
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    Json,
};
use rust_decimal::Decimal;
use serde::Deserialize;
use sqlx::Row;
use uuid::Uuid;

pub async fn upload_document(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    mut multipart: Multipart,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    require_permission(&claims, "documents:write")?;
    state.api_rate_limiter.check(&format!("upload:{}", claims.sub))?;

    let mut file_name = String::new();
    let mut declared_mime = String::new();
    let mut file_bytes: Vec<u8> = Vec::new();
    let mut vendor_id: Option<Uuid> = None;
    let mut process_sync = true;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Multipart error: {}", e)))?
    {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" {
            file_name = field.file_name().unwrap_or("upload.bin").to_string();
            declared_mime = field.content_type().unwrap_or("").to_string();
            let data = field
                .bytes()
                .await
                .map_err(|e| AppError::BadRequest(format!("Failed reading uploaded file: {}", e)))?;
            file_bytes = data.to_vec();
        } else if name == "vendor_id" {
            if let Ok(t) = field.text().await {
                if !t.trim().is_empty() {
                    vendor_id = Some(parse_uuid(t.trim(), "vendor_id")?);
                }
            }
        } else if name == "async_only" {
            if let Ok(t) = field.text().await {
                if t.eq_ignore_ascii_case("true") {
                    process_sync = false;
                }
            }
        }
    }

    if file_bytes.is_empty() {
        return Err(AppError::BadRequest("No file provided in 'file' form field.".into()));
    }

    let validated = state
        .storage
        .store_immutable(&file_name, &declared_mime, &file_bytes)
        .await?;

    let existing_dup: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM documents WHERE sha256_hash = $1 ORDER BY created_at ASC LIMIT 1",
    )
    .bind(&validated.sha256_hash)
    .fetch_optional(&state.db)
    .await?;

    let doc_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO documents (
            original_filename, stored_blob_key, sha256_hash, file_size_bytes,
            declared_mime_type, detected_mime_type, file_extension,
            is_duplicate_file, duplicate_of_document_id, vendor_id, uploaded_by
        )
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
        RETURNING id
        "#,
    )
    .bind(&validated.original_filename)
    .bind(&validated.stored_blob_key)
    .bind(&validated.sha256_hash)
    .bind(validated.file_size_bytes)
    .bind(&validated.declared_mime)
    .bind(&validated.detected_mime)
    .bind(&validated.extension)
    .bind(existing_dup.is_some())
    .bind(existing_dup)
    .bind(vendor_id)
    .bind(claims.sub)
    .fetch_one(&state.db)
    .await?;

    sqlx::query(
        "INSERT INTO document_versions (document_id, version_number, sha256_hash, stored_blob_key, change_note, created_by) VALUES ($1, 1, $2, $3, 'Initial immutable upload', $4)",
    )
    .bind(doc_id)
    .bind(&validated.sha256_hash)
    .bind(&validated.stored_blob_key)
    .bind(claims.sub)
    .execute(&state.db)
    .await?;

    // Avoid race condition: sync uploads start pre-locked as 'running' and are NOT queued for background workers
    let job_id: Uuid = if process_sync {
        sqlx::query_scalar(
            r#"
            INSERT INTO processing_jobs (idempotency_key, job_type, target_id, status, attempts, locked_at, payload)
            VALUES ($1, 'document_pipeline', $2, 'running', 1, NOW(), $3)
            RETURNING id
            "#,
        )
        .bind(format!("doc_pipeline:{}", doc_id))
        .bind(doc_id)
        .bind(serde_json::json!({ "document_id": doc_id, "actor_id": claims.sub }))
        .fetch_one(&state.db)
        .await?
    } else {
        let jid: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO processing_jobs (idempotency_key, job_type, target_id, status, attempts, locked_at, payload)
            VALUES ($1, 'document_pipeline', $2, 'queued', 0, NULL, $3)
            RETURNING id
            "#,
        )
        .bind(format!("doc_pipeline:{}", doc_id))
        .bind(doc_id)
        .bind(serde_json::json!({ "document_id": doc_id, "actor_id": claims.sub }))
        .fetch_one(&state.db)
        .await?;
        state.redis.publish_job_signal("abifr:jobs", &jid.to_string()).await;
        jid
    };

    let mut pipeline_error: Option<String> = None;
    if process_sync {
        match run_document_pipeline(&state, doc_id, Some(claims.sub)).await {
            Ok(_) => {
                let _ = sqlx::query(
                    "UPDATE processing_jobs SET status = 'completed', locked_at = NULL, completed_at = NOW(), updated_at = NOW() WHERE id = $1",
                )
                .bind(job_id)
                .execute(&state.db)
                .await;
            }
            Err(e) => {
                pipeline_error = Some(e.to_string());
                let _ = sqlx::query(
                    "UPDATE processing_jobs SET status = 'failed', locked_at = NULL, last_error = $2, updated_at = NOW() WHERE id = $1",
                )
                .bind(job_id)
                .bind(&pipeline_error)
                .execute(&state.db)
                .await;
            }
        }
    }

    state
        .log_audit(
            Some(claims.sub),
            Some(&claims.role),
            "document.uploaded",
            "document",
            Some(&doc_id.to_string()),
            serde_json::json!({
                "filename": validated.original_filename,
                "sha256": validated.sha256_hash,
                "is_duplicate_file": existing_dup.is_some(),
                "detected_mime": validated.detected_mime
            }),
        )
        .await;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "id": doc_id,
            "job_id": job_id,
            "original_filename": validated.original_filename,
            "sha256_hash": validated.sha256_hash,
            "detected_mime_type": validated.detected_mime,
            "file_size_bytes": validated.file_size_bytes,
            "is_duplicate_file": existing_dup.is_some(),
            "duplicate_of_document_id": existing_dup,
            "processing_error": pipeline_error
        })),
    ))
}

pub async fn list_documents(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(pg): Query<PaginationQuery>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "documents:read")?;
    let (limit, offset) = pg.resolve();
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents")
        .fetch_one(&state.db)
        .await?;

    let rows = sqlx::query(
        r#"
        SELECT d.id, d.original_filename, d.sha256_hash, d.file_size_bytes, d.detected_mime_type,
               d.file_extension, d.page_count, d.ai_category, d.ai_classification_confidence,
               d.effective_category, d.processing_status, d.ocr_status, d.extraction_status,
               d.matching_status, d.reconciliation_status, d.failure_reason,
               d.is_duplicate_file, d.duplicate_of_document_id, d.prompt_injection_flagged,
               d.materialized_entity_type, d.materialized_entity_id,
               d.is_synthetic_fixture, d.created_at, v.legal_name AS vendor_name
        FROM documents d
        LEFT JOIN vendors v ON v.id = d.vendor_id
        ORDER BY d.created_at DESC
        LIMIT $1 OFFSET $2
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await?;

    let items: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<Uuid, _>("id"),
                "original_filename": r.get::<String, _>("original_filename"),
                "sha256_hash": r.get::<String, _>("sha256_hash"),
                "file_size_bytes": r.get::<i64, _>("file_size_bytes"),
                "detected_mime_type": r.get::<String, _>("detected_mime_type"),
                "file_extension": r.get::<String, _>("file_extension"),
                "page_count": r.get::<Option<i32>, _>("page_count"),
                "ai_category": r.get::<String, _>("ai_category"),
                "ai_classification_confidence": r.get::<Option<Decimal>, _>("ai_classification_confidence"),
                "effective_category": r.get::<String, _>("effective_category"),
                "processing_status": r.get::<String, _>("processing_status"),
                "ocr_status": r.get::<String, _>("ocr_status"),
                "extraction_status": r.get::<String, _>("extraction_status"),
                "matching_status": r.get::<String, _>("matching_status"),
                "reconciliation_status": r.get::<String, _>("reconciliation_status"),
                "failure_reason": r.get::<Option<String>, _>("failure_reason"),
                "is_duplicate_file": r.get::<bool, _>("is_duplicate_file"),
                "duplicate_of_document_id": r.get::<Option<Uuid>, _>("duplicate_of_document_id"),
                "prompt_injection_flagged": r.get::<bool, _>("prompt_injection_flagged"),
                "materialized_entity_type": r.get::<Option<String>, _>("materialized_entity_type"),
                "materialized_entity_id": r.get::<Option<Uuid>, _>("materialized_entity_id"),
                "is_synthetic_fixture": r.get::<bool, _>("is_synthetic_fixture"),
                "vendor_name": r.get::<Option<String>, _>("vendor_name"),
                "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at")
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "documents": items,
        "pagination": pagination_json(total, limit, offset)
    })))
}

pub async fn get_document_detail(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "documents:read")?;
    let doc_id = parse_uuid(&id, "document_id")?;

    let d = sqlx::query(
        r#"
        SELECT id, original_filename, sha256_hash, file_size_bytes, declared_mime_type,
               detected_mime_type, file_extension, page_count, ai_category,
               ai_classification_confidence, effective_category, processing_status,
               ocr_status, extraction_status, matching_status, reconciliation_status,
               failure_reason, is_duplicate_file, duplicate_of_document_id,
               prompt_injection_flagged, materialized_entity_type, materialized_entity_id,
               materialized_at, is_synthetic_fixture, created_at
        FROM documents WHERE id = $1
        "#,
    )
    .bind(doc_id)
    .fetch_one(&state.db)
    .await?;

    let latest_ext = sqlx::query(
        r#"
        SELECT id, extraction_method, provider_name, model_version, raw_text, pages_json,
               tables_json, structured_payload, prompt_injection_signals, redaction_applied,
               duration_ms, created_at
        FROM document_extractions
        WHERE document_id = $1
        ORDER BY created_at DESC LIMIT 1
        "#,
    )
    .bind(doc_id)
    .fetch_optional(&state.db)
    .await?;

    let fields = if let Some(ext) = &latest_ext {
        let ext_id: Uuid = ext.get("id");
        let rows = sqlx::query(
            r#"
            SELECT id, field_name, line_item_index, extracted_value, original_text,
                   page_number, bounding_box, source_location, extraction_method,
                   provider_confidence, validation_status, normalized_value,
                   normalized_decimal, normalized_currency, normalization_rule,
                   human_override_value, human_override_reason, human_override_at
            FROM extracted_fields
            WHERE extraction_id = $1
            ORDER BY line_item_index NULLS FIRST, field_name ASC
            "#,
        )
        .bind(ext_id)
        .fetch_all(&state.db)
        .await?;

        rows.into_iter()
            .map(|r| {
                serde_json::json!({
                    "id": r.get::<Uuid, _>("id"),
                    "field_name": r.get::<String, _>("field_name"),
                    "line_item_index": r.get::<Option<i32>, _>("line_item_index"),
                    "extracted_value": r.get::<Option<String>, _>("extracted_value"),
                    "original_text": r.get::<Option<String>, _>("original_text"),
                    "page_number": r.get::<Option<i32>, _>("page_number"),
                    "bounding_box": r.get::<Option<serde_json::Value>, _>("bounding_box"),
                    "source_location": r.get::<Option<String>, _>("source_location"),
                    "extraction_method": r.get::<String, _>("extraction_method"),
                    "provider_confidence": r.get::<Option<Decimal>, _>("provider_confidence"),
                    "validation_status": r.get::<String, _>("validation_status"),
                    "normalized_value": r.get::<Option<String>, _>("normalized_value"),
                    "normalized_decimal": r.get::<Option<Decimal>, _>("normalized_decimal"),
                    "normalized_currency": r.get::<Option<String>, _>("normalized_currency"),
                    "normalization_rule": r.get::<Option<String>, _>("normalization_rule"),
                    "human_override_value": r.get::<Option<String>, _>("human_override_value"),
                    "human_override_reason": r.get::<Option<String>, _>("human_override_reason"),
                    "human_override_at": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("human_override_at")
                })
            })
            .collect::<Vec<_>>()
    } else {
        vec![]
    };

    let extraction_json = latest_ext.map(|e| {
        serde_json::json!({
            "id": e.get::<Uuid, _>("id"),
            "extraction_method": e.get::<String, _>("extraction_method"),
            "provider_name": e.get::<String, _>("provider_name"),
            "model_version": e.get::<String, _>("model_version"),
            "raw_text": e.get::<String, _>("raw_text"),
            "pages": e.get::<serde_json::Value, _>("pages_json"),
            "tables": e.get::<serde_json::Value, _>("tables_json"),
            "structured_payload": e.get::<serde_json::Value, _>("structured_payload"),
            "prompt_injection_signals": e.get::<serde_json::Value, _>("prompt_injection_signals"),
            "redaction_applied": e.get::<bool, _>("redaction_applied"),
            "duration_ms": e.get::<Option<i32>, _>("duration_ms"),
            "created_at": e.get::<chrono::DateTime<chrono::Utc>, _>("created_at")
        })
    });

    Ok(Json(serde_json::json!({
        "document": {
            "id": d.get::<Uuid, _>("id"),
            "original_filename": d.get::<String, _>("original_filename"),
            "sha256_hash": d.get::<String, _>("sha256_hash"),
            "file_size_bytes": d.get::<i64, _>("file_size_bytes"),
            "declared_mime_type": d.get::<String, _>("declared_mime_type"),
            "detected_mime_type": d.get::<String, _>("detected_mime_type"),
            "file_extension": d.get::<String, _>("file_extension"),
            "page_count": d.get::<Option<i32>, _>("page_count"),
            "ai_category": d.get::<String, _>("ai_category"),
            "ai_classification_confidence": d.get::<Option<Decimal>, _>("ai_classification_confidence"),
            "effective_category": d.get::<String, _>("effective_category"),
            "processing_status": d.get::<String, _>("processing_status"),
            "ocr_status": d.get::<String, _>("ocr_status"),
            "extraction_status": d.get::<String, _>("extraction_status"),
            "matching_status": d.get::<String, _>("matching_status"),
            "reconciliation_status": d.get::<String, _>("reconciliation_status"),
            "failure_reason": d.get::<Option<String>, _>("failure_reason"),
            "is_duplicate_file": d.get::<bool, _>("is_duplicate_file"),
            "prompt_injection_flagged": d.get::<bool, _>("prompt_injection_flagged"),
            "materialized_entity_type": d.get::<Option<String>, _>("materialized_entity_type"),
            "materialized_entity_id": d.get::<Option<Uuid>, _>("materialized_entity_id"),
            "materialized_at": d.get::<Option<chrono::DateTime<chrono::Utc>>, _>("materialized_at"),
            "is_synthetic_fixture": d.get::<bool, _>("is_synthetic_fixture"),
            "created_at": d.get::<chrono::DateTime<chrono::Utc>, _>("created_at")
        },
        "extraction": extraction_json,
        "extracted_fields": fields
    })))
}

pub async fn reprocess_document(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "documents:write")?;
    let doc_id = parse_uuid(&id, "document_id")?;
    let workflow_run_id = run_document_pipeline(&state, doc_id, Some(claims.sub)).await?;
    Ok(Json(serde_json::json!({ "status": "completed", "workflow_run_id": workflow_run_id })))
}

#[derive(Deserialize)]
pub struct ReclassifyRequest {
    pub category: String,
    pub reason: String,
}

pub async fn reclassify_document(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<String>,
    Json(req): Json<ReclassifyRequest>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "findings:review")?;
    let doc_id = parse_uuid(&id, "document_id")?;

    let allowed = [
        "contract",
        "contract_amendment",
        "invoice",
        "purchase_order",
        "delivery_receipt",
        "service_acceptance",
        "payment_record",
        "credit_note",
        "debit_note",
        "other_supporting",
        "unknown",
    ];
    if !allowed.contains(&req.category.as_str()) {
        return Err(AppError::BadRequest("Invalid document category.".into()));
    }

    let prev: String = sqlx::query_scalar("SELECT effective_category FROM documents WHERE id = $1")
        .bind(doc_id)
        .fetch_one(&state.db)
        .await?;

    sqlx::query(
        r#"
        UPDATE documents
        SET effective_category = $2,
            classification_locked_by = $3,
            classification_locked_at = NOW(),
            processing_status = CASE WHEN processing_status = 'requires_review' THEN 'completed' ELSE processing_status END,
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(doc_id)
    .bind(&req.category)
    .bind(claims.sub)
    .execute(&state.db)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO review_actions (target_type, target_id, action_type, reviewer_id, previous_state, new_state, notes)
        VALUES ('document', $1, 'reclassify', $2, $3, $4, $5)
        "#,
    )
    .bind(doc_id)
    .bind(claims.sub)
    .bind(serde_json::json!({ "effective_category": prev }))
    .bind(serde_json::json!({ "effective_category": req.category }))
    .bind(&req.reason)
    .execute(&state.db)
    .await?;

    state
        .log_audit(
            Some(claims.sub),
            Some(&claims.role),
            "document.reclassified",
            "document",
            Some(&doc_id.to_string()),
            serde_json::json!({ "from": prev, "to": req.category, "reason": req.reason }),
        )
        .await;

    Ok(Json(serde_json::json!({ "status": "updated", "effective_category": req.category })))
}

pub async fn render_document_page(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path((id, page)): Path<(String, i32)>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "documents:read")?;
    let doc_id = parse_uuid(&id, "document_id")?;

    let row = sqlx::query("SELECT stored_blob_key, file_extension FROM documents WHERE id = $1")
        .bind(doc_id)
        .fetch_one(&state.db)
        .await?;
    let blob_key: String = row.get("stored_blob_key");
    let ext: String = row.get("file_extension");
    let bytes = state.storage.read_blob(&blob_key).await?;

    let rendered = state.doc_intel.render_page(&bytes, &ext, page).await?;
    Ok(Json(serde_json::to_value(rendered).unwrap()))
}

#[derive(Deserialize)]
pub struct CorrectFieldRequest {
    pub corrected_value: String,
    pub reason: String,
}

pub async fn correct_extracted_field(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<String>,
    Json(req): Json<CorrectFieldRequest>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "findings:review")?;
    let field_id = parse_uuid(&id, "extracted_field_id")?;

    let prev_row = sqlx::query(
        "SELECT field_name, extracted_value, human_override_value FROM extracted_fields WHERE id = $1",
    )
    .bind(field_id)
    .fetch_one(&state.db)
    .await?;

    sqlx::query(
        r#"
        UPDATE extracted_fields
        SET human_override_value = $2,
            human_override_by = $3,
            human_override_at = NOW(),
            human_override_reason = $4,
            validation_status = 'human_corrected'
        WHERE id = $1
        "#,
    )
    .bind(field_id)
    .bind(&req.corrected_value)
    .bind(claims.sub)
    .bind(&req.reason)
    .execute(&state.db)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO review_actions (target_type, target_id, action_type, reviewer_id, previous_state, new_state, notes)
        VALUES ('extracted_field', $1, 'correct_value', $2, $3, $4, $5)
        "#,
    )
    .bind(field_id)
    .bind(claims.sub)
    .bind(serde_json::json!({
        "field_name": prev_row.get::<String, _>("field_name"),
        "original_extracted_value": prev_row.get::<Option<String>, _>("extracted_value"),
        "previous_override": prev_row.get::<Option<String>, _>("human_override_value")
    }))
    .bind(serde_json::json!({ "human_override_value": req.corrected_value }))
    .bind(&req.reason)
    .execute(&state.db)
    .await?;

    Ok(Json(serde_json::json!({
        "status": "corrected",
        "original_preserved": true,
        "human_override_value": req.corrected_value
    })))
}

pub async fn get_materialization_preview(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "documents:read")?;
    let doc_id = parse_uuid(&id, "document_id")?;
    let preview = preview_materialization(&state, doc_id).await?;
    Ok(Json(serde_json::to_value(preview).unwrap()))
}

pub async fn materialize_document_handler(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<String>,
    Json(req): Json<MaterializeRequest>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    require_permission(&claims, "documents:write")?;
    let doc_id = parse_uuid(&id, "document_id")?;
    let res = materialize_document(&state, doc_id, req, Some(claims.sub)).await?;
    Ok((StatusCode::CREATED, Json(serde_json::to_value(res).unwrap())))
}
