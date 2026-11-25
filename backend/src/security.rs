use crate::error::{AppError, AppResult};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
    time::Instant,
};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthClaims {
    pub jti: Uuid,
    pub sub: Uuid,
    pub email: String,
    pub full_name: String,
    pub role: String,
    pub csrf: String,
    pub exp: usize,
    pub iat: usize,
}

pub fn hash_password(password: &str) -> AppResult<String> {
    if password.len() < 8 {
        return Err(AppError::BadRequest(
            "Password must be at least 8 characters long.".into(),
        ));
    }
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Internal(format!("Password hashing failed: {}", e)))
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

pub fn issue_jwt(
    user_id: Uuid,
    email: &str,
    full_name: &str,
    role: &str,
    jwt_secret: &str,
) -> AppResult<(String, String, Uuid, DateTime<Utc>)> {
    let now = Utc::now();
    let expires_at = now + Duration::hours(12);
    let exp = expires_at.timestamp() as usize;
    let iat = now.timestamp() as usize;
    let csrf = Uuid::new_v4().to_string();
    let jti = Uuid::new_v4();

    let claims = AuthClaims {
        jti,
        sub: user_id,
        email: email.to_string(),
        full_name: full_name.to_string(),
        role: role.to_string(),
        csrf: csrf.clone(),
        exp,
        iat,
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_secret.as_bytes()),
    )
    .map_err(|e| AppError::Internal(format!("Token issuance failed: {}", e)))?;

    Ok((token, csrf, jti, expires_at))
}

pub fn verify_jwt(token: &str, jwt_secret: &str) -> AppResult<AuthClaims> {
    decode::<AuthClaims>(
        token,
        &DecodingKey::from_secret(jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .map(|data| data.claims)
    .map_err(|_| AppError::Unauthorized("Invalid or expired authentication token.".into()))
}

/// Role-based authorization matrix
pub fn has_permission(role: &str, permission: &str) -> bool {
    match role {
        "admin" => true,
        "reviewer" => matches!(
            permission,
            "documents:read"
                | "documents:write"
                | "reconciliation:run"
                | "findings:read"
                | "findings:review"
                | "reports:read"
                | "reports:export"
                | "vendors:write"
                | "contracts:write"
                | "payments:write"
                | "audit:read"
        ),
        "analyst" => matches!(
            permission,
            "documents:read"
                | "documents:write"
                | "reconciliation:run"
                | "findings:read"
                | "reports:read"
                | "reports:export"
                | "vendors:write"
                | "contracts:write"
                | "payments:write"
        ),
        "auditor" => matches!(
            permission,
            "documents:read" | "findings:read" | "reports:read" | "reports:export" | "audit:read"
        ),
        _ => false,
    }
}

pub fn require_permission(claims: &AuthClaims, permission: &str) -> AppResult<()> {
    if has_permission(&claims.role, permission) {
        Ok(())
    } else {
        Err(AppError::Forbidden(format!(
            "Role '{}' does not have '{}' permission.",
            claims.role, permission
        )))
    }
}

/// Bounded token-bucket rate limiter per client key with TTL eviction
#[derive(Clone)]
pub struct RateLimiter {
    buckets: Arc<Mutex<HashMap<String, (u32, Instant)>>>,
    max_requests: u32,
    window_secs: u64,
    max_entries: usize,
}

impl RateLimiter {
    pub fn new(max_requests: u32, window_secs: u64) -> Self {
        Self::with_max_entries(max_requests, window_secs, 10_000)
    }

    pub fn with_max_entries(max_requests: u32, window_secs: u64, max_entries: usize) -> Self {
        Self {
            buckets: Arc::new(Mutex::new(HashMap::new())),
            max_requests,
            window_secs,
            max_entries: max_entries.max(1),
        }
    }

    pub fn evict_expired(&self) -> usize {
        if let Ok(mut map) = self.buckets.lock() {
            let now = Instant::now();
            let before = map.len();
            map.retain(|_, (_, ts)| now.duration_since(*ts).as_secs() < self.window_secs);
            return before.saturating_sub(map.len());
        }
        0
    }

    pub fn len(&self) -> usize {
        self.buckets.lock().map(|m| m.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn check(&self, key: &str) -> AppResult<()> {
        let mut map = self
            .buckets
            .lock()
            .map_err(|_| AppError::Internal("Rate limiter lock poisoned".into()))?;
        let now = Instant::now();

        if map.len() >= self.max_entries {
            map.retain(|_, (_, ts)| now.duration_since(*ts).as_secs() < self.window_secs);
            if map.len() >= self.max_entries && !map.contains_key(key) {
                if let Some(oldest_key) = map
                    .iter()
                    .min_by_key(|(_, (_, ts))| *ts)
                    .map(|(k, _)| k.clone())
                {
                    map.remove(&oldest_key);
                }
            }
        }

        let entry = map.entry(key.to_string()).or_insert((0, now));
        if now.duration_since(entry.1).as_secs() >= self.window_secs {
            *entry = (1, now);
            return Ok(());
        }
        if entry.0 >= self.max_requests {
            return Err(AppError::TooManyRequests);
        }
        entry.0 += 1;
        Ok(())
    }
}

/// Detect prompt-injection attempts embedded inside untrusted financial documents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptInjectionSignal {
    pub pattern_name: String,
    pub matched_excerpt: String,
}

pub fn scan_prompt_injection(untrusted_text: &str) -> Vec<PromptInjectionSignal> {
    static PATTERNS: OnceLock<Vec<(&'static str, Regex)>> = OnceLock::new();
    let patterns = PATTERNS.get_or_init(|| {
        vec![
            (
                "ignore_instructions",
                Regex::new(r"(?i)ignore\s+(all\s+)?(previous|prior|above|system)\s+(instructions?|rules?|prompts?)").unwrap(),
            ),
            (
                "override_approval",
                Regex::new(r"(?i)(auto[\s-]?approve|mark\s+as\s+reconciled|skip\s+(verification|audit|reconciliation)|bypass\s+review)").unwrap(),
            ),
            (
                "system_role_spoof",
                Regex::new(r"(?i)(<\s*system\s*>|\[SYSTEM\]|system\s+prompt\s*:|you\s+are\s+now\s+an?\s+admin)").unwrap(),
            ),
            (
                "tool_or_command_exec",
                Regex::new(r"(?i)(execute\s+(shell|sql|command|payment)|drop\s+table|transfer\s+funds\s+immediately)").unwrap(),
            ),
            (
                "secret_exfiltration",
                Regex::new(r"(?i)(reveal|print|dump|exfiltrate)\s+(api[_\s]?key|secret|password|jwt|env)").unwrap(),
            ),
        ]
    });

    let mut signals = Vec::new();
    for (name, regex) in patterns {
        if let Some(m) = regex.find(untrusted_text) {
            let excerpt: String = m.as_str().chars().take(120).collect();
            signals.push(PromptInjectionSignal {
                pattern_name: (*name).to_string(),
                matched_excerpt: excerpt,
            });
        }
    }
    signals
}

/// Validate filename safely (prevents path traversal and null bytes)
pub fn sanitize_original_filename(raw: &str) -> AppResult<String> {
    if raw.contains('\0') {
        return Err(AppError::BadRequest("Invalid filename containing null byte.".into()));
    }
    if raw.contains("..") || raw.contains('/') || raw.contains('\\') {
        return Err(AppError::BadRequest(
            "Filename must not contain path traversal or directory separators.".into(),
        ));
    }
    let cleaned: String = raw
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(240)
        .collect();
    if cleaned.is_empty() {
        return Err(AppError::BadRequest("Filename cannot be empty.".into()));
    }
    Ok(cleaned)
}
