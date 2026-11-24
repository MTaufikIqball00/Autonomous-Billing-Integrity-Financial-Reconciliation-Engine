# Agentic Billing Integrity & Financial Reconciliation (ABIFR)

Evidence-driven enterprise financial reconciliation platform built with **Rust (Axum + Tokio + SQLx + `rust_decimal`)**, **PostgreSQL**, a scoped **Python Document Intelligence worker (PyMuPDF + RapidOCR PP-OCRv4)**, and a **Next.js + Tailwind CSS** operations dashboard.

## Architecture Summary
- **`backend/` (Primary Backend — Rust)**: Owns authentication & RBAC (`admin`, `reviewer`, `analyst`, `auditor`), immutable SHA-256 content-addressed document storage, deterministic decimal financial normalization (`rust_decimal`), 12-stage (A–L) contract/PO/invoice/payment reconciliation engine, auditable 8-agent workflow orchestration, durable PostgreSQL job queue (+ optional Redis), and reproducible CSV/XLSX/PDF audit reports.
- **`backend/src/bin/pg_manager.rs`**: Isolated portable PostgreSQL cluster manager bound strictly to `127.0.0.1:14543` (or external PostgreSQL via `DATABASE_URL`).
- **`doc-intel/` (Internal Python Worker — No FastAPI)**: Scoped strictly to multimodal extraction (digital/scanned PDF, PNG, JPEG, TIFF, DOCX, XLSX, CSV), bounding-box provenance, RapidOCR PP-OCRv4, page PNG rendering, prompt-injection scanning, and gated external VLM provider abstraction (`local_deterministic` default, `openai`/`gemini` opt-in with PII redaction).
- **`frontend/` (Next.js + Tailwind CSS)**: 14-module enterprise operations dashboard with Document Evidence Viewer (bounding-box overlay + inline correction preserving original AI values), Reconciliation Workbench, Human Review Queue, and currency-segregated audit reports.

## Isolated Ports (Verified Conflict-Free)
See [docs/PORTS.md](docs/PORTS.md). Defaults:
- Frontend UI: `http://127.0.0.1:14300` (`FRONTEND_PORT`)
- Rust Backend API: `http://127.0.0.1:14800` (`BACKEND_PORT`)
- Python Doc-Intel: `http://127.0.0.1:14900` (`DOC_INTEL_PORT`, internal)
- Managed PostgreSQL: `127.0.0.1:14543` (`POSTGRES_PORT`, internal)
- Redis (Optional): `127.0.0.1:14637` (`REDIS_PORT`, internal)

## Quick Start (Windows PowerShell)
```powershell
# Start all services on isolated ports (auto-generates .env with random local secrets on first run)
powershell -ExecutionPolicy Bypass -File .\scripts\launch.ps1

# Or start detached in background
powershell -ExecutionPolicy Bypass -File .\scripts\launch.ps1 -Detached

# Stop all services cleanly
powershell -ExecutionPolicy Bypass -File .\scripts\stop.ps1
```

## Automated Verification Suites
```powershell
# 1. Rust normalization, reconciliation, security, storage & export tests
cd backend && cargo test --test comprehensive_tests

# 2. Python document intelligence, OCR & prompt-injection tests
.\doc-intel\.venv\Scripts\python.exe -m pytest doc-intel\tests -v

# 3. Frontend TypeScript, build & Vitest suite
cd frontend
node .\node_modules\typescript\bin\tsc --noEmit
node .\node_modules\vitest\vitest.mjs run
node .\node_modules\next\dist\bin\next build

# 4. Live end-to-end integration test against running launcher stack
.\doc-intel\.venv\Scripts\python.exe scripts\verify_e2e.py
```

## Documentation & License
- [Architecture & Technical Decisions](docs/ARCHITECTURE.md)
- [Tailwind CSS UI Audit](docs/TAILWIND_UI_AUDIT.md)
- [Port Allocation](docs/PORTS.md)
- **License**: Licensed under the [Apache License 2.0](LICENSE) (`Apache-2.0`).
