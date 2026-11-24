use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Authentication required")]
    Unauthorized(String),
    #[error("Access denied")]
    Forbidden(String),
    #[error("Resource not found")]
    NotFound(String),
    #[error("Invalid request")]
    BadRequest(String),
    #[error("Conflict")]
    Conflict(String),
    #[error("Rate limit exceeded")]
    TooManyRequests,
    #[error("Upstream document service error")]
    UpstreamError(String),
    #[error("Internal server error")]
    Internal(String),
}

#[derive(Serialize)]
struct ErrorBody {
    error_code: &'static str,
    message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_code, message) = match self {
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED", msg),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, "FORBIDDEN", msg),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, "NOT_FOUND", msg),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "BAD_REQUEST", msg),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, "CONFLICT", msg),
            AppError::TooManyRequests => (
                StatusCode::TOO_MANY_REQUESTS,
                "RATE_LIMITED",
                "Too many requests; please retry shortly.".to_string(),
            ),
            AppError::UpstreamError(msg) => {
                tracing::error!(detail = %msg, "Upstream service error");
                (
                    StatusCode::BAD_GATEWAY,
                    "UPSTREAM_ERROR",
                    "Document intelligence worker failed to process the request.".to_string(),
                )
            }
            AppError::Internal(msg) => {
                // Never leak internal DB/filesystem details to clients
                tracing::error!(detail = %msg, "Internal server error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "An internal error occurred. Reference server audit logs.".to_string(),
                )
            }
        };

        (
            status,
            Json(ErrorBody {
                error_code,
                message,
            }),
        )
            .into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match &err {
            sqlx::Error::RowNotFound => AppError::NotFound("Requested record was not found.".into()),
            sqlx::Error::Database(db_err) => {
                if db_err.code().as_deref() == Some("23505") {
                    return AppError::Conflict("A record with this unique identifier already exists.".into());
                }
                if db_err.code().as_deref() == Some("23503") {
                    return AppError::BadRequest("Referenced entity does not exist.".into());
                }
                AppError::Internal(format!("Database error: {}", db_err))
            }
            _ => AppError::Internal(format!("Database error: {}", err)),
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;
