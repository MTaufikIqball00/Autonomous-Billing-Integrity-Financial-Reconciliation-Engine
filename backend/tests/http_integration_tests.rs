use abifr_backend::{
    config::AppConfig,
    db::{connect_postgres, OptionalRedis},
    doc_intel_client::DocIntelClient,
    routes::build_router,
    security::RateLimiter,
    state::AppState,
    storage::DocumentStorage,
};
use axum::{
    body::Body,
    http::{header, HeaderMap, Method, Request, StatusCode},
    Router,
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::path::PathBuf;
use tempfile::{tempdir, TempDir};
use tower::ServiceExt;
use uuid::Uuid;

struct TestHarness {
    router: Router,
    admin_email: String,
    admin_password: String,
    _storage_dir: TempDir,
}

impl TestHarness {
    async fn from_env_or_skip() -> Option<Self> {
        let database_url = match std::env::var("DATABASE_URL") {
            Ok(v) if !v.trim().is_empty() => v,
            _ => {
                eprintln!("[SKIP] http_integration_tests skipped because DATABASE_URL is not set");
                return None;
            }
        };

        let storage_tmp = tempdir().expect("create temp storage dir");
        let jwt_secret = std::env::var("JWT_SECRET")
            .ok()
            .filter(|s| s.len() >= 32)
            .unwrap_or_else(|| "test_integration_jwt_secret_minimum_32_characters_long".into());
        let doc_intel_token = std::env::var("DOC_INTEL_INTERNAL_TOKEN")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "test_internal_doc_intel_token".into());
        let admin_email = std::env::var("BOOTSTRAP_ADMIN_EMAIL")
            .unwrap_or_else(|_| "admin@abifr.local".into());
        let admin_password = std::env::var("BOOTSTRAP_ADMIN_PASSWORD")
            .unwrap_or_else(|_| "AbifrDevAdmin!2026".into());

        let config = AppConfig {
            backend_host: "127.0.0.1".into(),
            backend_port: 14800,
            frontend_origin: "http://127.0.0.1:14300".into(),
            database_url: database_url.clone(),
            redis_url: None,
            jwt_secret,
            doc_intel_url: "http://127.0.0.1:14900".into(),
            doc_intel_internal_token: doc_intel_token.clone(),
            doc_intel_timeout_secs: 15,
            storage_dir: PathBuf::from(storage_tmp.path()),
            max_upload_bytes: 5 * 1024 * 1024,
            cookie_secure: false,
            bootstrap_admin_email: admin_email.clone(),
            bootstrap_admin_password: admin_password.clone(),
            bootstrap_reviewer_password: None,
            bootstrap_analyst_password: None,
            bootstrap_auditor_password: None,
            ai_provider: "local_deterministic".into(),
            enable_external_ai: false,
            redact_before_external_ai: true,
        };

        let db = connect_postgres(&database_url)
            .await
            .expect("connect to PostgreSQL for HTTP integration tests");
        let storage = DocumentStorage::new(config.storage_dir.clone(), config.max_upload_bytes);
        storage.ensure_initialized().await.expect("init storage");
        let doc_intel = DocIntelClient::new(config.doc_intel_url.clone(), doc_intel_token, 15)
            .expect("build doc_intel client");

        let state = AppState {
            config,
            db,
            redis: OptionalRedis::connect(None).await,
            storage,
            doc_intel,
            api_rate_limiter: RateLimiter::new(500, 60),
            login_rate_limiter: RateLimiter::new(5, 300),
        };
        state
            .ensure_bootstrap_users()
            .await
            .expect("bootstrap default RBAC users");

        let router = build_router(state);
        Some(Self {
            router,
            admin_email,
            admin_password,
            _storage_dir: storage_tmp,
        })
    }

    async fn send(&self, req: Request<Body>) -> (StatusCode, HeaderMap, Vec<u8>) {
        let resp = self
            .router
            .clone()
            .oneshot(req)
            .await
            .expect("router oneshot");
        let status = resp.status();
        let headers = resp.headers().clone();
        let bytes = resp
            .into_body()
            .collect()
            .await
            .expect("collect body")
            .to_bytes()
            .to_vec();
        (status, headers, bytes)
    }

    async fn login(&self, email: &str, password: &str, client_ip: &str) -> (StatusCode, String, String, Value) {
        let req = Request::builder()
            .method(Method::POST)
            .uri("/api/v1/auth/login")
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-forwarded-for", client_ip)
            .body(Body::from(json!({ "email": email, "password": password }).to_string()))
            .unwrap();
        let (status, headers, bytes) = self.send(req).await;
        let cookie = headers
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .find_map(|s| s.split(';').next().map(|p| p.trim().to_string()))
            .unwrap_or_default();
        let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        let csrf = body["csrf_token"].as_str().unwrap_or("").to_string();
        (status, cookie, csrf, body)
    }

    async fn api_json(
        &self,
        method: Method,
        uri: &str,
        auth: (&str, Option<&str>),
        payload: Option<Value>,
    ) -> (StatusCode, Value) {
        let (cookie, csrf) = auth;
        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::COOKIE, cookie);
        if let Some(c) = csrf {
            builder = builder.header("X-CSRF-Token", c);
        }
        let body = if let Some(p) = payload {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(p.to_string())
        } else {
            Body::empty()
        };
        let (status, _, bytes) = self.send(builder.body(body).unwrap()).await;
        let val: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, val)
    }
}

fn build_multipart_pdf(filename: &str, pdf_bytes: &[u8]) -> (String, Vec<u8>) {
    let boundary = "----AbifrIntegrationBoundary987654";
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"async_only\"\r\n\r\ntrue\r\n");
    body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\nContent-Type: application/pdf\r\n\r\n",
            filename
        )
        .as_bytes(),
    );
    body.extend_from_slice(pdf_bytes);
    body.extend_from_slice(format!("\r\n--{}--\r\n", boundary).as_bytes());
    (format!("multipart/form-data; boundary={}", boundary), body)
}

#[tokio::test]
async fn test_http_api_security_rbac_materialization_reconciliation_and_reports() {
    let Some(harness) = TestHarness::from_env_or_skip().await else {
        return;
    };
    let tag = &Uuid::new_v4().simple().to_string()[..8];

    // 1. Login sets HttpOnly cookie & returns CSRF token; /auth/me succeeds via cookie
    let (st, admin_cookie, admin_csrf, login_json) = harness
        .login(&harness.admin_email, &harness.admin_password, "10.10.0.1")
        .await;
    assert_eq!(st, StatusCode::OK, "admin login body: {}", login_json);
    assert!(admin_cookie.starts_with("abifr_session="));
    assert!(!admin_csrf.is_empty());

    let (me_st, me_json) = harness
        .api_json(Method::GET, "/api/v1/auth/me", (&admin_cookie, None), None)
        .await;
    assert_eq!(me_st, StatusCode::OK);
    assert_eq!(me_json["role"], "admin");

    // 2. CSRF protection enforced on mutating endpoints using Cookie auth
    let vendor_payload = json!({
        "vendor_code": format!("INT-VND-{}", tag),
        "legal_name": format!("Integration Vendor {}", tag),
        "default_currency": "USD",
        "payment_terms": "Net 30"
    });
    let (no_csrf_st, _) = harness
        .api_json(
            Method::POST,
            "/api/v1/vendors",
            (&admin_cookie, None),
            Some(vendor_payload.clone()),
        )
        .await;
    assert_eq!(no_csrf_st, StatusCode::FORBIDDEN);

    let (bad_csrf_st, _) = harness
        .api_json(
            Method::POST,
            "/api/v1/vendors",
            (&admin_cookie, Some("invalid-csrf-token")),
            Some(vendor_payload.clone()),
        )
        .await;
    assert_eq!(bad_csrf_st, StatusCode::FORBIDDEN);

    let (ok_vnd_st, vnd_json) = harness
        .api_json(
            Method::POST,
            "/api/v1/vendors",
            (&admin_cookie, Some(&admin_csrf)),
            Some(vendor_payload),
        )
        .await;
    assert_eq!(ok_vnd_st, StatusCode::CREATED, "vendor create: {}", vnd_json);
    let vendor_id = vnd_json["id"].as_str().expect("vendor id").to_string();

    // 3. Session revocation on POST /auth/logout -> subsequent /auth/me rejected (401)
    let (s2_st, temp_cookie, temp_csrf, _) = harness
        .login(&harness.admin_email, &harness.admin_password, "10.10.0.2")
        .await;
    assert_eq!(s2_st, StatusCode::OK);
    let (out_st, _) = harness
        .api_json(
            Method::POST,
            "/api/v1/auth/logout",
            (&temp_cookie, Some(&temp_csrf)),
            None,
        )
        .await;
    assert_eq!(out_st, StatusCode::OK);
    let (revoked_st, _) = harness
        .api_json(Method::GET, "/api/v1/auth/me", (&temp_cookie, None), None)
        .await;
    assert_eq!(revoked_st, StatusCode::UNAUTHORIZED);

    // 4. Dedicated strict login rate limiter on IP without affecting other IPs
    for attempt in 1..=5 {
        let (rl_st, _, _, _) = harness
            .login(&harness.admin_email, "WrongPassword!999", "198.51.100.77")
            .await;
        assert_eq!(rl_st, StatusCode::UNAUTHORIZED, "attempt {}", attempt);
    }
    let (blocked_st, _, _, _) = harness
        .login(&harness.admin_email, "WrongPassword!999", "198.51.100.77")
        .await;
    assert_eq!(blocked_st, StatusCode::TOO_MANY_REQUESTS);

    let (other_ip_st, _, _, _) = harness
        .login(&harness.admin_email, &harness.admin_password, "198.51.100.88")
        .await;
    assert_eq!(other_ip_st, StatusCode::OK);

    // 5 & 6. Provision temporary RBAC test users (reviewer, analyst, auditor, deactivated)
    let temp_secret = format!("AbifrTempRolePass!{}", tag);
    let mut created_users = std::collections::HashMap::new();
    for role in ["reviewer", "analyst", "auditor", "deact"] {
        let target_role = if role == "deact" { "analyst" } else { role };
        let email = format!("int_{}_{}@abifr.local", role, tag);
        let (u_st, u_json) = harness
            .api_json(
                Method::POST,
                "/api/v1/users",
                (&admin_cookie, Some(&admin_csrf)),
                Some(json!({
                    "email": email,
                    "full_name": format!("Test {} {}", role, tag),
                    "password": temp_secret,
                    "role": target_role
                })),
            )
            .await;
        assert_eq!(u_st, StatusCode::CREATED, "create {}: {}", role, u_json);
        created_users.insert(role, (u_json["id"].as_str().unwrap().to_string(), email));
    }

    // Deactivated user check: session invalidation + login denial
    let (deact_id, deact_email) = &created_users["deact"];
    let (d_st, deact_cookie, _, _) = harness.login(deact_email, &temp_secret, "10.10.0.3").await;
    assert_eq!(d_st, StatusCode::OK);
    let (deact_ok, _) = harness
        .api_json(Method::GET, "/api/v1/auth/me", (&deact_cookie, None), None)
        .await;
    assert_eq!(deact_ok, StatusCode::OK);

    let (patch_st, _) = harness
        .api_json(
            Method::PATCH,
            &format!("/api/v1/users/{}/status", deact_id),
            (&admin_cookie, Some(&admin_csrf)),
            Some(json!({ "is_active": false })),
        )
        .await;
    assert_eq!(patch_st, StatusCode::OK);

    let (deact_session_after, _) = harness
        .api_json(Method::GET, "/api/v1/auth/me", (&deact_cookie, None), None)
        .await;
    assert_eq!(deact_session_after, StatusCode::UNAUTHORIZED);
    let (deact_relogin, _, _, _) = harness.login(deact_email, &temp_secret, "10.10.0.4").await;
    assert_eq!(deact_relogin, StatusCode::FORBIDDEN);

    // RBAC checks across auditor / analyst / reviewer
    let (aud_st, aud_cookie, aud_csrf, _) = harness
        .login(&created_users["auditor"].1, &temp_secret, "10.10.0.5")
        .await;
    assert_eq!(aud_st, StatusCode::OK);
    let (aud_read_logs, _) = harness
        .api_json(Method::GET, "/api/v1/audit-logs", (&aud_cookie, None), None)
        .await;
    assert_eq!(aud_read_logs, StatusCode::OK);
    let (aud_write_vnd, _) = harness
        .api_json(
            Method::POST,
            "/api/v1/vendors",
            (&aud_cookie, Some(&aud_csrf)),
            Some(json!({ "vendor_code": "AUD-BLK", "legal_name": "Blocked Vendor" })),
        )
        .await;
    assert_eq!(aud_write_vnd, StatusCode::FORBIDDEN);

    let (an_st, an_cookie, an_csrf, _) = harness
        .login(&created_users["analyst"].1, &temp_secret, "10.10.0.6")
        .await;
    assert_eq!(an_st, StatusCode::OK);
    let (an_user_blocked, _) = harness
        .api_json(
            Method::POST,
            "/api/v1/users",
            (&an_cookie, Some(&an_csrf)),
            Some(json!({
                "email": "nope@abifr.local",
                "full_name": "Nope",
                "password": temp_secret,
                "role": "auditor"
            })),
        )
        .await;
    assert_eq!(an_user_blocked, StatusCode::FORBIDDEN);

    let (rev_st, rev_cookie, rev_csrf, _) = harness
        .login(&created_users["reviewer"].1, &temp_secret, "10.10.0.7")
        .await;
    assert_eq!(rev_st, StatusCode::OK);

    // 7. End-to-end API flow: contract -> upload PDF -> preview -> classify -> materialize -> reconcile -> review -> report CSV/XLSX/PDF
    let contract_num = format!("INT-CTR-{}", tag);
    let (ctr_st, ctr_json) = harness
        .api_json(
            Method::POST,
            "/api/v1/contracts",
            (&admin_cookie, Some(&admin_csrf)),
            Some(json!({
                "contract_number": contract_num,
                "vendor_id": vendor_id,
                "title": "HTTP Integration Master Contract",
                "currency": "USD",
                "effective_date": "2026-01-01",
                "expiration_date": "2026-12-31",
                "line_items": [{
                    "item_code": "SKU-INT",
                    "description": "Integration Cloud Node",
                    "unit": "unit",
                    "unit_price": "100.00"
                }]
            })),
        )
        .await;
    assert_eq!(ctr_st, StatusCode::CREATED, "contract: {}", ctr_json);

    let pdf_payload = format!("%PDF-1.4\n% INT-TEST-{}\n1 0 obj<<>>endobj\n%%EOF", tag);
    let (ct_hdr, mp_body) = build_multipart_pdf(&format!("int_inv_{}.pdf", tag), pdf_payload.as_bytes());
    let up_req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/documents/upload")
        .header(header::COOKIE, &admin_cookie)
        .header("X-CSRF-Token", &admin_csrf)
        .header(header::CONTENT_TYPE, ct_hdr)
        .body(Body::from(mp_body))
        .unwrap();
    let (up_st, _, up_bytes) = harness.send(up_req).await;
    let up_json: Value = serde_json::from_slice(&up_bytes).unwrap();
    assert_eq!(up_st, StatusCode::CREATED, "upload: {}", up_json);
    let doc_id = up_json["id"].as_str().unwrap().to_string();

    let (prev_st, prev_json) = harness
        .api_json(
            Method::GET,
            &format!("/api/v1/documents/{}/materialization-preview", doc_id),
            (&admin_cookie, None),
            None,
        )
        .await;
    assert_eq!(prev_st, StatusCode::OK, "preview: {}", prev_json);
    assert_eq!(prev_json["already_materialized"], false);

    let (cls_st, _) = harness
        .api_json(
            Method::PATCH,
            &format!("/api/v1/documents/{}/classify", doc_id),
            (&admin_cookie, Some(&admin_csrf)),
            Some(json!({ "category": "invoice", "reason": "HTTP integration test classification" })),
        )
        .await;
    assert_eq!(cls_st, StatusCode::OK);

    let inv_num = format!("INT-INV-{}", tag);
    let (mat_st, mat_json) = harness
        .api_json(
            Method::POST,
            &format!("/api/v1/documents/{}/materialize", doc_id),
            (&admin_cookie, Some(&admin_csrf)),
            Some(json!({
                "vendor_id": vendor_id,
                "document_number": inv_num,
                "document_date": "2026-08-20",
                "currency": "USD",
                "contract_number": contract_num,
                "subtotal": "270.00",
                "total_amount": "270.00",
                "auto_reconcile": true,
                "line_items": [{
                    "item_code": "SKU-INT",
                    "description": "Integration Cloud Node",
                    "quantity": "2",
                    "unit": "unit",
                    "unit_price": "135.00",
                    "stated_line_total": "270.00"
                }]
            })),
        )
        .await;
    assert_eq!(mat_st, StatusCode::CREATED, "materialize: {}", mat_json);
    assert_eq!(mat_json["entity_type"], "invoice");
    let invoice_id = mat_json["entity_id"].as_str().unwrap().to_string();

    let (rec_st, rec_json) = harness
        .api_json(
            Method::POST,
            &format!("/api/v1/invoices/{}/reconcile", invoice_id),
            (&admin_cookie, Some(&admin_csrf)),
            None,
        )
        .await;
    assert_eq!(rec_st, StatusCode::OK, "reconcile: {}", rec_json);

    let (fnd_st, fnd_json) = harness
        .api_json(
            Method::GET,
            "/api/v1/findings?limit=2&offset=0&category=CONTRACT_PRICE_MISMATCH",
            (&admin_cookie, None),
            None,
        )
        .await;
    assert_eq!(fnd_st, StatusCode::OK);
    assert_eq!(fnd_json["pagination"]["limit"], 2);
    assert_eq!(fnd_json["pagination"]["offset"], 0);
    let findings_arr = fnd_json["findings"].as_array().unwrap();
    assert!(!findings_arr.is_empty(), "expected CONTRACT_PRICE_MISMATCH finding");
    let finding_id = findings_arr[0]["id"].as_str().unwrap().to_string();

    // Analyst blocked from human review; reviewer allowed
    let (an_rev_st, _) = harness
        .api_json(
            Method::POST,
            &format!("/api/v1/findings/{}/review", finding_id),
            (&an_cookie, Some(&an_csrf)),
            Some(json!({ "action": "approve", "notes": "analyst should be denied" })),
        )
        .await;
    assert_eq!(an_rev_st, StatusCode::FORBIDDEN);

    let (rev_act_st, rev_act_json) = harness
        .api_json(
            Method::POST,
            &format!("/api/v1/findings/{}/review", finding_id),
            (&rev_cookie, Some(&rev_csrf)),
            Some(json!({ "action": "approve", "notes": "Confirmed price mismatch in HTTP integration test" })),
        )
        .await;
    assert_eq!(rev_act_st, StatusCode::OK, "review: {}", rev_act_json);
    assert_eq!(rev_act_json["reviewer_status"], "approved");

    // Generate audit report and verify CSV / XLSX / PDF export bytes
    let (rep_st, rep_json) = harness
        .api_json(
            Method::POST,
            "/api/v1/reports/generate",
            (&aud_cookie, Some(&aud_csrf)),
            Some(json!({
                "report_type": "periodic",
                "invoice_id": invoice_id,
                "title": format!("Integration Report {}", tag)
            })),
        )
        .await;
    assert_eq!(rep_st, StatusCode::CREATED, "report: {}", rep_json);
    let report_id = rep_json["id"].as_str().unwrap();

    for (fmt, prefix) in [
        ("csv", b"finding_id".as_slice()),
        ("xlsx", b"PK\x03\x04".as_slice()),
        ("pdf", b"%PDF-1.4".as_slice()),
    ] {
        let exp_req = Request::builder()
            .method(Method::GET)
            .uri(format!("/api/v1/reports/{}/export?format={}", report_id, fmt))
            .header(header::COOKIE, &aud_cookie)
            .body(Body::empty())
            .unwrap();
        let (exp_st, _, exp_bytes) = harness.send(exp_req).await;
        assert_eq!(exp_st, StatusCode::OK, "export {}", fmt);
        assert!(
            exp_bytes.starts_with(prefix),
            "export {} unexpected signature",
            fmt
        );
    }
}
