use super::{pagination_json, AuthenticatedUser, PaginationQuery};
use crate::{
    error::{AppError, AppResult},
    fixtures::load_synthetic_fixtures,
    reporting::{build_audit_report, export_report_csv, export_report_pdf, export_report_xlsx},
    security::require_permission,
    state::{parse_uuid, AppState},
};
use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Deserialize;
use sqlx::Row;
use uuid::Uuid;

pub async fn health_handler(State(state): State<AppState>) -> AppResult<Json<serde_json::Value>> {
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

pub async fn dashboard_overview(
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

    let vendor_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM vendors")
        .fetch_one(&state.db)
        .await?;
    let contract_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM contracts")
        .fetch_one(&state.db)
        .await?;
    let payment_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM payments")
        .fetch_one(&state.db)
        .await?;
    let unmatched_payments: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM payments WHERE allocation_status = 'unmatched'",
    )
    .fetch_one(&state.db)
    .await?;

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

pub async fn list_reports(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(pg): Query<PaginationQuery>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "reports:read")?;
    let (limit, offset) = pg.resolve();
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_reports")
        .fetch_one(&state.db)
        .await?;

    let rows = sqlx::query(
        "SELECT id, report_type, title, date_from, date_to, rule_version, model_version, report_payload, generated_at FROM audit_reports ORDER BY generated_at DESC LIMIT $1 OFFSET $2",
    )
    .bind(limit)
    .bind(offset)
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
    Ok(Json(serde_json::json!({
        "reports": reports,
        "pagination": pagination_json(total, limit, offset)
    })))
}

#[derive(Deserialize)]
pub struct GenerateReportRequest {
    pub report_type: String,
    pub title: Option<String>,
    pub vendor_id: Option<Uuid>,
    pub invoice_id: Option<Uuid>,
    pub contract_id: Option<Uuid>,
    pub date_from: Option<NaiveDate>,
    pub date_to: Option<NaiveDate>,
}

pub async fn generate_report_handler(
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
        .log_audit(
            Some(claims.sub),
            Some(&claims.role),
            "report.generated",
            "audit_report",
            Some(&id.to_string()),
            serde_json::json!({ "report_type": report.report_type }),
        )
        .await;

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id, "report": report }))))
}

#[derive(Deserialize)]
pub struct ExportQuery {
    pub format: String,
}

pub async fn export_report_handler(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<String>,
    Query(q): Query<ExportQuery>,
) -> AppResult<impl IntoResponse> {
    require_permission(&claims, "reports:export")?;
    let rid = parse_uuid(&id, "report_id")?;

    let payload: serde_json::Value =
        sqlx::query_scalar("SELECT report_payload FROM audit_reports WHERE id = $1")
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

pub async fn list_models(
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
    Ok(Json(serde_json::json!({
        "models": models,
        "enable_external_ai_env": state.config.enable_external_ai
    })))
}

#[derive(Deserialize)]
pub struct UpdateModelRequest {
    pub model_identifier: Option<String>,
    pub is_enabled: Option<bool>,
    pub is_default: Option<bool>,
    pub redaction_required: Option<bool>,
}

pub async fn update_model_config(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(provider): Path<String>,
    Json(req): Json<UpdateModelRequest>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "settings:write")?;
    if req.is_enabled == Some(true)
        && provider != "local_deterministic"
        && !state.config.enable_external_ai
    {
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
        .log_audit(
            Some(claims.sub),
            Some(&claims.role),
            "model.updated",
            "model_metadata",
            Some(&provider),
            serde_json::json!({ "is_enabled": req.is_enabled, "is_default": req.is_default }),
        )
        .await;
    Ok(Json(serde_json::json!({ "status": "updated", "provider": provider })))
}

pub async fn list_settings(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "documents:read")?;
    let rows = sqlx::query(
        "SELECT setting_key, setting_value, description, updated_at FROM system_settings ORDER BY setting_key ASC",
    )
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
pub struct UpdateSettingRequest {
    pub value: serde_json::Value,
}

pub async fn update_setting(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(key): Path<String>,
    Json(req): Json<UpdateSettingRequest>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "settings:write")?;
    sqlx::query(
        "UPDATE system_settings SET setting_value = $2, updated_by = $3, updated_at = NOW() WHERE setting_key = $1",
    )
    .bind(&key)
    .bind(&req.value)
    .bind(claims.sub)
    .execute(&state.db)
    .await?;
    state
        .log_audit(
            Some(claims.sub),
            Some(&claims.role),
            "setting.updated",
            "system_settings",
            Some(&key),
            req.value.clone(),
        )
        .await;
    Ok(Json(serde_json::json!({ "status": "updated", "key": key })))
}

pub async fn list_audit_logs(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(pg): Query<PaginationQuery>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "audit:read")?;
    let (limit, offset) = pg.resolve();
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_logs")
        .fetch_one(&state.db)
        .await?;

    let rows = sqlx::query(
        r#"
        SELECT a.id, a.actor_role, a.action, a.resource_type, a.resource_id, a.details, a.created_at, u.email AS actor_email
        FROM audit_logs a
        LEFT JOIN users u ON u.id = a.actor_user_id
        ORDER BY a.created_at DESC
        LIMIT $1 OFFSET $2
        "#,
    )
    .bind(limit)
    .bind(offset)
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
    Ok(Json(serde_json::json!({
        "audit_logs": logs,
        "pagination": pagination_json(total, limit, offset)
    })))
}

pub async fn load_synthetic_fixtures_handler(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "findings:review")?;
    let summary = load_synthetic_fixtures(&state, claims.sub).await?;
    state
        .log_audit(
            Some(claims.sub),
            Some(&claims.role),
            "fixtures.loaded_synthetic",
            "system",
            None,
            serde_json::to_value(&summary).unwrap(),
        )
        .await;
    Ok(Json(serde_json::to_value(summary).unwrap()))
}
