use super::{pagination_json, AuthenticatedUser, PaginationQuery};
use crate::{
    error::{AppError, AppResult},
    security::require_permission,
    state::{parse_uuid, AppState},
};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use rust_decimal::Decimal;
use serde::Deserialize;
use sqlx::Row;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct FindingFilterQuery {
    pub category: Option<String>,
    pub status: Option<String>,
    pub severity: Option<String>,
    pub classification_type: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub async fn list_findings(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(q): Query<FindingFilterQuery>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "findings:read")?;
    let pg = PaginationQuery {
        limit: q.limit,
        offset: q.offset,
    };
    let (limit, offset) = pg.resolve();

    let total: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM anomaly_findings f
        WHERE ($1::text IS NULL OR f.category = $1)
          AND ($2::text IS NULL OR f.status = $2)
          AND ($3::text IS NULL OR f.severity = $3)
          AND ($4::text IS NULL OR f.classification_type = $4)
        "#,
    )
    .bind(&q.category)
    .bind(&q.status)
    .bind(&q.severity)
    .bind(&q.classification_type)
    .fetch_one(&state.db)
    .await?;

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
          AND ($3::text IS NULL OR f.severity = $3)
          AND ($4::text IS NULL OR f.classification_type = $4)
        ORDER BY f.detected_at DESC
        LIMIT $5 OFFSET $6
        "#,
    )
    .bind(&q.category)
    .bind(&q.status)
    .bind(&q.severity)
    .bind(&q.classification_type)
    .bind(limit)
    .bind(offset)
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
    Ok(Json(serde_json::json!({
        "findings": findings,
        "pagination": pagination_json(total, limit, offset)
    })))
}

pub async fn get_finding_detail(
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
pub struct ReviewFindingRequest {
    pub action: String,
    pub notes: String,
}

pub async fn review_finding(
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

pub async fn get_review_queue(
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

pub async fn list_agent_runs(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(pg): Query<PaginationQuery>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "documents:read")?;
    let (limit, offset) = pg.resolve();
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_runs")
        .fetch_one(&state.db)
        .await?;

    let rows = sqlx::query(
        r#"
        SELECT id, workflow_run_id, step_number, agent_name, document_id, invoice_id,
               status, model_or_rule_version, input_payload, output_payload,
               evidence_refs, error_message, duration_ms, started_at, completed_at
        FROM agent_runs
        ORDER BY started_at DESC, step_number ASC
        LIMIT $1 OFFSET $2
        "#,
    )
    .bind(limit)
    .bind(offset)
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
    Ok(Json(serde_json::json!({
        "agent_runs": runs,
        "pagination": pagination_json(total, limit, offset)
    })))
}
