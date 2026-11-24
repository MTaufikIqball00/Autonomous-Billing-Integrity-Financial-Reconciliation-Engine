use crate::{
    agents::{run_document_pipeline, run_invoice_reconciliation_workflow},
    error::{AppError, AppResult},
    fixtures::load_synthetic_fixtures,
    reporting::{build_audit_report, export_report_csv, export_report_pdf, export_report_xlsx},
    security::{hash_password, issue_jwt, require_permission, verify_jwt, verify_password, AuthClaims},
    state::{parse_uuid, AppState},
};
use axum::{
    extract::{FromRequestParts, Multipart, Path, Query, State},
    http::{header, request::Parts, HeaderMap, HeaderValue, Method, StatusCode},
    response::IntoResponse,
    routing::{get, patch, post},
    Json, Router,
};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Deserialize;
use sqlx::Row;
use uuid::Uuid;

pub struct AuthenticatedUser(pub AuthClaims);

impl FromRequestParts<AppState> for AuthenticatedUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let mut token_from_cookie = false;
        let mut token: Option<String> = None;

        if let Some(auth) = parts.headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) {
            if let Some(stripped) = auth.strip_prefix("Bearer ") {
                token = Some(stripped.trim().to_string());
            }
        }

        if token.is_none() {
            if let Some(cookies) = parts.headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) {
                for pair in cookies.split(';') {
                    let trimmed = pair.trim();
                    if let Some(val) = trimmed.strip_prefix("abifr_session=") {
                        token = Some(val.to_string());
                        token_from_cookie = true;
                        break;
                    }
                }
            }
        }

        let raw_token = token.ok_or_else(|| AppError::Unauthorized("Missing authentication credentials.".into()))?;
        let claims = verify_jwt(&raw_token, &state.config.jwt_secret)?;

        // CSRF protection for state-changing requests authenticated via cookies
        if token_from_cookie
            && matches!(
                parts.method,
                Method::POST | Method::PUT | Method::PATCH | Method::DELETE
            )
        {
            let csrf_header = parts
                .headers
                .get("X-CSRF-Token")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if csrf_header != claims.csrf {
                return Err(AppError::Forbidden("CSRF token mismatch.".into()));
            }
        }

        Ok(AuthenticatedUser(claims))
    }
}

pub fn build_router(state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(health_handler))
        // Auth & Users
        .route("/auth/login", post(login_handler))
        .route("/auth/logout", post(logout_handler))
        .route("/auth/me", get(me_handler))
        .route("/users", get(list_users).post(create_user))
        // Dashboard Overview
        .route("/dashboard/overview", get(dashboard_overview))
        // Documents & Extraction Viewer
        .route("/documents/upload", post(upload_document))
        .route("/documents", get(list_documents))
        .route("/documents/{id}", get(get_document_detail))
        .route("/documents/{id}/reprocess", post(reprocess_document))
        .route("/documents/{id}/classify", patch(reclassify_document))
        .route("/documents/{id}/page/{page}", get(render_document_page))
        .route("/extracted-fields/{id}/correct", patch(correct_extracted_field))
        // Vendors
        .route("/vendors", get(list_vendors).post(create_vendor))
        // Contracts & Amendments
        .route("/contracts", get(list_contracts).post(create_contract))
        .route("/contracts/{id}", get(get_contract_detail))
        .route("/contracts/{id}/versions", post(add_contract_version))
        // Purchase Orders
        .route("/purchase-orders", get(list_purchase_orders).post(create_purchase_order))
        // Invoices & Reconciliation
        .route("/invoices", get(list_invoices).post(create_invoice))
        .route("/invoices/{id}", get(get_invoice_detail))
        .route("/invoices/{id}/reconcile", post(reconcile_invoice_handler))
        .route("/invoices/{id}/confirm-match", patch(confirm_invoice_match))
        // Payments
        .route("/payments", get(list_payments).post(create_payment))
        .route("/payments/{id}/allocate", post(allocate_payment))
        // Findings & Human Review
        .route("/findings", get(list_findings))
        .route("/findings/{id}", get(get_finding_detail))
        .route("/findings/{id}/review", post(review_finding))
        .route("/review-queue", get(get_review_queue))
        // Agents
        .route("/agents/runs", get(list_agent_runs))
        // Reports & Exports
        .route("/reports", get(list_reports))
        .route("/reports/generate", post(generate_report_handler))
        .route("/reports/{id}/export", get(export_report_handler))
        // Models, Settings, Audit Logs & Synthetic Fixtures
        .route("/models", get(list_models))
        .route("/models/{provider}", patch(update_model_config))
        .route("/settings", get(list_settings))
        .route("/settings/{key}", patch(update_setting))
        .route("/audit-logs", get(list_audit_logs))
        .route("/fixtures/load-synthetic", post(load_synthetic_fixtures_handler));

    Router::new()
        .nest("/api/v1", api)
        .with_state(state)
}

// =============================================================================
// Health
// =============================================================================
async fn health_handler(State(state): State<AppState>) -> AppResult<Json<serde_json::Value>> {
    let db_ok = sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.db)
        .await
        .is_ok();
    let doc_intel = state.doc_intel.health().await;

    Ok(Json(serde_json::json!({
        "status": if db_ok { "ok" } else { "degraded" },
        "backend": "rust-axum",
        "backend_port": state.config.backend_port,
        "postgresql": if db_ok { "connected" } else { "unreachable" },
        "redis": if state.redis.is_connected() { "connected" } else { "optional_not_configured_using_pg_queue" },
        "doc_intel": doc_intel,
        "external_ai_enabled": state.config.enable_external_ai
    })))
}

// =============================================================================
// Auth & RBAC Users
// =============================================================================
#[derive(Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
}

async fn login_handler(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> AppResult<impl IntoResponse> {
    state.rate_limiter.check(&format!("login:{}", req.email.to_lowercase()))?;

    let row = sqlx::query(
        "SELECT id, email, full_name, password_hash, role_name, is_active FROM users WHERE LOWER(email) = LOWER($1)",
    )
    .bind(req.email.trim())
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::Unauthorized("Invalid email or password.".into()))?;

    let is_active: bool = row.get("is_active");
    if !is_active {
        return Err(AppError::Forbidden("Account is deactivated.".into()));
    }

    let hash: String = row.get("password_hash");
    if !verify_password(&req.password, &hash) {
        return Err(AppError::Unauthorized("Invalid email or password.".into()));
    }

    let user_id: Uuid = row.get("id");
    let email: String = row.get("email");
    let full_name: String = row.get("full_name");
    let role: String = row.get("role_name");

    let (token, csrf) = issue_jwt(user_id, &email, &full_name, &role, &state.config.jwt_secret)?;

    state
        .log_audit(
            Some(user_id),
            Some(&role),
            "auth.login",
            "user",
            Some(&user_id.to_string()),
            serde_json::json!({ "email": email }),
        )
        .await;

    let secure_attr = if state.config.cookie_secure { "; Secure" } else { "" };
    let cookie = format!(
        "abifr_session={}; HttpOnly; SameSite=Lax; Path=/; Max-Age=43200{}",
        token, secure_attr
    );

    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie).map_err(|_| AppError::Internal("Cookie format error".into()))?,
    );

    Ok((
        headers,
        Json(serde_json::json!({
            "token": token,
            "csrf_token": csrf,
            "user": {
                "id": user_id,
                "email": email,
                "full_name": full_name,
                "role": role
            }
        })),
    ))
}

async fn logout_handler() -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_static("abifr_session=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0"),
    );
    (headers, Json(serde_json::json!({ "status": "logged_out" })))
}

async fn me_handler(AuthenticatedUser(claims): AuthenticatedUser) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "id": claims.sub,
        "email": claims.email,
        "full_name": claims.full_name,
        "role": claims.role,
        "csrf_token": claims.csrf
    }))
}

async fn list_users(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "users:manage")?;
    let rows = sqlx::query(
        "SELECT id, email, full_name, role_name, is_active, created_at FROM users ORDER BY created_at DESC",
    )
    .fetch_all(&state.db)
    .await?;

    let items: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<Uuid, _>("id"),
                "email": r.get::<String, _>("email"),
                "full_name": r.get::<String, _>("full_name"),
                "role": r.get::<String, _>("role_name"),
                "is_active": r.get::<bool, _>("is_active"),
                "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at")
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "users": items })))
}

#[derive(Deserialize)]
struct CreateUserRequest {
    email: String,
    full_name: String,
    password: String,
    role: String,
}

async fn create_user(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Json(req): Json<CreateUserRequest>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    require_permission(&claims, "users:manage")?;
    let pw_hash = hash_password(&req.password)?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, full_name, password_hash, role_name) VALUES ($1,$2,$3,$4) RETURNING id",
    )
    .bind(req.email.trim())
    .bind(req.full_name.trim())
    .bind(pw_hash)
    .bind(req.role.trim())
    .fetch_one(&state.db)
    .await?;

    state
        .log_audit(
            Some(claims.sub),
            Some(&claims.role),
            "users.created",
            "user",
            Some(&id.to_string()),
            serde_json::json!({ "email": req.email, "role": req.role }),
        )
        .await;

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}

// =============================================================================
// Executive Overview (Live PostgreSQL Queries Only — Zero Fabricated Metrics)
// =============================================================================
async fn dashboard_overview(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "documents:read")?;

    let doc_stats = sqlx::query(
        r#"
        SELECT
          COUNT(*) AS total,
          COUNT(*) FILTER (WHERE processing_status = 'pending') AS pending,
          COUNT(*) FILTER (WHERE processing_status = 'processing') AS processing,
          COUNT(*) FILTER (WHERE processing_status = 'completed') AS completed,
          COUNT(*) FILTER (WHERE processing_status = 'failed') AS failed,
          COUNT(*) FILTER (WHERE processing_status = 'requires_review') AS requires_review,
          COUNT(*) FILTER (WHERE is_duplicate_file = TRUE) AS duplicate_files,
          COUNT(*) FILTER (WHERE prompt_injection_flagged = TRUE) AS prompt_injection_flagged
        FROM documents
        "#,
    )
    .fetch_one(&state.db)
    .await?;

    let inv_stats = sqlx::query(
        r#"
        SELECT
          COUNT(*) AS total,
          COUNT(*) FILTER (WHERE reconciliation_status = 'reconciled') AS reconciled,
          COUNT(*) FILTER (WHERE reconciliation_status = 'discrepancy_confirmed') AS discrepancy_confirmed,
          COUNT(*) FILTER (WHERE reconciliation_status = 'requires_review') AS requires_review,
          COUNT(*) FILTER (WHERE reconciliation_status = 'pending') AS pending
        FROM invoices
        "#,
    )
    .fetch_one(&state.db)
    .await?;

    let finding_stats = sqlx::query(
        r#"
        SELECT
          COUNT(*) AS total,
          COUNT(*) FILTER (WHERE classification_type IN ('confirmed_rule_violation','exact_duplicate')) AS confirmed,
          COUNT(*) FILTER (WHERE classification_type IN ('potential_anomaly','probable_duplicate')) AS potential,
          COUNT(*) FILTER (WHERE classification_type = 'unresolved_discrepancy') AS unresolved,
          COUNT(*) FILTER (WHERE reviewer_status = 'pending') AS pending_review
        FROM anomaly_findings
        "#,
    )
    .fetch_one(&state.db)
    .await?;

    // Segregated by currency — NEVER combined across currencies!
    let currency_rows = sqlx::query(
        r#"
        SELECT
          currency,
          COUNT(*) FILTER (WHERE classification_type IN ('confirmed_rule_violation','exact_duplicate')) AS confirmed_count,
          COALESCE(SUM(ABS(variance_amount)) FILTER (WHERE classification_type IN ('confirmed_rule_violation','exact_duplicate')), 0) AS confirmed_variance,
          COUNT(*) FILTER (WHERE classification_type NOT IN ('confirmed_rule_violation','exact_duplicate')) AS potential_count,
          COALESCE(SUM(ABS(variance_amount)) FILTER (WHERE classification_type NOT IN ('confirmed_rule_violation','exact_duplicate')), 0) AS potential_variance
        FROM anomaly_findings
        WHERE currency IS NOT NULL
        GROUP BY currency
        ORDER BY currency ASC
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    let currency_breakdown: Vec<serde_json::Value> = currency_rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "currency": r.get::<String, _>("currency"),
                "confirmed_count": r.get::<i64, _>("confirmed_count"),
                "confirmed_variance": r.get::<Decimal, _>("confirmed_variance"),
                "potential_count": r.get::<i64, _>("potential_count"),
                "potential_variance": r.get::<Decimal, _>("potential_variance")
            })
        })
        .collect();

    let vendor_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM vendors").fetch_one(&state.db).await?;
    let contract_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM contracts").fetch_one(&state.db).await?;
    let payment_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM payments").fetch_one(&state.db).await?;
    let unmatched_payments: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM payments WHERE allocation_status = 'unmatched'").fetch_one(&state.db).await?;

    Ok(Json(serde_json::json!({
        "documents": {
            "total": doc_stats.get::<i64, _>("total"),
            "pending": doc_stats.get::<i64, _>("pending"),
            "processing": doc_stats.get::<i64, _>("processing"),
            "completed": doc_stats.get::<i64, _>("completed"),
            "failed": doc_stats.get::<i64, _>("failed"),
            "requires_review": doc_stats.get::<i64, _>("requires_review"),
            "duplicate_files": doc_stats.get::<i64, _>("duplicate_files"),
            "prompt_injection_flagged": doc_stats.get::<i64, _>("prompt_injection_flagged")
        },
        "invoices": {
            "total": inv_stats.get::<i64, _>("total"),
            "reconciled": inv_stats.get::<i64, _>("reconciled"),
            "discrepancy_confirmed": inv_stats.get::<i64, _>("discrepancy_confirmed"),
            "requires_review": inv_stats.get::<i64, _>("requires_review"),
            "pending": inv_stats.get::<i64, _>("pending")
        },
        "findings": {
            "total": finding_stats.get::<i64, _>("total"),
            "confirmed": finding_stats.get::<i64, _>("confirmed"),
            "potential": finding_stats.get::<i64, _>("potential"),
            "unresolved": finding_stats.get::<i64, _>("unresolved"),
            "pending_review": finding_stats.get::<i64, _>("pending_review")
        },
        "counts": {
            "vendors": vendor_count,
            "contracts": contract_count,
            "payments": payment_count,
            "unmatched_payments": unmatched_payments
        },
        "currency_breakdown": currency_breakdown
    })))
}

// =============================================================================
// Document Ingestion, Storage, Processing, Viewer & Correction
// =============================================================================
async fn upload_document(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    mut multipart: Multipart,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    require_permission(&claims, "documents:write")?;
    state.rate_limiter.check(&format!("upload:{}", claims.sub))?;

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

    // Check duplicate SHA-256 fingerprint
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

    // Enqueue durable processing job (PostgreSQL queue + optional Redis signal)
    let job_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO processing_jobs (idempotency_key, job_type, target_id, status, payload)
        VALUES ($1, 'document_pipeline', $2, 'queued', $3)
        RETURNING id
        "#,
    )
    .bind(format!("doc_pipeline:{}", doc_id))
    .bind(doc_id)
    .bind(serde_json::json!({ "document_id": doc_id, "actor_id": claims.sub }))
    .fetch_one(&state.db)
    .await?;

    state.redis.publish_job_signal("abifr:jobs", &job_id.to_string()).await;

    let mut pipeline_error: Option<String> = None;
    if process_sync {
        match run_document_pipeline(&state, doc_id, Some(claims.sub)).await {
            Ok(_) => {
                let _ = sqlx::query("UPDATE processing_jobs SET status = 'completed', attempts = 1, completed_at = NOW(), updated_at = NOW() WHERE id = $1")
                    .bind(job_id)
                    .execute(&state.db)
                    .await;
            }
            Err(e) => {
                pipeline_error = Some(e.to_string());
                let _ = sqlx::query("UPDATE processing_jobs SET status = 'failed', attempts = 1, last_error = $2, updated_at = NOW() WHERE id = $1")
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

async fn list_documents(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "documents:read")?;

    let rows = sqlx::query(
        r#"
        SELECT d.id, d.original_filename, d.sha256_hash, d.file_size_bytes, d.detected_mime_type,
               d.file_extension, d.page_count, d.ai_category, d.ai_classification_confidence,
               d.effective_category, d.processing_status, d.ocr_status, d.extraction_status,
               d.matching_status, d.reconciliation_status, d.failure_reason,
               d.is_duplicate_file, d.duplicate_of_document_id, d.prompt_injection_flagged,
               d.is_synthetic_fixture, d.created_at, v.legal_name AS vendor_name
        FROM documents d
        LEFT JOIN vendors v ON v.id = d.vendor_id
        ORDER BY d.created_at DESC
        LIMIT 200
        "#,
    )
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
                "is_synthetic_fixture": r.get::<bool, _>("is_synthetic_fixture"),
                "vendor_name": r.get::<Option<String>, _>("vendor_name"),
                "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at")
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "documents": items })))
}

async fn get_document_detail(
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
               prompt_injection_flagged, is_synthetic_fixture, created_at
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
            "is_synthetic_fixture": d.get::<bool, _>("is_synthetic_fixture"),
            "created_at": d.get::<chrono::DateTime<chrono::Utc>, _>("created_at")
        },
        "extraction": extraction_json,
        "extracted_fields": fields
    })))
}

async fn reprocess_document(
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
struct ReclassifyRequest {
    category: String,
    reason: String,
}

async fn reclassify_document(
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

    // Preserves ai_category immutably while updating effective_category + recording review_action
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

async fn render_document_page(
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
struct CorrectFieldRequest {
    corrected_value: String,
    reason: String,
}

async fn correct_extracted_field(
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

    // NEVER overwrite `extracted_value` — store human correction in `human_override_value`
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

// =============================================================================
// Vendors
// =============================================================================
async fn list_vendors(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "documents:read")?;
    let rows = sqlx::query(
        r#"
        SELECT v.id, v.vendor_code, v.legal_name, v.tax_id, v.default_currency,
               v.country_code, v.payment_terms, v.status, v.created_at,
               (SELECT COUNT(*) FROM contracts c WHERE c.vendor_id = v.id) AS contract_count,
               (SELECT COUNT(*) FROM invoices i WHERE i.vendor_id = v.id) AS invoice_count,
               (SELECT COUNT(*) FROM anomaly_findings f WHERE f.vendor_id = v.id AND f.reviewer_status = 'pending') AS open_findings
        FROM vendors v
        ORDER BY v.legal_name ASC
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    let vendors: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<Uuid, _>("id"),
                "vendor_code": r.get::<String, _>("vendor_code"),
                "legal_name": r.get::<String, _>("legal_name"),
                "tax_id": r.get::<Option<String>, _>("tax_id"),
                "default_currency": r.get::<Option<String>, _>("default_currency"),
                "country_code": r.get::<Option<String>, _>("country_code"),
                "payment_terms": r.get::<Option<String>, _>("payment_terms"),
                "status": r.get::<String, _>("status"),
                "contract_count": r.get::<i64, _>("contract_count"),
                "invoice_count": r.get::<i64, _>("invoice_count"),
                "open_findings": r.get::<i64, _>("open_findings")
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "vendors": vendors })))
}

#[derive(Deserialize)]
struct CreateVendorRequest {
    vendor_code: String,
    legal_name: String,
    tax_id: Option<String>,
    default_currency: Option<String>,
    country_code: Option<String>,
    payment_terms: Option<String>,
}

async fn create_vendor(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Json(req): Json<CreateVendorRequest>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    require_permission(&claims, "vendors:write")?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO vendors (vendor_code, legal_name, tax_id, default_currency, country_code, payment_terms) VALUES ($1,$2,$3,$4,$5,$6) RETURNING id",
    )
    .bind(req.vendor_code.trim())
    .bind(req.legal_name.trim())
    .bind(req.tax_id)
    .bind(req.default_currency)
    .bind(req.country_code)
    .bind(req.payment_terms)
    .fetch_one(&state.db)
    .await?;

    state
        .log_audit(Some(claims.sub), Some(&claims.role), "vendor.created", "vendor", Some(&id.to_string()), serde_json::json!({ "vendor_code": req.vendor_code }))
        .await;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}

// =============================================================================
// Contracts & Amendments
// =============================================================================
async fn list_contracts(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "documents:read")?;
    let rows = sqlx::query(
        r#"
        SELECT c.id, c.contract_number, c.title, c.currency, c.status, c.vendor_id, v.legal_name AS vendor_name,
               (SELECT COUNT(*) FROM contract_versions cv WHERE cv.contract_id = c.id) AS version_count,
               (SELECT MAX(cv.expiration_date) FROM contract_versions cv WHERE cv.contract_id = c.id) AS latest_expiration
        FROM contracts c
        JOIN vendors v ON v.id = c.vendor_id
        ORDER BY c.created_at DESC
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    let contracts: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<Uuid, _>("id"),
                "contract_number": r.get::<String, _>("contract_number"),
                "title": r.get::<String, _>("title"),
                "currency": r.get::<String, _>("currency"),
                "status": r.get::<String, _>("status"),
                "vendor_id": r.get::<Uuid, _>("vendor_id"),
                "vendor_name": r.get::<String, _>("vendor_name"),
                "version_count": r.get::<i64, _>("version_count"),
                "latest_expiration": r.get::<Option<NaiveDate>, _>("latest_expiration")
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "contracts": contracts })))
}

#[derive(Deserialize)]
struct ContractLineItemReq {
    item_code: Option<String>,
    description: String,
    unit: String,
    unit_price: Decimal,
    discount_pct: Option<Decimal>,
    tax_rate: Option<Decimal>,
}

#[derive(Deserialize)]
struct CreateContractRequest {
    contract_number: String,
    vendor_id: Uuid,
    title: String,
    currency: String,
    effective_date: NaiveDate,
    expiration_date: NaiveDate,
    payment_terms: Option<String>,
    price_tolerance_pct: Option<Decimal>,
    price_tolerance_abs: Option<Decimal>,
    default_tax_rate: Option<Decimal>,
    allowed_additional_charges: Option<Vec<String>>,
    line_items: Vec<ContractLineItemReq>,
}

async fn create_contract(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Json(req): Json<CreateContractRequest>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    require_permission(&claims, "contracts:write")?;
    let mut tx = state.db.begin().await?;

    let contract_id: Uuid = sqlx::query_scalar(
        "INSERT INTO contracts (contract_number, vendor_id, title, currency) VALUES ($1,$2,$3,$4) RETURNING id",
    )
    .bind(req.contract_number.trim())
    .bind(req.vendor_id)
    .bind(req.title.trim())
    .bind(req.currency.trim().to_ascii_uppercase())
    .fetch_one(&mut *tx)
    .await?;

    let version_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO contract_versions (
            contract_id, version_label, version_number, is_amendment,
            effective_date, expiration_date, payment_terms,
            price_tolerance_pct, price_tolerance_abs, default_tax_rate, allowed_additional_charges
        )
        VALUES ($1,'v1.0',1,FALSE,$2,$3,$4,$5,$6,$7,$8)
        RETURNING id
        "#,
    )
    .bind(contract_id)
    .bind(req.effective_date)
    .bind(req.expiration_date)
    .bind(&req.payment_terms)
    .bind(req.price_tolerance_pct)
    .bind(req.price_tolerance_abs)
    .bind(req.default_tax_rate)
    .bind(serde_json::to_value(req.allowed_additional_charges.unwrap_or_default()).unwrap())
    .fetch_one(&mut *tx)
    .await?;

    for item in &req.line_items {
        sqlx::query(
            "INSERT INTO contract_line_items (contract_version_id, item_code, description, unit, unit_price, currency, discount_pct, tax_rate) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(version_id)
        .bind(&item.item_code)
        .bind(&item.description)
        .bind(&item.unit)
        .bind(item.unit_price)
        .bind(req.currency.trim().to_ascii_uppercase())
        .bind(item.discount_pct.unwrap_or(Decimal::ZERO))
        .bind(item.tax_rate)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": contract_id, "version_id": version_id }))))
}

async fn get_contract_detail(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "documents:read")?;
    let cid = parse_uuid(&id, "contract_id")?;

    let c = sqlx::query(
        "SELECT c.id, c.contract_number, c.title, c.currency, c.status, c.vendor_id, v.legal_name AS vendor_name FROM contracts c JOIN vendors v ON v.id = c.vendor_id WHERE c.id = $1",
    )
    .bind(cid)
    .fetch_one(&state.db)
    .await?;

    let v_rows = sqlx::query(
        r#"
        SELECT id, version_label, version_number, is_amendment, effective_date, expiration_date,
               payment_terms, price_tolerance_pct, price_tolerance_abs, quantity_tolerance_pct,
               default_tax_rate, tax_provision_notes, allowed_additional_charges, document_id
        FROM contract_versions WHERE contract_id = $1 ORDER BY version_number ASC
        "#,
    )
    .bind(cid)
    .fetch_all(&state.db)
    .await?;

    let mut versions = Vec::new();
    for vr in v_rows {
        let vid: Uuid = vr.get("id");
        let item_rows = sqlx::query(
            "SELECT id, item_code, description, unit, unit_price, currency, discount_pct, tax_rate FROM contract_line_items WHERE contract_version_id = $1",
        )
        .bind(vid)
        .fetch_all(&state.db)
        .await?;

        let items: Vec<serde_json::Value> = item_rows
            .into_iter()
            .map(|ir| {
                serde_json::json!({
                    "id": ir.get::<Uuid, _>("id"),
                    "item_code": ir.get::<Option<String>, _>("item_code"),
                    "description": ir.get::<String, _>("description"),
                    "unit": ir.get::<String, _>("unit"),
                    "unit_price": ir.get::<Decimal, _>("unit_price"),
                    "currency": ir.get::<String, _>("currency"),
                    "discount_pct": ir.get::<Option<Decimal>, _>("discount_pct"),
                    "tax_rate": ir.get::<Option<Decimal>, _>("tax_rate")
                })
            })
            .collect();

        versions.push(serde_json::json!({
            "id": vid,
            "version_label": vr.get::<String, _>("version_label"),
            "version_number": vr.get::<i32, _>("version_number"),
            "is_amendment": vr.get::<bool, _>("is_amendment"),
            "effective_date": vr.get::<NaiveDate, _>("effective_date"),
            "expiration_date": vr.get::<NaiveDate, _>("expiration_date"),
            "payment_terms": vr.get::<Option<String>, _>("payment_terms"),
            "price_tolerance_pct": vr.get::<Option<Decimal>, _>("price_tolerance_pct"),
            "price_tolerance_abs": vr.get::<Option<Decimal>, _>("price_tolerance_abs"),
            "default_tax_rate": vr.get::<Option<Decimal>, _>("default_tax_rate"),
            "tax_provision_notes": vr.get::<Option<String>, _>("tax_provision_notes"),
            "allowed_additional_charges": vr.get::<serde_json::Value, _>("allowed_additional_charges"),
            "document_id": vr.get::<Option<Uuid>, _>("document_id"),
            "line_items": items
        }));
    }

    Ok(Json(serde_json::json!({
        "contract": {
            "id": c.get::<Uuid, _>("id"),
            "contract_number": c.get::<String, _>("contract_number"),
            "title": c.get::<String, _>("title"),
            "currency": c.get::<String, _>("currency"),
            "status": c.get::<String, _>("status"),
            "vendor_id": c.get::<Uuid, _>("vendor_id"),
            "vendor_name": c.get::<String, _>("vendor_name")
        },
        "versions": versions
    })))
}

#[derive(Deserialize)]
struct AddContractVersionRequest {
    version_label: String,
    effective_date: NaiveDate,
    expiration_date: NaiveDate,
    price_tolerance_pct: Option<Decimal>,
    default_tax_rate: Option<Decimal>,
    line_items: Vec<ContractLineItemReq>,
}

async fn add_contract_version(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<String>,
    Json(req): Json<AddContractVersionRequest>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    require_permission(&claims, "contracts:write")?;
    let cid = parse_uuid(&id, "contract_id")?;

    let currency: String = sqlx::query_scalar("SELECT currency FROM contracts WHERE id = $1")
        .bind(cid)
        .fetch_one(&state.db)
        .await?;
    let next_ver: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(version_number), 0) + 1 FROM contract_versions WHERE contract_id = $1",
    )
    .bind(cid)
    .fetch_one(&state.db)
    .await?;

    let mut tx = state.db.begin().await?;
    let vid: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO contract_versions (contract_id, version_label, version_number, is_amendment, effective_date, expiration_date, price_tolerance_pct, default_tax_rate)
        VALUES ($1,$2,$3,TRUE,$4,$5,$6,$7) RETURNING id
        "#,
    )
    .bind(cid)
    .bind(&req.version_label)
    .bind(next_ver)
    .bind(req.effective_date)
    .bind(req.expiration_date)
    .bind(req.price_tolerance_pct)
    .bind(req.default_tax_rate)
    .fetch_one(&mut *tx)
    .await?;

    for item in &req.line_items {
        sqlx::query(
            "INSERT INTO contract_line_items (contract_version_id, item_code, description, unit, unit_price, currency, discount_pct, tax_rate) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(vid)
        .bind(&item.item_code)
        .bind(&item.description)
        .bind(&item.unit)
        .bind(item.unit_price)
        .bind(&currency)
        .bind(item.discount_pct.unwrap_or(Decimal::ZERO))
        .bind(item.tax_rate)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "version_id": vid, "version_number": next_ver }))))
}

// =============================================================================
// Purchase Orders
// =============================================================================
async fn list_purchase_orders(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "documents:read")?;
    let rows = sqlx::query(
        r#"
        SELECT po.id, po.po_number, po.issue_date, po.currency, po.total_amount, po.status,
               v.legal_name AS vendor_name, c.contract_number,
               (SELECT COUNT(*) FROM purchase_order_line_items li WHERE li.purchase_order_id = po.id) AS line_count
        FROM purchase_orders po
        JOIN vendors v ON v.id = po.vendor_id
        LEFT JOIN contracts c ON c.id = po.contract_id
        ORDER BY po.issue_date DESC
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    let pos: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<Uuid, _>("id"),
                "po_number": r.get::<String, _>("po_number"),
                "issue_date": r.get::<NaiveDate, _>("issue_date"),
                "currency": r.get::<String, _>("currency"),
                "total_amount": r.get::<Decimal, _>("total_amount"),
                "status": r.get::<String, _>("status"),
                "vendor_name": r.get::<String, _>("vendor_name"),
                "contract_number": r.get::<Option<String>, _>("contract_number"),
                "line_count": r.get::<i64, _>("line_count")
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "purchase_orders": pos })))
}

#[derive(Deserialize)]
struct PoLineReq {
    item_code: Option<String>,
    description: String,
    unit: String,
    ordered_quantity: Decimal,
    received_quantity: Option<Decimal>,
    unit_price: Decimal,
}

#[derive(Deserialize)]
struct CreatePoRequest {
    po_number: String,
    vendor_id: Uuid,
    contract_id: Option<Uuid>,
    issue_date: NaiveDate,
    currency: String,
    line_items: Vec<PoLineReq>,
}

async fn create_purchase_order(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Json(req): Json<CreatePoRequest>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    require_permission(&claims, "documents:write")?;
    let total: Decimal = req
        .line_items
        .iter()
        .map(|l| l.ordered_quantity * l.unit_price)
        .sum();
    let mut tx = state.db.begin().await?;
    let po_id: Uuid = sqlx::query_scalar(
        "INSERT INTO purchase_orders (po_number, vendor_id, contract_id, issue_date, currency, total_amount) VALUES ($1,$2,$3,$4,$5,$6) RETURNING id",
    )
    .bind(req.po_number.trim())
    .bind(req.vendor_id)
    .bind(req.contract_id)
    .bind(req.issue_date)
    .bind(req.currency.trim().to_ascii_uppercase())
    .bind(total)
    .fetch_one(&mut *tx)
    .await?;

    for l in &req.line_items {
        sqlx::query(
            "INSERT INTO purchase_order_line_items (purchase_order_id, item_code, description, unit, ordered_quantity, received_quantity, unit_price, line_total) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(po_id)
        .bind(&l.item_code)
        .bind(&l.description)
        .bind(&l.unit)
        .bind(l.ordered_quantity)
        .bind(l.received_quantity)
        .bind(l.unit_price)
        .bind(l.ordered_quantity * l.unit_price)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": po_id }))))
}

// =============================================================================
// Invoices & Reconciliation
// =============================================================================
async fn list_invoices(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "documents:read")?;
    let rows = sqlx::query(
        r#"
        SELECT i.id, i.invoice_number, i.invoice_date, i.currency, i.referenced_contract_number,
               i.referenced_po_number, i.stated_subtotal, i.stated_tax_total, i.stated_total,
               i.recalculated_total, i.match_confidence_status, i.reconciliation_status,
               i.document_id, v.legal_name AS vendor_name,
               (SELECT COUNT(*) FROM anomaly_findings f WHERE f.invoice_id = i.id) AS findings_count
        FROM invoices i
        JOIN vendors v ON v.id = i.vendor_id
        ORDER BY i.created_at DESC
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    let invoices: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<Uuid, _>("id"),
                "invoice_number": r.get::<String, _>("invoice_number"),
                "invoice_date": r.get::<NaiveDate, _>("invoice_date"),
                "currency": r.get::<String, _>("currency"),
                "referenced_contract_number": r.get::<Option<String>, _>("referenced_contract_number"),
                "referenced_po_number": r.get::<Option<String>, _>("referenced_po_number"),
                "stated_subtotal": r.get::<Decimal, _>("stated_subtotal"),
                "stated_tax_total": r.get::<Decimal, _>("stated_tax_total"),
                "stated_total": r.get::<Decimal, _>("stated_total"),
                "recalculated_total": r.get::<Option<Decimal>, _>("recalculated_total"),
                "match_confidence_status": r.get::<String, _>("match_confidence_status"),
                "reconciliation_status": r.get::<String, _>("reconciliation_status"),
                "document_id": r.get::<Option<Uuid>, _>("document_id"),
                "vendor_name": r.get::<String, _>("vendor_name"),
                "findings_count": r.get::<i64, _>("findings_count")
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "invoices": invoices })))
}

#[derive(Deserialize)]
struct InvoiceLineReq {
    item_code: Option<String>,
    description: String,
    quantity: Decimal,
    unit: String,
    unit_price: Decimal,
    discount_pct: Option<Decimal>,
    discount_amount: Option<Decimal>,
    tax_rate: Option<Decimal>,
    tax_amount: Option<Decimal>,
    stated_line_subtotal: Decimal,
    stated_line_total: Decimal,
    is_additional_charge: Option<bool>,
}

#[derive(Deserialize)]
struct CreateInvoiceRequest {
    document_id: Option<Uuid>,
    invoice_number: String,
    vendor_id: Uuid,
    customer_name: Option<String>,
    invoice_date: NaiveDate,
    currency: String,
    referenced_contract_number: Option<String>,
    referenced_po_number: Option<String>,
    stated_subtotal: Decimal,
    stated_discount_total: Option<Decimal>,
    stated_tax_total: Option<Decimal>,
    stated_additional_charges: Option<Decimal>,
    stated_total: Decimal,
    line_items: Vec<InvoiceLineReq>,
    auto_reconcile: Option<bool>,
}

async fn create_invoice(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Json(req): Json<CreateInvoiceRequest>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    require_permission(&claims, "documents:write")?;
    let mut tx = state.db.begin().await?;

    let inv_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO invoices (
            document_id, invoice_number, vendor_id, customer_name, invoice_date, currency,
            referenced_contract_number, referenced_po_number, stated_subtotal,
            stated_discount_total, stated_tax_total, stated_additional_charges, stated_total
        )
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
        RETURNING id
        "#,
    )
    .bind(req.document_id)
    .bind(req.invoice_number.trim())
    .bind(req.vendor_id)
    .bind(&req.customer_name)
    .bind(req.invoice_date)
    .bind(req.currency.trim().to_ascii_uppercase())
    .bind(&req.referenced_contract_number)
    .bind(&req.referenced_po_number)
    .bind(req.stated_subtotal)
    .bind(req.stated_discount_total.unwrap_or(Decimal::ZERO))
    .bind(req.stated_tax_total.unwrap_or(Decimal::ZERO))
    .bind(req.stated_additional_charges.unwrap_or(Decimal::ZERO))
    .bind(req.stated_total)
    .fetch_one(&mut *tx)
    .await?;

    for (idx, l) in req.line_items.iter().enumerate() {
        sqlx::query(
            r#"
            INSERT INTO invoice_line_items (
                invoice_id, line_number, item_code, description, quantity, unit, unit_price,
                discount_pct, discount_amount, tax_rate, tax_amount,
                stated_line_subtotal, stated_line_total, is_additional_charge
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
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
        .bind(l.stated_line_subtotal)
        .bind(l.stated_line_total)
        .bind(l.is_additional_charge.unwrap_or(false))
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    if req.auto_reconcile.unwrap_or(true) {
        let (job_id, out) = run_invoice_reconciliation_workflow(&state, inv_id, Some(claims.sub), None).await?;
        return Ok((
            StatusCode::CREATED,
            Json(serde_json::json!({
                "id": inv_id,
                "reconciliation_job_id": job_id,
                "reconciliation": out
            })),
        ));
    }

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": inv_id }))))
}

async fn get_invoice_detail(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "documents:read")?;
    let inv_id = parse_uuid(&id, "invoice_id")?;

    let inv = sqlx::query(
        r#"
        SELECT i.*, v.legal_name AS vendor_name, c.contract_number AS matched_contract_number,
               cv.version_label AS matched_contract_version_label
        FROM invoices i
        JOIN vendors v ON v.id = i.vendor_id
        LEFT JOIN contracts c ON c.id = i.matched_contract_id
        LEFT JOIN contract_versions cv ON cv.id = i.matched_contract_version_id
        WHERE i.id = $1
        "#,
    )
    .bind(inv_id)
    .fetch_one(&state.db)
    .await?;

    let line_rows = sqlx::query(
        r#"
        SELECT li.*, ci.unit_price AS contract_unit_price, ci.description AS contract_item_description
        FROM invoice_line_items li
        LEFT JOIN contract_line_items ci ON ci.id = li.matched_contract_item_id
        WHERE li.invoice_id = $1 ORDER BY li.line_number ASC
        "#,
    )
    .bind(inv_id)
    .fetch_all(&state.db)
    .await?;

    let lines: Vec<serde_json::Value> = line_rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<Uuid, _>("id"),
                "line_number": r.get::<i32, _>("line_number"),
                "item_code": r.get::<Option<String>, _>("item_code"),
                "description": r.get::<String, _>("description"),
                "quantity": r.get::<Decimal, _>("quantity"),
                "unit": r.get::<String, _>("unit"),
                "unit_price": r.get::<Decimal, _>("unit_price"),
                "discount_pct": r.get::<Decimal, _>("discount_pct"),
                "discount_amount": r.get::<Decimal, _>("discount_amount"),
                "tax_rate": r.get::<Option<Decimal>, _>("tax_rate"),
                "tax_amount": r.get::<Decimal, _>("tax_amount"),
                "stated_line_subtotal": r.get::<Decimal, _>("stated_line_subtotal"),
                "stated_line_total": r.get::<Decimal, _>("stated_line_total"),
                "is_additional_charge": r.get::<bool, _>("is_additional_charge"),
                "match_method": r.get::<String, _>("match_method"),
                "match_requires_review": r.get::<bool, _>("match_requires_review"),
                "contract_unit_price": r.get::<Option<Decimal>, _>("contract_unit_price"),
                "contract_item_description": r.get::<Option<String>, _>("contract_item_description")
            })
        })
        .collect();

    let stage_rows = sqlx::query(
        r#"
        SELECT stage_code, stage_name, status, expected_value, actual_value, variance_amount,
               variance_pct, permitted_tolerance, amount_outside_tolerance, currency,
               rule_applied, calculation_details
        FROM reconciliation_results
        WHERE reconciliation_job_id = (
            SELECT id FROM reconciliation_jobs WHERE invoice_id = $1 ORDER BY started_at DESC LIMIT 1
        )
        ORDER BY stage_code ASC, created_at ASC
        "#,
    )
    .bind(inv_id)
    .fetch_all(&state.db)
    .await?;

    let stages: Vec<serde_json::Value> = stage_rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "stage_code": r.get::<String, _>("stage_code"),
                "stage_name": r.get::<String, _>("stage_name"),
                "status": r.get::<String, _>("status"),
                "expected_value": r.get::<Option<Decimal>, _>("expected_value"),
                "actual_value": r.get::<Option<Decimal>, _>("actual_value"),
                "variance_amount": r.get::<Option<Decimal>, _>("variance_amount"),
                "variance_pct": r.get::<Option<Decimal>, _>("variance_pct"),
                "permitted_tolerance": r.get::<Option<String>, _>("permitted_tolerance"),
                "amount_outside_tolerance": r.get::<Option<Decimal>, _>("amount_outside_tolerance"),
                "currency": r.get::<Option<String>, _>("currency"),
                "rule_applied": r.get::<String, _>("rule_applied"),
                "calculation_details": r.get::<serde_json::Value, _>("calculation_details")
            })
        })
        .collect();

    let finding_rows = sqlx::query(
        "SELECT id, category, classification_type, severity, status, reviewer_status, expected_value, actual_value, variance_amount, currency, rule_applied, explanation FROM anomaly_findings WHERE invoice_id = $1 ORDER BY detected_at DESC",
    )
    .bind(inv_id)
    .fetch_all(&state.db)
    .await?;

    let findings: Vec<serde_json::Value> = finding_rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<Uuid, _>("id"),
                "category": r.get::<String, _>("category"),
                "classification_type": r.get::<String, _>("classification_type"),
                "severity": r.get::<String, _>("severity"),
                "status": r.get::<String, _>("status"),
                "reviewer_status": r.get::<String, _>("reviewer_status"),
                "expected_value": r.get::<Option<Decimal>, _>("expected_value"),
                "actual_value": r.get::<Option<Decimal>, _>("actual_value"),
                "variance_amount": r.get::<Option<Decimal>, _>("variance_amount"),
                "currency": r.get::<Option<String>, _>("currency"),
                "rule_applied": r.get::<String, _>("rule_applied"),
                "explanation": r.get::<String, _>("explanation")
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "invoice": {
            "id": inv.get::<Uuid, _>("id"),
            "document_id": inv.get::<Option<Uuid>, _>("document_id"),
            "invoice_number": inv.get::<String, _>("invoice_number"),
            "vendor_name": inv.get::<String, _>("vendor_name"),
            "invoice_date": inv.get::<NaiveDate, _>("invoice_date"),
            "currency": inv.get::<String, _>("currency"),
            "referenced_contract_number": inv.get::<Option<String>, _>("referenced_contract_number"),
            "matched_contract_number": inv.get::<Option<String>, _>("matched_contract_number"),
            "matched_contract_version_label": inv.get::<Option<String>, _>("matched_contract_version_label"),
            "referenced_po_number": inv.get::<Option<String>, _>("referenced_po_number"),
            "stated_subtotal": inv.get::<Decimal, _>("stated_subtotal"),
            "stated_tax_total": inv.get::<Decimal, _>("stated_tax_total"),
            "stated_additional_charges": inv.get::<Decimal, _>("stated_additional_charges"),
            "stated_total": inv.get::<Decimal, _>("stated_total"),
            "recalculated_subtotal": inv.get::<Option<Decimal>, _>("recalculated_subtotal"),
            "recalculated_tax_total": inv.get::<Option<Decimal>, _>("recalculated_tax_total"),
            "recalculated_total": inv.get::<Option<Decimal>, _>("recalculated_total"),
            "match_confidence_status": inv.get::<String, _>("match_confidence_status"),
            "reconciliation_status": inv.get::<String, _>("reconciliation_status")
        },
        "line_items": lines,
        "stages": stages,
        "findings": findings
    })))
}

async fn reconcile_invoice_handler(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "reconciliation:run")?;
    let inv_id = parse_uuid(&id, "invoice_id")?;
    let (job_id, out) = run_invoice_reconciliation_workflow(&state, inv_id, Some(claims.sub), None).await?;
    Ok(Json(serde_json::json!({
        "reconciliation_job_id": job_id,
        "result": out
    })))
}

#[derive(Deserialize)]
struct ConfirmMatchRequest {
    invoice_line_item_id: Uuid,
    contract_line_item_id: Uuid,
    notes: String,
}

async fn confirm_invoice_match(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<String>,
    Json(req): Json<ConfirmMatchRequest>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "findings:review")?;
    let inv_id = parse_uuid(&id, "invoice_id")?;

    sqlx::query(
        "UPDATE invoice_line_items SET matched_contract_item_id = $2, match_method = 'human_confirmed', match_requires_review = FALSE WHERE id = $1 AND invoice_id = $3",
    )
    .bind(req.invoice_line_item_id)
    .bind(req.contract_line_item_id)
    .bind(inv_id)
    .execute(&state.db)
    .await?;

    sqlx::query(
        "INSERT INTO review_actions (target_type, target_id, action_type, reviewer_id, new_state, notes) VALUES ('invoice_match', $1, 'confirm_relationship', $2, $3, $4)",
    )
    .bind(req.invoice_line_item_id)
    .bind(claims.sub)
    .bind(serde_json::json!({ "contract_line_item_id": req.contract_line_item_id }))
    .bind(&req.notes)
    .execute(&state.db)
    .await?;

    let (job_id, out) = run_invoice_reconciliation_workflow(&state, inv_id, Some(claims.sub), None).await?;
    Ok(Json(serde_json::json!({ "status": "confirmed_and_reconciled", "reconciliation_job_id": job_id, "result": out })))
}

// =============================================================================
// Payments & Allocations
// =============================================================================
async fn list_payments(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "documents:read")?;
    let rows = sqlx::query(
        r#"
        SELECT p.id, p.payment_reference, p.bank_reference, p.beneficiary_name, p.payment_date,
               p.amount, p.currency, p.referenced_invoice_number, p.transaction_status,
               p.is_refund_or_credit, p.allocation_status, v.legal_name AS vendor_name,
               COALESCE((SELECT SUM(pa.allocated_amount) FROM payment_allocations pa WHERE pa.payment_id = p.id), 0) AS allocated_total
        FROM payments p
        JOIN vendors v ON v.id = p.vendor_id
        ORDER BY p.payment_date DESC
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    let payments: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<Uuid, _>("id"),
                "payment_reference": r.get::<String, _>("payment_reference"),
                "bank_reference": r.get::<Option<String>, _>("bank_reference"),
                "beneficiary_name": r.get::<String, _>("beneficiary_name"),
                "payment_date": r.get::<NaiveDate, _>("payment_date"),
                "amount": r.get::<Decimal, _>("amount"),
                "currency": r.get::<String, _>("currency"),
                "referenced_invoice_number": r.get::<Option<String>, _>("referenced_invoice_number"),
                "transaction_status": r.get::<String, _>("transaction_status"),
                "is_refund_or_credit": r.get::<bool, _>("is_refund_or_credit"),
                "allocation_status": r.get::<String, _>("allocation_status"),
                "vendor_name": r.get::<String, _>("vendor_name"),
                "allocated_total": r.get::<Decimal, _>("allocated_total")
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "payments": payments })))
}

#[derive(Deserialize)]
struct CreatePaymentRequest {
    payment_reference: String,
    bank_reference: Option<String>,
    vendor_id: Uuid,
    beneficiary_name: String,
    payment_date: NaiveDate,
    amount: Decimal,
    currency: String,
    referenced_invoice_number: Option<String>,
    is_refund_or_credit: Option<bool>,
}

async fn create_payment(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Json(req): Json<CreatePaymentRequest>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    require_permission(&claims, "payments:write")?;
    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO payments (payment_reference, bank_reference, vendor_id, beneficiary_name, payment_date, amount, currency, referenced_invoice_number, is_refund_or_credit)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING id
        "#,
    )
    .bind(req.payment_reference.trim())
    .bind(&req.bank_reference)
    .bind(req.vendor_id)
    .bind(req.beneficiary_name.trim())
    .bind(req.payment_date)
    .bind(req.amount)
    .bind(req.currency.trim().to_ascii_uppercase())
    .bind(&req.referenced_invoice_number)
    .bind(req.is_refund_or_credit.unwrap_or(false))
    .fetch_one(&state.db)
    .await?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}

#[derive(Deserialize)]
struct AllocatePaymentRequest {
    invoice_id: Uuid,
    allocated_amount: Decimal,
    matching_evidence: String,
}

async fn allocate_payment(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<String>,
    Json(req): Json<AllocatePaymentRequest>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "findings:review")?;
    let pay_id = parse_uuid(&id, "payment_id")?;

    let pay = sqlx::query("SELECT currency, amount FROM payments WHERE id = $1")
        .bind(pay_id)
        .fetch_one(&state.db)
        .await?;
    let p_curr: String = pay.get("currency");
    let p_amt: Decimal = pay.get("amount");

    let inv_curr: String = sqlx::query_scalar("SELECT currency FROM invoices WHERE id = $1")
        .bind(req.invoice_id)
        .fetch_one(&state.db)
        .await?;

    if !p_curr.eq_ignore_ascii_case(&inv_curr) {
        return Err(AppError::BadRequest(format!(
            "Cannot allocate {} payment to {} invoice without verified FX conversion.",
            p_curr, inv_curr
        )));
    }

    sqlx::query(
        r#"
        INSERT INTO payment_allocations (payment_id, invoice_id, allocated_amount, currency, matching_evidence, matched_by, confirmed_by)
        VALUES ($1,$2,$3,$4,$5,'human_confirmed',$6)
        ON CONFLICT (payment_id, invoice_id) DO UPDATE SET allocated_amount = EXCLUDED.allocated_amount, matching_evidence = EXCLUDED.matching_evidence
        "#,
    )
    .bind(pay_id)
    .bind(req.invoice_id)
    .bind(req.allocated_amount)
    .bind(&p_curr)
    .bind(&req.matching_evidence)
    .bind(claims.sub)
    .execute(&state.db)
    .await?;

    let total_alloc: Decimal = sqlx::query_scalar(
        "SELECT COALESCE(SUM(allocated_amount), 0) FROM payment_allocations WHERE payment_id = $1",
    )
    .bind(pay_id)
    .fetch_one(&state.db)
    .await?;

    let new_status = if total_alloc == p_amt {
        "fully_allocated"
    } else if total_alloc > p_amt {
        "over_allocated"
    } else {
        "partially_allocated"
    };

    sqlx::query("UPDATE payments SET allocation_status = $2 WHERE id = $1")
        .bind(pay_id)
        .bind(new_status)
        .execute(&state.db)
        .await?;

    Ok(Json(serde_json::json!({ "status": new_status, "allocated_total": total_alloc })))
}

// =============================================================================
// Findings, Evidence & Human-in-the-Loop Review
// =============================================================================
#[derive(Deserialize)]
struct FindingFilterQuery {
    category: Option<String>,
    status: Option<String>,
    classification_type: Option<String>,
}

async fn list_findings(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(q): Query<FindingFilterQuery>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "findings:read")?;

    let rows = sqlx::query(
        r#"
        SELECT f.id, f.category, f.classification_type, f.severity, f.severity_rule_applied,
               f.status, f.reviewer_status, f.expected_value, f.actual_value, f.variance_amount,
               f.currency, f.rule_applied, f.explanation, f.resolution_notes, f.detected_at,
               f.document_id, f.invoice_id, v.legal_name AS vendor_name,
               i.invoice_number, c.contract_number,
               (SELECT COUNT(*) FROM evidence_references er WHERE er.finding_id = f.id) AS evidence_count
        FROM anomaly_findings f
        LEFT JOIN vendors v ON v.id = f.vendor_id
        LEFT JOIN invoices i ON i.id = f.invoice_id
        LEFT JOIN contracts c ON c.id = f.contract_id
        WHERE ($1::text IS NULL OR f.category = $1)
          AND ($2::text IS NULL OR f.status = $2)
          AND ($3::text IS NULL OR f.classification_type = $3)
        ORDER BY f.detected_at DESC
        LIMIT 250
        "#,
    )
    .bind(&q.category)
    .bind(&q.status)
    .bind(&q.classification_type)
    .fetch_all(&state.db)
    .await?;

    let findings: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<Uuid, _>("id"),
                "category": r.get::<String, _>("category"),
                "classification_type": r.get::<String, _>("classification_type"),
                "severity": r.get::<String, _>("severity"),
                "severity_rule_applied": r.get::<String, _>("severity_rule_applied"),
                "status": r.get::<String, _>("status"),
                "reviewer_status": r.get::<String, _>("reviewer_status"),
                "expected_value": r.get::<Option<Decimal>, _>("expected_value"),
                "actual_value": r.get::<Option<Decimal>, _>("actual_value"),
                "variance_amount": r.get::<Option<Decimal>, _>("variance_amount"),
                "currency": r.get::<Option<String>, _>("currency"),
                "rule_applied": r.get::<String, _>("rule_applied"),
                "explanation": r.get::<String, _>("explanation"),
                "resolution_notes": r.get::<Option<String>, _>("resolution_notes"),
                "detected_at": r.get::<chrono::DateTime<chrono::Utc>, _>("detected_at"),
                "document_id": r.get::<Option<Uuid>, _>("document_id"),
                "invoice_id": r.get::<Option<Uuid>, _>("invoice_id"),
                "vendor_name": r.get::<Option<String>, _>("vendor_name"),
                "invoice_number": r.get::<Option<String>, _>("invoice_number"),
                "contract_number": r.get::<Option<String>, _>("contract_number"),
                "evidence_count": r.get::<i64, _>("evidence_count")
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "findings": findings })))
}

async fn get_finding_detail(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "findings:read")?;
    let fid = parse_uuid(&id, "finding_id")?;

    let f = sqlx::query(
        r#"
        SELECT f.*, v.legal_name AS vendor_name, i.invoice_number, c.contract_number
        FROM anomaly_findings f
        LEFT JOIN vendors v ON v.id = f.vendor_id
        LEFT JOIN invoices i ON i.id = f.invoice_id
        LEFT JOIN contracts c ON c.id = f.contract_id
        WHERE f.id = $1
        "#,
    )
    .bind(fid)
    .fetch_one(&state.db)
    .await?;

    let ev_rows = sqlx::query(
        "SELECT id, document_id, extracted_field_id, evidence_role, page_number, bounding_box, quoted_text, source_location FROM evidence_references WHERE finding_id = $1",
    )
    .bind(fid)
    .fetch_all(&state.db)
    .await?;

    let evidence: Vec<serde_json::Value> = ev_rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<Uuid, _>("id"),
                "document_id": r.get::<Option<Uuid>, _>("document_id"),
                "extracted_field_id": r.get::<Option<Uuid>, _>("extracted_field_id"),
                "evidence_role": r.get::<String, _>("evidence_role"),
                "page_number": r.get::<Option<i32>, _>("page_number"),
                "bounding_box": r.get::<Option<serde_json::Value>, _>("bounding_box"),
                "quoted_text": r.get::<Option<String>, _>("quoted_text"),
                "source_location": r.get::<Option<String>, _>("source_location")
            })
        })
        .collect();

    let rev_rows = sqlx::query(
        r#"
        SELECT ra.id, ra.action_type, ra.notes, ra.previous_state, ra.new_state, ra.created_at, u.full_name AS reviewer_name, u.role_name
        FROM review_actions ra
        JOIN users u ON u.id = ra.reviewer_id
        WHERE ra.target_type = 'finding' AND ra.target_id = $1
        ORDER BY ra.created_at DESC
        "#,
    )
    .bind(fid)
    .fetch_all(&state.db)
    .await?;

    let history: Vec<serde_json::Value> = rev_rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<Uuid, _>("id"),
                "action_type": r.get::<String, _>("action_type"),
                "notes": r.get::<String, _>("notes"),
                "previous_state": r.get::<serde_json::Value, _>("previous_state"),
                "new_state": r.get::<serde_json::Value, _>("new_state"),
                "reviewer_name": r.get::<String, _>("reviewer_name"),
                "reviewer_role": r.get::<String, _>("role_name"),
                "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at")
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "finding": {
            "id": f.get::<Uuid, _>("id"),
            "category": f.get::<String, _>("category"),
            "classification_type": f.get::<String, _>("classification_type"),
            "severity": f.get::<String, _>("severity"),
            "severity_rule_applied": f.get::<String, _>("severity_rule_applied"),
            "status": f.get::<String, _>("status"),
            "reviewer_status": f.get::<String, _>("reviewer_status"),
            "vendor_name": f.get::<Option<String>, _>("vendor_name"),
            "invoice_number": f.get::<Option<String>, _>("invoice_number"),
            "contract_number": f.get::<Option<String>, _>("contract_number"),
            "expected_value": f.get::<Option<Decimal>, _>("expected_value"),
            "actual_value": f.get::<Option<Decimal>, _>("actual_value"),
            "variance_amount": f.get::<Option<Decimal>, _>("variance_amount"),
            "currency": f.get::<Option<String>, _>("currency"),
            "rule_applied": f.get::<String, _>("rule_applied"),
            "explanation": f.get::<String, _>("explanation"),
            "calculation_details": f.get::<serde_json::Value, _>("calculation_details"),
            "extraction_provenance": f.get::<serde_json::Value, _>("extraction_provenance"),
            "resolution_notes": f.get::<Option<String>, _>("resolution_notes"),
            "detected_at": f.get::<chrono::DateTime<chrono::Utc>, _>("detected_at")
        },
        "evidence": evidence,
        "review_history": history
    })))
}

#[derive(Deserialize)]
struct ReviewFindingRequest {
    action: String, // approve|reject|resolve|escalate|add_explanation
    notes: String,
}

async fn review_finding(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<String>,
    Json(req): Json<ReviewFindingRequest>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "findings:review")?;
    let fid = parse_uuid(&id, "finding_id")?;

    let (new_status, new_rev_status) = match req.action.as_str() {
        "approve" => ("approved", "approved"),
        "reject" => ("rejected", "rejected"),
        "resolve" => ("resolved", "resolved"),
        "escalate" => ("escalated", "escalated"),
        "add_explanation" => ("requires_review", "pending"),
        _ => return Err(AppError::BadRequest("Invalid review action.".into())),
    };

    let prev = sqlx::query("SELECT status, reviewer_status, resolution_notes FROM anomaly_findings WHERE id = $1")
        .bind(fid)
        .fetch_one(&state.db)
        .await?;

    sqlx::query(
        r#"
        UPDATE anomaly_findings
        SET status = $2,
            reviewer_status = $3,
            resolution_notes = $4,
            resolved_by = $5,
            resolved_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(fid)
    .bind(new_status)
    .bind(new_rev_status)
    .bind(&req.notes)
    .bind(claims.sub)
    .execute(&state.db)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO review_actions (target_type, target_id, action_type, reviewer_id, previous_state, new_state, notes)
        VALUES ('finding', $1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(fid)
    .bind(&req.action)
    .bind(claims.sub)
    .bind(serde_json::json!({
        "status": prev.get::<String, _>("status"),
        "reviewer_status": prev.get::<String, _>("reviewer_status")
    }))
    .bind(serde_json::json!({
        "status": new_status,
        "reviewer_status": new_rev_status
    }))
    .bind(&req.notes)
    .execute(&state.db)
    .await?;

    state
        .log_audit(
            Some(claims.sub),
            Some(&claims.role),
            &format!("finding.{}", req.action),
            "finding",
            Some(&fid.to_string()),
            serde_json::json!({ "notes": req.notes, "new_status": new_status }),
        )
        .await;

    Ok(Json(serde_json::json!({ "status": new_status, "reviewer_status": new_rev_status })))
}

async fn get_review_queue(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "findings:read")?;

    let pending_findings = sqlx::query(
        r#"
        SELECT f.id, f.category, f.classification_type, f.severity, f.status,
               f.variance_amount, f.currency, f.explanation, f.detected_at,
               v.legal_name AS vendor_name, i.invoice_number, f.document_id
        FROM anomaly_findings f
        LEFT JOIN vendors v ON v.id = f.vendor_id
        LEFT JOIN invoices i ON i.id = f.invoice_id
        WHERE f.reviewer_status = 'pending' OR f.status IN ('requires_review', 'escalated')
        ORDER BY CASE f.severity WHEN 'critical' THEN 1 WHEN 'high' THEN 2 WHEN 'medium' THEN 3 ELSE 4 END, f.detected_at DESC
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    let docs_needing_review = sqlx::query(
        "SELECT id, original_filename, effective_category, processing_status, prompt_injection_flagged, created_at FROM documents WHERE processing_status = 'requires_review' OR effective_category = 'unknown' ORDER BY created_at DESC",
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(serde_json::json!({
        "pending_findings": pending_findings.into_iter().map(|r| serde_json::json!({
            "id": r.get::<Uuid, _>("id"),
            "category": r.get::<String, _>("category"),
            "classification_type": r.get::<String, _>("classification_type"),
            "severity": r.get::<String, _>("severity"),
            "status": r.get::<String, _>("status"),
            "variance_amount": r.get::<Option<Decimal>, _>("variance_amount"),
            "currency": r.get::<Option<String>, _>("currency"),
            "explanation": r.get::<String, _>("explanation"),
            "vendor_name": r.get::<Option<String>, _>("vendor_name"),
            "invoice_number": r.get::<Option<String>, _>("invoice_number"),
            "document_id": r.get::<Option<Uuid>, _>("document_id"),
            "detected_at": r.get::<chrono::DateTime<chrono::Utc>, _>("detected_at")
        })).collect::<Vec<_>>(),
        "documents_needing_review": docs_needing_review.into_iter().map(|r| serde_json::json!({
            "id": r.get::<Uuid, _>("id"),
            "original_filename": r.get::<String, _>("original_filename"),
            "effective_category": r.get::<String, _>("effective_category"),
            "processing_status": r.get::<String, _>("processing_status"),
            "prompt_injection_flagged": r.get::<bool, _>("prompt_injection_flagged"),
            "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at")
        })).collect::<Vec<_>>()
    })))
}

// =============================================================================
// Agent Runs Activity
// =============================================================================
async fn list_agent_runs(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "documents:read")?;
    let rows = sqlx::query(
        r#"
        SELECT id, workflow_run_id, step_number, agent_name, document_id, invoice_id,
               status, model_or_rule_version, input_payload, output_payload,
               evidence_refs, error_message, duration_ms, started_at, completed_at
        FROM agent_runs
        ORDER BY started_at DESC, step_number ASC
        LIMIT 200
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    let runs: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<Uuid, _>("id"),
                "workflow_run_id": r.get::<Uuid, _>("workflow_run_id"),
                "step_number": r.get::<i32, _>("step_number"),
                "agent_name": r.get::<String, _>("agent_name"),
                "document_id": r.get::<Option<Uuid>, _>("document_id"),
                "invoice_id": r.get::<Option<Uuid>, _>("invoice_id"),
                "status": r.get::<String, _>("status"),
                "model_or_rule_version": r.get::<String, _>("model_or_rule_version"),
                "output_payload": r.get::<serde_json::Value, _>("output_payload"),
                "error_message": r.get::<Option<String>, _>("error_message"),
                "duration_ms": r.get::<Option<i32>, _>("duration_ms"),
                "started_at": r.get::<chrono::DateTime<chrono::Utc>, _>("started_at")
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "agent_runs": runs })))
}

// =============================================================================
// Audit Reports & Real Exports (CSV / XLSX / PDF)
// =============================================================================
async fn list_reports(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "reports:read")?;
    let rows = sqlx::query(
        "SELECT id, report_type, title, date_from, date_to, rule_version, model_version, report_payload, generated_at FROM audit_reports ORDER BY generated_at DESC LIMIT 50",
    )
    .fetch_all(&state.db)
    .await?;

    let reports: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<Uuid, _>("id"),
                "report_type": r.get::<String, _>("report_type"),
                "title": r.get::<String, _>("title"),
                "date_from": r.get::<Option<NaiveDate>, _>("date_from"),
                "date_to": r.get::<Option<NaiveDate>, _>("date_to"),
                "rule_version": r.get::<String, _>("rule_version"),
                "model_version": r.get::<String, _>("model_version"),
                "report_payload": r.get::<serde_json::Value, _>("report_payload"),
                "generated_at": r.get::<chrono::DateTime<chrono::Utc>, _>("generated_at")
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "reports": reports })))
}

#[derive(Deserialize)]
struct GenerateReportRequest {
    report_type: String,
    title: Option<String>,
    vendor_id: Option<Uuid>,
    invoice_id: Option<Uuid>,
    contract_id: Option<Uuid>,
    date_from: Option<NaiveDate>,
    date_to: Option<NaiveDate>,
}

async fn generate_report_handler(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Json(req): Json<GenerateReportRequest>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    require_permission(&claims, "reports:read")?;
    let title = req
        .title
        .unwrap_or_else(|| format!("ABIFR {} Audit Report", req.report_type.replace('_', " ")));

    let report = build_audit_report(
        &state.db,
        &req.report_type,
        &title,
        req.vendor_id,
        req.invoice_id,
        req.contract_id,
        req.date_from,
        req.date_to,
    )
    .await?;

    let payload = serde_json::to_value(&report)
        .map_err(|e| AppError::Internal(format!("Report serialize failed: {}", e)))?;

    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO audit_reports (report_type, title, scope_json, date_from, date_to, rule_version, model_version, report_payload, generated_by)
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
        RETURNING id
        "#,
    )
    .bind(&report.report_type)
    .bind(&report.title)
    .bind(&report.scope)
    .bind(report.date_from)
    .bind(report.date_to)
    .bind(&report.rule_version)
    .bind(&report.model_version)
    .bind(&payload)
    .bind(claims.sub)
    .fetch_one(&state.db)
    .await?;

    state
        .log_audit(Some(claims.sub), Some(&claims.role), "report.generated", "audit_report", Some(&id.to_string()), serde_json::json!({ "report_type": report.report_type }))
        .await;

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id, "report": report }))))
}

#[derive(Deserialize)]
struct ExportQuery {
    format: String,
}

async fn export_report_handler(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<String>,
    Query(q): Query<ExportQuery>,
) -> AppResult<impl IntoResponse> {
    require_permission(&claims, "reports:export")?;
    let rid = parse_uuid(&id, "report_id")?;

    let payload: serde_json::Value = sqlx::query_scalar("SELECT report_payload FROM audit_reports WHERE id = $1")
        .bind(rid)
        .fetch_one(&state.db)
        .await?;

    let report: crate::reporting::StructuredAuditReport = serde_json::from_value(payload)
        .map_err(|e| AppError::Internal(format!("Stored report invalid: {}", e)))?;

    let (bytes, content_type, ext) = match q.format.to_ascii_lowercase().as_str() {
        "csv" => (export_report_csv(&report)?, "text/csv; charset=utf-8", "csv"),
        "xlsx" => (
            export_report_xlsx(&report)?,
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            "xlsx",
        ),
        "pdf" => (export_report_pdf(&report), "application/pdf", "pdf"),
        _ => return Err(AppError::BadRequest("Supported export formats: csv, xlsx, pdf.".into())),
    };

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"abifr-report-{}.{}\"", rid, ext))
            .unwrap(),
    );

    Ok((headers, bytes))
}

// =============================================================================
// Model Configuration, System Settings, Audit Logs & Synthetic Fixtures
// =============================================================================
async fn list_models(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "documents:read")?;
    let rows = sqlx::query(
        "SELECT id, provider_name, display_name, model_identifier, is_enabled, is_default, requires_external_egress, redaction_required, config_json, updated_at FROM model_metadata ORDER BY is_default DESC, provider_name ASC",
    )
    .fetch_all(&state.db)
    .await?;

    let models: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<Uuid, _>("id"),
                "provider_name": r.get::<String, _>("provider_name"),
                "display_name": r.get::<String, _>("display_name"),
                "model_identifier": r.get::<String, _>("model_identifier"),
                "is_enabled": r.get::<bool, _>("is_enabled"),
                "is_default": r.get::<bool, _>("is_default"),
                "requires_external_egress": r.get::<bool, _>("requires_external_egress"),
                "redaction_required": r.get::<bool, _>("redaction_required"),
                "config_json": r.get::<serde_json::Value, _>("config_json")
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "models": models, "enable_external_ai_env": state.config.enable_external_ai })))
}

#[derive(Deserialize)]
struct UpdateModelRequest {
    model_identifier: Option<String>,
    is_enabled: Option<bool>,
    is_default: Option<bool>,
    redaction_required: Option<bool>,
}

async fn update_model_config(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(provider): Path<String>,
    Json(req): Json<UpdateModelRequest>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "settings:write")?;
    if req.is_enabled == Some(true) && provider != "local_deterministic" && !state.config.enable_external_ai {
        return Err(AppError::BadRequest(
            "Cannot enable external AI provider while ENABLE_EXTERNAL_AI=false in server environment.".into(),
        ));
    }

    if req.is_default == Some(true) {
        sqlx::query("UPDATE model_metadata SET is_default = FALSE")
            .execute(&state.db)
            .await?;
    }

    sqlx::query(
        r#"
        UPDATE model_metadata
        SET model_identifier = COALESCE($2, model_identifier),
            is_enabled = COALESCE($3, is_enabled),
            is_default = COALESCE($4, is_default),
            redaction_required = COALESCE($5, redaction_required),
            updated_by = $6,
            updated_at = NOW()
        WHERE provider_name = $1
        "#,
    )
    .bind(&provider)
    .bind(&req.model_identifier)
    .bind(req.is_enabled)
    .bind(req.is_default)
    .bind(req.redaction_required)
    .bind(claims.sub)
    .execute(&state.db)
    .await?;

    state
        .log_audit(Some(claims.sub), Some(&claims.role), "model.updated", "model_metadata", Some(&provider), serde_json::json!({ "is_enabled": req.is_enabled, "is_default": req.is_default }))
        .await;
    Ok(Json(serde_json::json!({ "status": "updated", "provider": provider })))
}

async fn list_settings(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "documents:read")?;
    let rows = sqlx::query("SELECT setting_key, setting_value, description, updated_at FROM system_settings ORDER BY setting_key ASC")
        .fetch_all(&state.db)
        .await?;
    let settings: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "key": r.get::<String, _>("setting_key"),
                "value": r.get::<serde_json::Value, _>("setting_value"),
                "description": r.get::<String, _>("description"),
                "updated_at": r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at")
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "settings": settings })))
}

#[derive(Deserialize)]
struct UpdateSettingRequest {
    value: serde_json::Value,
}

async fn update_setting(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(key): Path<String>,
    Json(req): Json<UpdateSettingRequest>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "settings:write")?;
    sqlx::query("UPDATE system_settings SET setting_value = $2, updated_by = $3, updated_at = NOW() WHERE setting_key = $1")
        .bind(&key)
        .bind(&req.value)
        .bind(claims.sub)
        .execute(&state.db)
        .await?;
    state
        .log_audit(Some(claims.sub), Some(&claims.role), "setting.updated", "system_settings", Some(&key), req.value.clone())
        .await;
    Ok(Json(serde_json::json!({ "status": "updated", "key": key })))
}

async fn list_audit_logs(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "audit:read")?;
    let rows = sqlx::query(
        r#"
        SELECT a.id, a.actor_role, a.action, a.resource_type, a.resource_id, a.details, a.created_at, u.email AS actor_email
        FROM audit_logs a
        LEFT JOIN users u ON u.id = a.actor_user_id
        ORDER BY a.created_at DESC
        LIMIT 200
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    let logs: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "id": r.get::<i64, _>("id"),
                "actor_email": r.get::<Option<String>, _>("actor_email"),
                "actor_role": r.get::<Option<String>, _>("actor_role"),
                "action": r.get::<String, _>("action"),
                "resource_type": r.get::<String, _>("resource_type"),
                "resource_id": r.get::<Option<String>, _>("resource_id"),
                "details": r.get::<serde_json::Value, _>("details"),
                "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at")
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "audit_logs": logs })))
}

async fn load_synthetic_fixtures_handler(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "findings:review")?;
    let summary = load_synthetic_fixtures(&state, claims.sub).await?;
    state
        .log_audit(Some(claims.sub), Some(&claims.role), "fixtures.loaded_synthetic", "system", None, serde_json::to_value(&summary).unwrap())
        .await;
    Ok(Json(serde_json::to_value(summary).unwrap()))
}
