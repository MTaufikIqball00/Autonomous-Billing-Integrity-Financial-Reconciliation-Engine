use super::AuthenticatedUser;
use crate::{
    error::{AppError, AppResult},
    security::{hash_password, issue_jwt, require_permission, verify_jwt, verify_password},
    state::{parse_uuid, AppState},
};
use axum::{
    extract::{ConnectInfo, Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use sqlx::Row;
use std::net::SocketAddr;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

fn extract_client_ip(headers: &HeaderMap, connect_info: Option<&ConnectInfo<SocketAddr>>) -> String {
    if let Some(xff) = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return xff.to_string();
    }
    if let Some(ci) = connect_info {
        return ci.0.ip().to_string();
    }
    "local".into()
}

pub async fn login_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    connect_info: Option<axum::extract::Extension<ConnectInfo<SocketAddr>>>,
    Json(req): Json<LoginRequest>,
) -> AppResult<impl IntoResponse> {
    let client_ip = extract_client_ip(&headers, connect_info.as_ref().map(|e| &e.0));
    let email_lower = req.email.trim().to_ascii_lowercase();

    state
        .login_rate_limiter
        .check(&format!("login_ip:{}", client_ip))?;
    state
        .login_rate_limiter
        .check(&format!("login_ip_email:{}:{}", client_ip, email_lower))?;

    let row = sqlx::query(
        r#"
        SELECT id, email, full_name, password_hash, role_name, is_active, must_rotate_password
        FROM users WHERE LOWER(email) = LOWER($1)
        "#,
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
    let must_rotate_password: bool = row.get("must_rotate_password");

    let (token, csrf, jti, expires_at) =
        issue_jwt(user_id, &email, &full_name, &role, &state.config.jwt_secret)?;

    sqlx::query(
        "INSERT INTO user_sessions (jti, user_id, csrf_token, expires_at) VALUES ($1,$2,$3,$4)",
    )
    .bind(jti)
    .bind(user_id)
    .bind(&csrf)
    .bind(expires_at)
    .execute(&state.db)
    .await?;

    state
        .log_audit(
            Some(user_id),
            Some(&role),
            "auth.login",
            "user",
            Some(&user_id.to_string()),
            serde_json::json!({ "email": email, "ip": client_ip, "jti": jti }),
        )
        .await;

    let secure_attr = if state.config.cookie_secure { "; Secure" } else { "" };
    let cookie = format!(
        "abifr_session={}; HttpOnly; SameSite=Lax; Path=/; Max-Age=43200{}",
        token, secure_attr
    );

    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie)
            .map_err(|_| AppError::Internal("Cookie format error".into()))?,
    );

    Ok((
        resp_headers,
        Json(serde_json::json!({
            "token": token,
            "csrf_token": csrf,
            "user": {
                "id": user_id,
                "email": email,
                "full_name": full_name,
                "role": role,
                "must_rotate_password": must_rotate_password
            }
        })),
    ))
}

pub async fn logout_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // Extract token from Bearer or Cookie if present and revoke session in DB
    let mut token: Option<String> = None;
    if let Some(auth) = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) {
        if let Some(s) = auth.strip_prefix("Bearer ") {
            token = Some(s.trim().to_string());
        }
    }
    if token.is_none() {
        if let Some(cookies) = headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) {
            for pair in cookies.split(';') {
                if let Some(val) = pair.trim().strip_prefix("abifr_session=") {
                    if !val.is_empty() {
                        token = Some(val.to_string());
                    }
                    break;
                }
            }
        }
    }
    if let Some(raw) = token {
        if let Ok(claims) = verify_jwt(&raw, &state.config.jwt_secret) {
            let _ = sqlx::query(
                "UPDATE user_sessions SET revoked_at = NOW() WHERE jti = $1 AND revoked_at IS NULL",
            )
            .bind(claims.jti)
            .execute(&state.db)
            .await;
            state
                .log_audit(
                    Some(claims.sub),
                    Some(&claims.role),
                    "auth.logout",
                    "session",
                    Some(&claims.jti.to_string()),
                    serde_json::json!({}),
                )
                .await;
        }
    }

    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_static("abifr_session=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0"),
    );
    (resp_headers, Json(serde_json::json!({ "status": "logged_out" })))
}

pub async fn me_handler(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
) -> AppResult<Json<serde_json::Value>> {
    let must_rotate: bool =
        sqlx::query_scalar("SELECT must_rotate_password FROM users WHERE id = $1")
            .bind(claims.sub)
            .fetch_optional(&state.db)
            .await?
            .unwrap_or(false);

    Ok(Json(serde_json::json!({
        "id": claims.sub,
        "email": claims.email,
        "full_name": claims.full_name,
        "role": claims.role,
        "csrf_token": claims.csrf,
        "must_rotate_password": must_rotate
    })))
}

#[derive(Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

pub async fn change_password_handler(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Json(req): Json<ChangePasswordRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let current_hash: String =
        sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1")
            .bind(claims.sub)
            .fetch_one(&state.db)
            .await?;

    if !verify_password(&req.current_password, &current_hash) {
        return Err(AppError::Unauthorized("Current password is incorrect.".into()));
    }
    if req.current_password == req.new_password {
        return Err(AppError::BadRequest(
            "New password must differ from current password.".into(),
        ));
    }

    let new_hash = hash_password(&req.new_password)?;
    sqlx::query(
        "UPDATE users SET password_hash = $2, must_rotate_password = FALSE, updated_at = NOW() WHERE id = $1",
    )
    .bind(claims.sub)
    .bind(new_hash)
    .execute(&state.db)
    .await?;

    // Revoke all other active sessions for this user
    sqlx::query(
        "UPDATE user_sessions SET revoked_at = NOW() WHERE user_id = $1 AND jti <> $2 AND revoked_at IS NULL",
    )
    .bind(claims.sub)
    .bind(claims.jti)
    .execute(&state.db)
    .await?;

    state
        .log_audit(
            Some(claims.sub),
            Some(&claims.role),
            "auth.password_changed",
            "user",
            Some(&claims.sub.to_string()),
            serde_json::json!({}),
        )
        .await;

    Ok(Json(serde_json::json!({ "status": "password_updated", "must_rotate_password": false })))
}

pub async fn list_users(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "users:manage")?;
    let rows = sqlx::query(
        "SELECT id, email, full_name, role_name, is_active, must_rotate_password, created_at FROM users ORDER BY created_at DESC",
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
                "must_rotate_password": r.get::<bool, _>("must_rotate_password"),
                "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at")
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "users": items })))
}

#[derive(Deserialize)]
pub struct CreateUserRequest {
    pub email: String,
    pub full_name: String,
    pub password: String,
    pub role: String,
}

pub async fn create_user(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Json(req): Json<CreateUserRequest>,
) -> AppResult<(StatusCode, Json<serde_json::Value>)> {
    require_permission(&claims, "users:manage")?;
    let role = req.role.trim();
    if !matches!(role, "admin" | "reviewer" | "analyst" | "auditor") {
        return Err(AppError::BadRequest("Invalid role name.".into()));
    }
    let pw_hash = hash_password(&req.password)?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (email, full_name, password_hash, role_name, must_rotate_password) VALUES ($1,$2,$3,$4,TRUE) RETURNING id",
    )
    .bind(req.email.trim())
    .bind(req.full_name.trim())
    .bind(pw_hash)
    .bind(role)
    .fetch_one(&state.db)
    .await?;

    state
        .log_audit(
            Some(claims.sub),
            Some(&claims.role),
            "users.created",
            "user",
            Some(&id.to_string()),
            serde_json::json!({ "email": req.email, "role": role }),
        )
        .await;

    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}

#[derive(Deserialize)]
pub struct UpdateUserStatusRequest {
    pub is_active: Option<bool>,
    pub role: Option<String>,
}

pub async fn update_user_status(
    State(state): State<AppState>,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateUserStatusRequest>,
) -> AppResult<Json<serde_json::Value>> {
    require_permission(&claims, "users:manage")?;
    let target_id = parse_uuid(&id, "user_id")?;

    if target_id == claims.sub && req.is_active == Some(false) {
        return Err(AppError::BadRequest("Cannot deactivate your own active session.".into()));
    }

    if let Some(ref role) = req.role {
        if !matches!(role.as_str(), "admin" | "reviewer" | "analyst" | "auditor") {
            return Err(AppError::BadRequest("Invalid role name.".into()));
        }
    }

    sqlx::query(
        r#"
        UPDATE users
        SET is_active = COALESCE($2, is_active),
            role_name = COALESCE($3, role_name),
            updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(target_id)
    .bind(req.is_active)
    .bind(&req.role)
    .execute(&state.db)
    .await?;

    if req.is_active == Some(false) {
        sqlx::query(
            "UPDATE user_sessions SET revoked_at = NOW() WHERE user_id = $1 AND revoked_at IS NULL",
        )
        .bind(target_id)
        .execute(&state.db)
        .await?;
    }

    state
        .log_audit(
            Some(claims.sub),
            Some(&claims.role),
            "users.updated",
            "user",
            Some(&target_id.to_string()),
            serde_json::json!({ "is_active": req.is_active, "role": req.role }),
        )
        .await;

    Ok(Json(serde_json::json!({ "status": "updated", "id": target_id })))
}
