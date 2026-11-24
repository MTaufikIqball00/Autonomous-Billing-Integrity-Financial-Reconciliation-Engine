# Architecture & Technical Decisions
## Agentic Billing Integrity & Financial Reconciliation (ABIFR)

### 1. Environment-Verified Architectural Decisions
Based on direct workspace and environment inspection (Windows host, Rust 1.99.1, Node 24.15.0, Python 3.10.0, no Docker/WSL/system PostgreSQL/Redis pre-installed):

1. **Primary Backend — Rust (`backend/`)**:
   - Framework: `axum` 0.8 + `tokio` + `tower-http` (CORS, tracing, body limits).
   - Database: `sqlx` 0.8 (`postgres`, `rust_decimal`, `chrono`, `uuid`, `json`, `migrate`).
   - Exact Financial Arithmetic: `rust_decimal` (`Decimal`) exclusively — **zero binary floating-point arithmetic** (`f32`/`f64`) in financial calculations, tax computations, discount validations, variance calculations, or totals.
   - Authentication & RBAC: `argon2` password hashing + HMAC-SHA256 signed HTTP-only session cookies / Bearer JWT (`jsonwebtoken`) + CSRF token verification on state-changing cookie requests + strict RBAC (`admin`, `reviewer`, `analyst`, `auditor`).
   - Security: Per-IP rate limiting, magic-byte + MIME + extension validation, SHA-256 content fingerprinting, immutable content-addressed storage preventing path traversal (`storage/blobs/<sha256>`), parameterized SQLx queries everywhere, prompt-injection sanitization & isolation boundary for document text.
   - Queue: Durable PostgreSQL transactional queue (`processing_jobs` with `FOR UPDATE SKIP LOCKED`, idempotency keys, bounded retries) plus optional Redis pub/sub & list integration (`redis` crate) activated when `REDIS_URL` is configured.
   - Managed PostgreSQL Runner (`backend/src/bin/pg_manager.rs`): Uses `postgresql_embedded` to provision and run an isolated, genuine PostgreSQL 16 instance bound exclusively to `127.0.0.1:14543` with data persisted in `.runtime/pgdata/`, while also supporting any external PostgreSQL via `DATABASE_URL`.

2. **Document Intelligence Service — Python (`doc-intel/`)**:
   - Strictly scoped to document parsing, OCR, table extraction, and optional VLM provider calls (OpenAI / Google Gemini / Ollama-compatible HTTP APIs).
   - **Not a competing backend**: Uses Python's standard `http.server.ThreadingHTTPServer` (no FastAPI) bound strictly to `127.0.0.1:14900`, authenticated by an internal shared secret header (`X-Internal-Token`).
   - Supported verified formats:
     - Digital & Scanned PDF (`pymupdf` text/table/bbox extraction + `rapidocr-onnxruntime` PaddleOCR PP-OCRv4 fallback for scanned pages)
     - Images: PNG, JPEG, TIFF (`Pillow` + `rapidocr-onnxruntime` OCR with word/line bounding boxes & provider-supplied OCR confidence)
     - DOCX (`python-docx` paragraphs + tables)
     - XLSX (`openpyxl` sheets/rows/cells with exact sheet & cell provenance)
     - CSV (`csv` stdlib with row/column provenance)
   - Configurable VLM Provider Abstraction: `local_deterministic` (default — zero external data egress!), `openai` (requires explicit `ENABLE_EXTERNAL_AI=true` + `OPENAI_API_KEY`), `gemini` (requires explicit `ENABLE_EXTERNAL_AI=true` + `GEMINI_API_KEY`). Includes PII/secret redaction option before any external call.
   - Prompt-Injection Defense: All extracted text is treated strictly as untrusted data; prompt-injection patterns are detected, flagged, and quarantined from control flow. No extracted field ever invents a confidence score (`null` when provider does not supply one).

3. **Frontend — Next.js + React + Tailwind CSS (`frontend/`)**:
   - App Router (`Next.js`), TypeScript, Tailwind CSS.
   - Communicates exclusively with the Rust backend API (`http://127.0.0.1:14800/api/v1`).
   - All metrics, charts, tables, findings, agent runs, and reports are driven strictly by live PostgreSQL queries via Rust endpoints — zero fabricated numbers.
   - Complete enterprise modules: Executive Overview, Document Processing Center (with evidence & bounding-box viewer + correction workflow), Contract Repository, Invoice Management, Vendor Management, Purchase Order Matching, Payment Reconciliation, AI Agent Activity, Anomaly Detection Center, Audit Findings, Human Review Queue, Financial Reconciliation Reports (CSV/XLSX/PDF export), Model & Provider Configuration, and System Settings.

### 2. Core Data Separation Principle
Every financial record maintains strict separation of four immutable layers:
1. **Raw Extracted Facts** (`document_extractions`, `extracted_fields`) — original text, page, bounding box, extraction method, raw provider confidence (nullable), immutable forever.
2. **Deterministic Normalized Values** (`normalized_fields`, `contracts`, `invoices`, `payments`) — exact `NUMERIC(20, 6)` values, explicit currency code, normalization rule applied, source evidence ID.
3. **Deterministic Calculations & Findings** (`reconciliation_results`, `anomaly_findings`) — arithmetic computed exclusively in Rust `Decimal`, never by LLMs, with explicit rule IDs and tolerance policies.
4. **AI Interpretations & Human Decisions** (`agent_runs`, `review_actions`, `audit_logs`) — append-only review trail preserving original AI outputs alongside every human correction, approval, rejection, or escalation.

### 3. Eight-Agent Auditable Workflow
Orchestrated deterministically by the Rust workflow engine (`backend/src/agents/`) with explicit state transitions, idempotency keys, max-step guards (no infinite loops), and per-run DB audit logging (`agent_runs`):
1. `DOCUMENT_CLASSIFIER` — Classifies document type; refuses forced classification below threshold (`unknown` → human review).
2. `CONTRACT_EXTRACTOR` — Structures contract versions, effective/expiry dates, line items, tolerances, discounts, tax clauses, amendments.
3. `INVOICE_EXTRACTOR` — Structures invoice header, line items, taxes, discounts, totals, preserving exact provenance.
4. `DOCUMENT_MATCHER` — Links invoices to effective contract version (at invoice date), POs, and payments; flags ambiguous candidates for human confirmation.
5. `RECONCILIATION_AGENT` — Invokes the authoritative deterministic Rust Reconciliation Engine (A–L stages).
6. `ANOMALY_ANALYST` — Classifies deterministic anomalies vs potential anomalies (`exact_duplicate`, `probable_duplicate`, `potential_anomaly`, `confirmed_rule_violation`, `unresolved_discrepancy`), never claiming fraud automatically.
7. `AUDIT_REPORT_GENERATOR` — Compiles currency-segregated, evidence-backed audit findings and exportable reports.
8. `REVIEW_COORDINATOR` — Routes low-confidence extractions, ambiguous matches, undefined tolerances, or material discrepancies to the Human Review Queue.

### 4. Isolated Port Allocation
Verified against active Windows TCP listeners (ports 5000, 8000, 9090 are occupied on host):

| Service | Host | Default Port | Env Variable | Exposure |
|---|---|---|---|---|
| Next.js Frontend | `127.0.0.1` | `14300` | `FRONTEND_PORT` | Local UI |
| Rust Backend API (Axum) | `127.0.0.1` | `14800` | `BACKEND_PORT` | Local API |
| Python Doc-Intel Worker | `127.0.0.1` | `14900` | `DOC_INTEL_PORT` | Internal only |
| Managed PostgreSQL 16 | `127.0.0.1` | `14543` | `POSTGRES_PORT` | Internal only |
| Redis (Optional) | `127.0.0.1` | `14637` | `REDIS_PORT` | Internal only |
