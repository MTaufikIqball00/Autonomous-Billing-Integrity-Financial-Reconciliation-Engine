use anyhow::{Context, Result};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub backend_host: String,
    pub backend_port: u16,
    pub frontend_origin: String,
    pub database_url: String,
    pub redis_url: Option<String>,
    pub jwt_secret: String,
    pub doc_intel_url: String,
    pub doc_intel_internal_token: String,
    pub doc_intel_timeout_secs: u64,
    pub storage_dir: PathBuf,
    pub max_upload_bytes: usize,
    pub cookie_secure: bool,
    pub bootstrap_admin_email: String,
    pub bootstrap_admin_password: String,
    pub ai_provider: String,
    pub enable_external_ai: bool,
    pub redact_before_external_ai: bool,
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        let _ = dotenvy::dotenv();

        let backend_host = std::env::var("BACKEND_HOST").unwrap_or_else(|_| "127.0.0.1".into());
        let backend_port: u16 = std::env::var("BACKEND_PORT")
            .unwrap_or_else(|_| "14800".into())
            .parse()
            .context("BACKEND_PORT must be a valid u16")?;

        let frontend_host = std::env::var("FRONTEND_HOST").unwrap_or_else(|_| "127.0.0.1".into());
        let frontend_port = std::env::var("FRONTEND_PORT").unwrap_or_else(|_| "14300".into());
        let frontend_origin = std::env::var("FRONTEND_ORIGIN")
            .unwrap_or_else(|_| format!("http://{}:{}", frontend_host, frontend_port));

        let database_url = std::env::var("DATABASE_URL")
            .context("DATABASE_URL must be configured")?;

        let redis_url = std::env::var("REDIS_URL")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let jwt_secret = std::env::var("JWT_SECRET")
            .context("JWT_SECRET must be configured (minimum 32 characters)")?;
        if jwt_secret.len() < 32 {
            anyhow::bail!("JWT_SECRET must be at least 32 characters long");
        }

        let doc_intel_host = std::env::var("DOC_INTEL_HOST").unwrap_or_else(|_| "127.0.0.1".into());
        let doc_intel_port = std::env::var("DOC_INTEL_PORT").unwrap_or_else(|_| "14900".into());
        let doc_intel_url = std::env::var("DOC_INTEL_URL")
            .unwrap_or_else(|_| format!("http://{}:{}", doc_intel_host, doc_intel_port));

        let doc_intel_internal_token = std::env::var("DOC_INTEL_INTERNAL_TOKEN")
            .context("DOC_INTEL_INTERNAL_TOKEN must be configured")?;

        let doc_intel_timeout_secs: u64 = std::env::var("DOC_INTEL_TIMEOUT_SECS")
            .unwrap_or_else(|_| "60".into())
            .parse()
            .unwrap_or(60);

        let storage_dir = PathBuf::from(
            std::env::var("STORAGE_DIR").unwrap_or_else(|_| "./storage".into()),
        );

        let max_upload_bytes: usize = std::env::var("MAX_UPLOAD_BYTES")
            .unwrap_or_else(|_| "26214400".into())
            .parse()
            .unwrap_or(26_214_400);

        let cookie_secure = std::env::var("COOKIE_SECURE")
            .map(|v| v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let bootstrap_admin_email = std::env::var("BOOTSTRAP_ADMIN_EMAIL")
            .unwrap_or_else(|_| "admin@abifr.local".into());
        let bootstrap_admin_password = std::env::var("BOOTSTRAP_ADMIN_PASSWORD")
            .unwrap_or_else(|_| "AbifrDevAdmin!2026".into());

        let ai_provider = std::env::var("AI_PROVIDER")
            .unwrap_or_else(|_| "local_deterministic".into());
        let enable_external_ai = std::env::var("ENABLE_EXTERNAL_AI")
            .map(|v| v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let redact_before_external_ai = std::env::var("REDACT_BEFORE_EXTERNAL_AI")
            .map(|v| !v.eq_ignore_ascii_case("false"))
            .unwrap_or(true);

        Ok(Self {
            backend_host,
            backend_port,
            frontend_origin,
            database_url,
            redis_url,
            jwt_secret,
            doc_intel_url,
            doc_intel_internal_token,
            doc_intel_timeout_secs,
            storage_dir,
            max_upload_bytes,
            cookie_secure,
            bootstrap_admin_email,
            bootstrap_admin_password,
            ai_provider,
            enable_external_ai,
            redact_before_external_ai,
        })
    }
}
