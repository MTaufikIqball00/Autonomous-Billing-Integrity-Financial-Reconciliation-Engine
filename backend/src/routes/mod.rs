pub mod admin;
pub mod auth;
pub mod documents;
pub mod entities;
pub mod findings;

use crate::{
    error::AppError,
    security::{verify_jwt, AuthClaims},
    state::AppState,
};
use axum::{
    extract::FromRequestParts,
    http::{header, request::Parts, Method},
    routing::{get, patch, post},
    Router,
};
use serde::Deserialize;
use sqlx::Row;

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
                        if !val.is_empty() {
                            token = Some(val.to_string());
                            token_from_cookie = true;
                        }
                        break;
                    }
                }
            }
        }

        let raw_token =
            token.ok_or_else(|| AppError::Unauthorized("Missing authentication credentials.".into()))?;
        let claims = verify_jwt(&raw_token, &state.config.jwt_secret)?;

        // Enforce server-side session validity, revocation, and active-user check
        let session_row = sqlx::query(
            r#"
            SELECT s.revoked_at, s.expires_at, u.is_active
            FROM user_sessions s
            JOIN users u ON u.id = s.user_id
            WHERE s.jti = $1 AND s.user_id = $2
            "#,
        )
        .bind(claims.jti)
        .bind(claims.sub)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::Unauthorized("Session not found or expired.".into()))?;

        let revoked_at: Option<chrono::DateTime<chrono::Utc>> = session_row.get("revoked_at");
        let expires_at: chrono::DateTime<chrono::Utc> = session_row.get("expires_at");
        let is_active: bool = session_row.get("is_active");

        if revoked_at.is_some() || expires_at <= chrono::Utc::now() {
            return Err(AppError::Unauthorized("Session has been revoked or expired.".into()));
        }
        if !is_active {
            return Err(AppError::Forbidden("Account has been deactivated.".into()));
        }

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

#[derive(Debug, Deserialize, Default)]
pub struct PaginationQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl PaginationQuery {
    pub fn resolve(&self) -> (i64, i64) {
        let limit = self.limit.unwrap_or(50).clamp(1, 200);
        let offset = self.offset.unwrap_or(0).max(0);
        (limit, offset)
    }
}

pub fn pagination_json(total: i64, limit: i64, offset: i64) -> serde_json::Value {
    serde_json::json!({
        "total": total,
        "limit": limit,
        "offset": offset
    })
}

pub fn build_router(state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(admin::health_handler))
        // Auth & Users
        .route("/auth/login", post(auth::login_handler))
        .route("/auth/logout", post(auth::logout_handler))
        .route("/auth/me", get(auth::me_handler))
        .route("/auth/change-password", post(auth::change_password_handler))
        .route("/users", get(auth::list_users).post(auth::create_user))
        .route("/users/{id}/status", patch(auth::update_user_status))
        // Dashboard Overview
        .route("/dashboard/overview", get(admin::dashboard_overview))
        // Documents, Extraction Viewer & Materialization
        .route("/documents/upload", post(documents::upload_document))
        .route("/documents", get(documents::list_documents))
        .route("/documents/{id}", get(documents::get_document_detail))
        .route("/documents/{id}/reprocess", post(documents::reprocess_document))
        .route("/documents/{id}/classify", patch(documents::reclassify_document))
        .route("/documents/{id}/page/{page}", get(documents::render_document_page))
        .route(
            "/documents/{id}/materialization-preview",
            get(documents::get_materialization_preview),
        )
        .route("/documents/{id}/materialize", post(documents::materialize_document_handler))
        .route("/extracted-fields/{id}/correct", patch(documents::correct_extracted_field))
        // Vendors
        .route("/vendors", get(entities::list_vendors).post(entities::create_vendor))
        .route("/vendors/{id}", patch(entities::update_vendor))
        // Contracts & Amendments
        .route("/contracts", get(entities::list_contracts).post(entities::create_contract))
        .route("/contracts/{id}", get(entities::get_contract_detail))
        .route("/contracts/{id}/versions", post(entities::add_contract_version))
        // Purchase Orders
        .route(
            "/purchase-orders",
            get(entities::list_purchase_orders).post(entities::create_purchase_order),
        )
        // Invoices & Reconciliation
        .route("/invoices", get(entities::list_invoices).post(entities::create_invoice))
        .route("/invoices/{id}", get(entities::get_invoice_detail))
        .route("/invoices/{id}/reconcile", post(entities::reconcile_invoice_handler))
        .route("/invoices/{id}/confirm-match", patch(entities::confirm_invoice_match))
        // Payments
        .route("/payments", get(entities::list_payments).post(entities::create_payment))
        .route("/payments/{id}/allocate", post(entities::allocate_payment))
        // Findings & Human Review
        .route("/findings", get(findings::list_findings))
        .route("/findings/{id}", get(findings::get_finding_detail))
        .route("/findings/{id}/review", post(findings::review_finding))
        .route("/review-queue", get(findings::get_review_queue))
        // Agents
        .route("/agents/runs", get(findings::list_agent_runs))
        // Reports & Exports
        .route("/reports", get(admin::list_reports))
        .route("/reports/generate", post(admin::generate_report_handler))
        .route("/reports/{id}/export", get(admin::export_report_handler))
        // Models, Settings, Audit Logs & Synthetic Fixtures
        .route("/models", get(admin::list_models))
        .route("/models/{provider}", patch(admin::update_model_config))
        .route("/settings", get(admin::list_settings))
        .route("/settings/{key}", patch(admin::update_setting))
        .route("/audit-logs", get(admin::list_audit_logs))
        .route("/fixtures/load-synthetic", post(admin::load_synthetic_fixtures_handler));

    Router::new().nest("/api/v1", api).with_state(state)
}
