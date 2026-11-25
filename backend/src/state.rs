use crate::{
    config::AppConfig,
    db::OptionalRedis,
    doc_intel_client::DocIntelClient,
    error::{AppError, AppResult},
    security::{hash_password, RateLimiter},
    storage::DocumentStorage,
};
use rand::{distributions::Alphanumeric, Rng};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub db: PgPool,
    pub redis: OptionalRedis,
    pub storage: DocumentStorage,
    pub doc_intel: DocIntelClient,
    pub api_rate_limiter: RateLimiter,
    pub login_rate_limiter: RateLimiter,
}

fn generate_distinct_random_password() -> String {
    let suffix: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(20)
        .map(char::from)
        .collect();
    format!("Abifr!{}", suffix)
}

impl AppState {
    pub async fn log_audit(
        &self,
        actor_user_id: Option<Uuid>,
        actor_role: Option<&str>,
        action: &str,
        resource_type: &str,
        resource_id: Option<&str>,
        details: serde_json::Value,
    ) {
        let res = sqlx::query(
            r#"
            INSERT INTO audit_logs (actor_user_id, actor_role, action, resource_type, resource_id, details)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(actor_user_id)
        .bind(actor_role)
        .bind(action)
        .bind(resource_type)
        .bind(resource_id)
        .bind(details)
        .execute(&self.db)
        .await;

        if let Err(e) = res {
            tracing::error!(error = %e, action = %action, "Failed to persist audit log");
        }
    }

    pub async fn ensure_bootstrap_users(&self) -> AppResult<()> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&self.db)
            .await?;

        if count == 0 {
            let admin_hash = hash_password(&self.config.bootstrap_admin_password)?;
            sqlx::query(
                r#"
                INSERT INTO users (email, full_name, password_hash, role_name, must_rotate_password)
                VALUES ($1, $2, $3, 'admin', TRUE)
                ON CONFLICT (email) DO NOTHING
                "#,
            )
            .bind(&self.config.bootstrap_admin_email)
            .bind("System Administrator")
            .bind(admin_hash)
            .execute(&self.db)
            .await?;

            // Each default role receives either its dedicated env password or a unique random secret
            let defaults: [(&str, &str, &str, Option<&String>); 3] = [
                (
                    "reviewer@abifr.local",
                    "Senior Financial Reviewer",
                    "reviewer",
                    self.config.bootstrap_reviewer_password.as_ref(),
                ),
                (
                    "analyst@abifr.local",
                    "Billing Operations Analyst",
                    "analyst",
                    self.config.bootstrap_analyst_password.as_ref(),
                ),
                (
                    "auditor@abifr.local",
                    "Internal Compliance Auditor",
                    "auditor",
                    self.config.bootstrap_auditor_password.as_ref(),
                ),
            ];
            for (email, name, role, maybe_pw) in defaults {
                let raw_pw = maybe_pw
                    .cloned()
                    .unwrap_or_else(generate_distinct_random_password);
                let h = hash_password(&raw_pw)?;
                let _ = sqlx::query(
                    r#"
                    INSERT INTO users (email, full_name, password_hash, role_name, must_rotate_password)
                    VALUES ($1, $2, $3, $4, TRUE)
                    ON CONFLICT (email) DO NOTHING
                    "#,
                )
                .bind(email)
                .bind(name)
                .bind(h)
                .bind(role)
                .execute(&self.db)
                .await;
            }
            tracing::info!(
                admin_email = %self.config.bootstrap_admin_email,
                "Bootstrapped initial RBAC accounts with isolated credentials and rotation flags"
            );
        }
        Ok(())
    }

    pub async fn load_severity_rules(&self) -> crate::reconciliation::SeverityRules {
        let row: Option<serde_json::Value> = sqlx::query_scalar(
            "SELECT setting_value FROM system_settings WHERE setting_key = 'severity_rules'",
        )
        .fetch_optional(&self.db)
        .await
        .ok()
        .flatten();

        row.and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default()
    }
}

pub fn parse_uuid(raw: &str, label: &str) -> AppResult<Uuid> {
    Uuid::parse_str(raw).map_err(|_| AppError::BadRequest(format!("Invalid UUID for {}", label)))
}
