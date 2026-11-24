use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone)]
pub struct DocIntelClient {
    http: reqwest::Client,
    base_url: String,
    internal_token: String,
}

#[derive(Debug, Serialize)]
struct ExtractRequest<'a> {
    file_base64: String,
    extension: &'a str,
    provider: Option<&'a str>,
    model: Option<&'a str>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocIntelClassification {
    pub category: String,
    pub confidence: Option<f64>,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocIntelExtractedField {
    pub field_name: String,
    pub line_item_index: Option<i32>,
    pub extracted_value: Option<String>,
    pub original_text: Option<String>,
    pub page_number: Option<i32>,
    pub bounding_box: Option<serde_json::Value>,
    pub source_location: Option<String>,
    pub extraction_method: String,
    pub provider_confidence: Option<f64>,
    pub validation_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocIntelPromptSignal {
    pub pattern_name: String,
    pub matched_excerpt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocIntelExtractResponse {
    pub page_count: i32,
    pub extraction_method: String,
    pub provider_name: String,
    pub model_version: String,
    pub provider_error: Option<String>,
    pub redaction_applied: bool,
    pub duration_ms: i32,
    pub classification: DocIntelClassification,
    pub raw_text: String,
    pub pages: serde_json::Value,
    pub tables: serde_json::Value,
    pub fields: Vec<DocIntelExtractedField>,
    pub structured_payload: serde_json::Value,
    pub prompt_injection_signals: Vec<DocIntelPromptSignal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedPageResponse {
    pub page_number: i32,
    pub page_count: i32,
    pub image_width: i32,
    pub image_height: i32,
    pub coord_width: f64,
    pub coord_height: f64,
    pub mime_type: String,
    pub data_base64: String,
}

impl DocIntelClient {
    pub fn new(base_url: String, internal_token: String, timeout_secs: u64) -> AppResult<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .map_err(|e| AppError::Internal(format!("HTTP client init failed: {}", e)))?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            internal_token,
        })
    }

    pub async fn health(&self) -> serde_json::Value {
        let url = format!("{}/internal/health", self.base_url);
        match self.http.get(&url).timeout(Duration::from_secs(3)).send().await {
            Ok(resp) if resp.status().is_success() => resp
                .json::<serde_json::Value>()
                .await
                .unwrap_or_else(|_| serde_json::json!({"status": "degraded"})),
            Ok(resp) => serde_json::json!({"status": "error", "http_status": resp.status().as_u16()}),
            Err(e) => serde_json::json!({"status": "unreachable", "detail": e.to_string()}),
        }
    }

    pub async fn extract(
        &self,
        file_bytes: &[u8],
        extension: &str,
        provider: Option<&str>,
        model: Option<&str>,
    ) -> AppResult<DocIntelExtractResponse> {
        let b64 = base64_encode(file_bytes);
        let req = ExtractRequest {
            file_base64: b64,
            extension,
            provider,
            model,
        };
        let url = format!("{}/internal/extract", self.base_url);

        // Safe bounded retry (max 2 attempts) for transient network errors only
        let mut last_err = String::new();
        for attempt in 1..=2 {
            let res = self
                .http
                .post(&url)
                .header("X-Internal-Token", &self.internal_token)
                .json(&req)
                .send()
                .await;

            match res {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        return resp
                            .json::<DocIntelExtractResponse>()
                            .await
                            .map_err(|e| AppError::UpstreamError(format!("Invalid doc-intel JSON: {}", e)));
                    }
                    let txt = resp.text().await.unwrap_or_default();
                    if status.as_u16() == 422 {
                        return Err(AppError::BadRequest(format!("Document could not be parsed: {}", txt)));
                    }
                    last_err = format!("HTTP {}: {}", status.as_u16(), txt);
                }
                Err(e) => {
                    last_err = e.to_string();
                }
            }
            if attempt < 2 {
                tokio::time::sleep(Duration::from_millis(400)).await;
            }
        }
        Err(AppError::UpstreamError(last_err))
    }

    pub async fn render_page(
        &self,
        file_bytes: &[u8],
        extension: &str,
        page_number: i32,
    ) -> AppResult<RenderedPageResponse> {
        let url = format!("{}/internal/render-page", self.base_url);
        let payload = serde_json::json!({
            "file_base64": base64_encode(file_bytes),
            "extension": extension,
            "page_number": page_number
        });
        let resp = self
            .http
            .post(&url)
            .header("X-Internal-Token", &self.internal_token)
            .json(&payload)
            .send()
            .await
            .map_err(|e| AppError::UpstreamError(e.to_string()))?;

        if !resp.status().is_success() {
            let txt = resp.text().await.unwrap_or_default();
            return Err(AppError::BadRequest(format!("Page preview unavailable: {}", txt)));
        }
        resp.json::<RenderedPageResponse>()
            .await
            .map_err(|e| AppError::UpstreamError(format!("Invalid page render response: {}", e)))
    }
}

/// Minimal zero-dependency RFC 4648 base64 encoder
pub fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    let mut i = 0;
    while i < input.len() {
        let b0 = input[i] as u32;
        let b1 = if i + 1 < input.len() { input[i + 1] as u32 } else { 0 };
        let b2 = if i + 2 < input.len() { input[i + 2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
        if i + 1 < input.len() {
            out.push(TABLE[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if i + 2 < input.len() {
            out.push(TABLE[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        i += 3;
    }
    out
}
