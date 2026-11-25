# Autonomous Billing Integrity & Financial Reconciliation Engine (ABIFR)

> **Deskripsi:** Platform rekonsiliasi keuangan enterprise berbasis bukti (*evidence-driven*) yang secara otomatis menganalisis kontrak, invoice, purchase order, dan catatan pembayaran untuk mendeteksi selisih tagihan, pelanggaran kontrak, invoice ganda, kesalahan perhitungan, dan potensi kebocoran finansial — dibangun dengan **Rust (Axum)**, **PostgreSQL**, **Python (OCR PaddleOCR PP-OCRv4)**, dan **Next.js 15 + Tailwind CSS**.

🇬🇧 English version: [README.en.md](README.en.md)

---

## ✨ Fitur Utama

- **Ingesti Dokumen Aman & Bebas Race Condition** — Mendukung PDF digital, PDF hasil scan, PNG, JPEG, TIFF, DOCX, XLSX, dan CSV. Validasi ekstensi, ukuran, MIME, dan *magic bytes*; fingerprint SHA-256; deteksi file duplikat; penyimpanan *content-addressed* yang tidak dapat diubah (dokumen asli selalu dipertahankan), serta klaim job sinkron/asinkron PostgreSQL dengan pemulihan *stale lock*.
- **Document Intelligence Multimodal** — Ekstraksi teks + *bounding box* (PyMuPDF), OCR dokumen scan (RapidOCR / PaddleOCR PP-OCRv4), deteksi halaman **hybrid teks + scan** berdasarkan rasio cakupan gambar, batas halaman terkonfigurasi (`DOC_INTEL_MAX_PAGES`), serta ekstraksi tabel dan *line item*. Setiap field menyimpan nilai asli, teks sumber, halaman, lokasi, metode ekstraksi, dan confidence (bernilai `null` jika tidak tersedia — **tidak pernah dikarang**).
- **Document-to-Entity Materialization Bridge** — Dokumen terstruktur (`invoice`, `contract`, `purchase_order`, `payment_record`, `credit_note`, `debit_note`) dimaterialisasi ke entitas finansial secara otomatis jika seluruh field valid, atau melalui pratinjau & konfirmasi manusia pada *Evidence Viewer* bila terdapat field ambigu, lalu langsung memicu alur rekonsiliasi.
- **Normalisasi Keuangan Deterministik** — Aritmetika desimal (`rust_decimal`, nol `f32`/`f64`), presisi mata uang ISO 4217 (JPY=0, USD=2, KWD=3), format pemisah US/Eropa, nilai negatif akuntansi. Nilai ambigu (mis. `1,250` atau `04/05/2026`) **ditandai untuk ditinjau**, bukan dinormalisasi diam-diam.
- **Mesin Rekonsiliasi 12 Tahap (A–L)** — Pencocokan dokumen, resolusi versi kontrak/amandemen yang berlaku pada tanggal invoice, pencocokan PO, pencocokan line item, verifikasi kuantitas, harga satuan (toleransi kontrak; ditandai `UNDEFINED` bila tidak terdokumentasi), diskon, pajak, subtotal/total, biaya tambahan tidak sah, rekonsiliasi pembayaran, dan deteksi duplikat.
- **Rekonsiliasi Pembayaran** — Lunas, sebagian, multi-pembayaran, kelebihan bayar, kandidat pembayaran ganda, credit note. **Tidak pernah** mencocokkan hanya karena nominal sama.
- **Workflow Multi-Agen yang Dapat Diaudit** — Document Classifier, Contract Extractor, Invoice Extractor, Document Matcher, Reconciliation Agent, Anomaly Analyst, Audit Report Generator, Review Coordinator. Semua perhitungan finansial tetap dipegang mesin deterministik Rust.
- **Human-in-the-Loop** — Approve, reject, resolve, escalate, koreksi nilai, ubah klasifikasi, konfirmasi materialisasi & relasi. Output AI asli tidak pernah ditimpa.
- **Laporan Audit Aman & Multi-Halaman** — Per invoice/kontrak/vendor, periodik, eksepsi, duplikat, pembayaran. Ekspor **CSV** (dengan sanitasi *formula injection* CWE-1236), **XLSX** multi-sheet, dan **PDF** multi-halaman dengan *word wrapping* penuh dan transliterasi karakter yang aman. Total antar mata uang **tidak pernah digabung** tanpa konversi terverifikasi.
- **Keamanan Enterprise** — RBAC (`admin`, `reviewer`, `analyst`, `auditor`), Argon2, JWT pada cookie `HttpOnly` + `X-CSRF-Token` (tanpa penyimpanan token di `sessionStorage`), revokasi sesi & cek status aktif di DB (`user_sessions`), rate limiter terpisah untuk `/auth/login` dengan kapasitas terbatas & eviksi TTL, kredensial bootstrap terpisah per role, autentikasi worker `doc-intel` *fail-closed* (`hmac.compare_digest`), header `x-goog-api-key` untuk Gemini, verifikasi SHA-256 biner PostgreSQL, serta **perlindungan prompt injection** pada isi dokumen.
- **Dashboard Enterprise Modular (14 Modul + Paginasi)** — Executive Overview, Document Processing Center + Evidence Viewer, Contract & Amendment Repository, Invoice Management & Workbench, Vendor Management, PO Matching, Payment Reconciliation, AI Agent Activity, **Anomaly Center** (terpisah), **Audit Findings** (dengan filter kategori/severity/status/klasifikasi), Human Review Queue, Audit Reports, Model & Provider Config, serta Settings Admin (manajemen user, editor JSON `system_settings`, rotasi kata sandi, dan audit log) dengan *lazy per-tab fetching* dan paginasi `limit`/`offset`.

---

## 🏗️ Arsitektur

```text
frontend/                  Next.js 15.5.29 + React 19 + Tailwind CSS (UI modular di src/components/modules/)
backend/                   Rust: Axum + Tokio + SQLx + rust_decimal (backend utama)
  ├─ src/routes/           Sub-modul endpoint (auth, documents, entities, findings, admin)
  ├─ src/reconciliation/   Sub-modul mesin rekonsiliasi 12 tahap (types, matching, payments, duplicates)
  ├─ src/materialization.rs Jembatan materialisasi dokumen-ke-entitas & pemicu rekonsiliasi
  └─ src/bin/pg_manager.rs Pengelola PostgreSQL terisolasi (verifikasi SHA-256 & query aman)
doc-intel/                 Python worker internal (PyMuPDF + RapidOCR), tanpa FastAPI
scripts/                   Launcher (launch.ps1 / launch.sh / stop.ps1) & verifikasi E2E
.github/workflows/ci.yml   Pipeline CI otomatis (Rust clippy/test, Python pytest, Next.js build/test)
docs/                      Arsitektur, audit UI Tailwind, alokasi port
```

Rust memegang seluruh aturan bisnis, rekonsiliasi, otorisasi, sesi, dan audit trail. Python hanya digunakan untuk OCR dan ekstraksi dokumen.

---

## 🔌 Alokasi Port (Terisolasi)

| Layanan | Host | Port | Variabel | Eksposur |
|---|---|---|---|---|
| Frontend Next.js | 127.0.0.1 | 14300 | `FRONTEND_PORT` | UI lokal |
| Backend Rust | 127.0.0.1 | 14800 | `BACKEND_PORT` | API lokal |
| Doc-Intel Python | 127.0.0.1 | 14900 | `DOC_INTEL_PORT` | Internal |
| PostgreSQL | 127.0.0.1 | 14543 | `POSTGRES_PORT` | Internal |
| Redis (opsional) | 127.0.0.1 | 14637 | `REDIS_PORT` | Internal |

---

## 🚀 Cara Menjalankan

### Prasyarat
- Rust (`cargo` + `clippy`), Node.js 20+, Python 3.10+

### Instalasi
```powershell
# Python worker
python -m venv doc-intel\.venv
.\doc-intel\.venv\Scripts\pip install -r doc-intel\requirements.txt

# Frontend
cd frontend; npm install; npm run build; cd ..
```

### Menjalankan (Windows PowerShell)
```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\launch.ps1          # interaktif
powershell -ExecutionPolicy Bypass -File .\scripts\launch.ps1 -Detached # background
powershell -ExecutionPolicy Bypass -File .\scripts\stop.ps1            # hentikan
```
Saat pertama kali dijalankan, launcher membuat file `.env` dengan secret acak dan password bootstrap unik untuk tiap role (`BOOTSTRAP_ADMIN_PASSWORD`, `BOOTSTRAP_REVIEWER_PASSWORD`, `BOOTSTRAP_ANALYST_PASSWORD`, `BOOTSTRAP_AUDITOR_PASSWORD`). Gunakan `BOOTSTRAP_ADMIN_PASSWORD` di `.env` untuk login awal (`admin@abifr.local`), lalu rotasi melalui menu **Settings & Audit Log**.

Buka **http://127.0.0.1:14300**.

> Linux/macOS: salin `.env.example` ke `.env`, lalu jalankan `bash scripts/launch.sh`.

### 🌐 Deploy Demo Portofolio ke Vercel (Frontend Saja)
1. Buka [vercel.com/new](https://vercel.com/new) dan import repository GitHub ini.
2. Atur **Root Directory** menjadi `frontend`, lalu klik **Deploy**.
3. Pada halaman login, klik tombol **Launch Interactive Demo (Synthetic Data)** untuk mencoba seluruh 14 modul UI, *Evidence Viewer*, *Reconciliation Workbench*, filter *Audit Findings*, simulasi review, ganti role RBAC (`admin`/`reviewer`/`analyst`/`auditor`), serta ekspor CSV secara langsung di browser tanpa memerlukan backend aktif. Seluruh data demo diberi label jelas `[DEMO MODE — SYNTHETIC DATA]`.

---

## 🧪 Pengujian

```powershell
cd backend; cargo clippy --all-targets -- -D warnings
cd backend; cargo test --test comprehensive_tests                  # 13 tes unit & keamanan Rust
.\doc-intel\.venv\Scripts\python.exe -m pytest doc-intel\tests -v  # 9 tes Python doc-intel
cd frontend; npm run typecheck; npm test; npm run build            # tsc, 6 tes Vitest, build Next.js 15.5.29
.\doc-intel\.venv\Scripts\python.exe scripts\verify_e2e.py           # 14 cek E2E live (saat stack aktif)
```

> Untuk menjalankan tes integrasi HTTP terhadap PostgreSQL aktif (`backend/tests/http_integration_tests.rs`), ekspor variabel `.env` (terutama `DATABASE_URL`) lalu jalankan `cd backend; cargo test --test http_integration_tests -- --nocapture`.

Hasil verifikasi terakhir pada mesin lokal:
- `cargo clippy --all-targets -- -D warnings`: **0 warnings/errors**
- `comprehensive_tests`: **13/13 lulus**
- `http_integration_tests`: **1/1 lulus** (cookie `HttpOnly`, CSRF, revokasi `/auth/logout`, rate limit `429`, user nonaktif, RBAC 4 role, materialisasi, rekonsiliasi, review, & ekspor CSV/XLSX/PDF)
- `pytest doc-intel\tests -v`: **9/9 lulus**
- `frontend` (`tsc`, Vitest, `next build`): **6/6 lulus**, build Next.js `15.5.29` sukses
- `scripts\verify_e2e.py`: **14/14 cek live E2E lulus**

---

## ⚠️ Keterbatasan

- Provider AI eksternal (OpenAI/Gemini) sudah diimplementasikan dengan *redaction* dan header auth aman, namun nonaktif secara default (`ENABLE_EXTERNAL_AI=false`) dan belum diuji dengan API key produksi nyata.
- Redis bersifat opsional; antrean default menggunakan PostgreSQL (`FOR UPDATE SKIP LOCKED`).
- Belum ada konversi kurs otomatis (sengaja — selisih mata uang dipisahkan dan ditandai, bukan dikonversi secara diam-diam).
- Tidak ada eksekusi pembayaran otomatis ke sistem keuangan eksternal.
- Proyek ini terverifikasi secara lokal dan memiliki pipeline CI, namun sebelum deployment produksi tetap memerlukan terminasi TLS (HTTPS + `COOKIE_SECURE=true`), manajemen secret eksternal, backup database otomatis, dan uji penetrasi.

---

## 📄 Dokumentasi & Lisensi
- [Arsitektur](docs/ARCHITECTURE.md)
- [Audit UI Tailwind](docs/TAILWIND_UI_AUDIT.md)
- [Alokasi Port](docs/PORTS.md)
- **Lisensi**: Dilindungi di bawah lisensi [Apache License 2.0](LICENSE) (`Apache-2.0`).
