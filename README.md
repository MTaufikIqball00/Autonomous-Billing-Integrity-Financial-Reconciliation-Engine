# Autonomous Billing Integrity & Financial Reconciliation Engine (ABIFR)

> **Deskripsi:** Platform rekonsiliasi keuangan enterprise berbasis bukti (*evidence-driven*) yang secara otomatis menganalisis kontrak, invoice, purchase order, dan catatan pembayaran untuk mendeteksi selisih tagihan, pelanggaran kontrak, invoice ganda, kesalahan perhitungan, dan potensi kebocoran finansial — dibangun dengan **Rust (Axum)**, **PostgreSQL**, **Python (OCR PaddleOCR PP-OCRv4)**, dan **Next.js + Tailwind CSS**.

🇬🇧 English version: [README.en.md](README.en.md)

---

## ✨ Fitur Utama

- **Ingesti Dokumen Aman** — Mendukung PDF digital, PDF hasil scan, PNG, JPEG, TIFF, DOCX, XLSX, dan CSV. Validasi ekstensi, ukuran, MIME, dan *magic bytes*; fingerprint SHA-256; deteksi file duplikat; penyimpanan *content-addressed* yang tidak dapat diubah (dokumen asli selalu dipertahankan).
- **Document Intelligence Multimodal** — Ekstraksi teks + *bounding box* (PyMuPDF), OCR dokumen scan (RapidOCR / PaddleOCR PP-OCRv4), ekstraksi tabel dan *line item*. Setiap field menyimpan nilai asli, teks sumber, halaman, lokasi, metode ekstraksi, dan confidence (bernilai `null` jika tidak tersedia — **tidak pernah dikarang**).
- **Normalisasi Keuangan Deterministik** — Aritmetika desimal (`rust_decimal`, tanpa floating-point), presisi mata uang ISO 4217 (JPY=0, USD=2, KWD=3), format pemisah US/Eropa, nilai negatif akuntansi. Nilai ambigu (mis. `1,250` atau `04/05/2026`) **ditandai untuk ditinjau**, bukan dinormalisasi diam-diam.
- **Mesin Rekonsiliasi 12 Tahap (A–L)** — Pencocokan dokumen, resolusi versi kontrak/amandemen yang berlaku pada tanggal invoice, pencocokan PO, pencocokan line item, verifikasi kuantitas, harga satuan (toleransi kontrak; ditandai `UNDEFINED` bila tidak terdokumentasi), diskon, pajak, subtotal/total, biaya tambahan tidak sah, rekonsiliasi pembayaran, dan deteksi duplikat.
- **Rekonsiliasi Pembayaran** — Lunas, sebagian, multi-pembayaran, kelebihan bayar, kandidat pembayaran ganda, credit note. **Tidak pernah** mencocokkan hanya karena nominal sama.
- **Workflow 8 Agen yang Dapat Diaudit** — Document Classifier, Contract Extractor, Invoice Extractor, Document Matcher, Reconciliation Agent, Anomaly Analyst, Audit Report Generator, Review Coordinator. Semua perhitungan finansial tetap dipegang mesin deterministik Rust.
- **Human-in-the-Loop** — Approve, reject, resolve, escalate, koreksi nilai, ubah klasifikasi, konfirmasi relasi. Output AI asli tidak pernah ditimpa.
- **Laporan Audit** — Per invoice/kontrak/vendor, periodik, eksepsi, duplikat, pembayaran. Ekspor **CSV, XLSX, PDF**. Total antar mata uang **tidak pernah digabung** tanpa konversi terverifikasi.
- **Keamanan** — RBAC (`admin`, `reviewer`, `analyst`, `auditor`), Argon2, JWT + cookie HttpOnly, CSRF, rate limiting, pencegahan path traversal & SQL injection, audit log, serta **perlindungan prompt injection** pada isi dokumen.
- **Dashboard Enterprise (14 Modul)** — Executive Overview, Document Processing Center dengan Evidence Viewer (overlay bounding box), Contract Repository, Invoice Workbench, Vendor, PO, Payment, Agent Activity, Anomaly Center, Findings, Review Queue, Reports, Model Config, Settings. Semua metrik berasal dari query database nyata.

---

## 🏗️ Arsitektur

```
frontend/   Next.js 15 + React 19 + Tailwind CSS (UI)
backend/    Rust: Axum + Tokio + SQLx + rust_decimal (backend utama)
  └─ bin/pg_manager.rs   Pengelola PostgreSQL terisolasi
doc-intel/  Python worker internal (PyMuPDF + RapidOCR), tanpa FastAPI
scripts/    Launcher (launch.ps1 / launch.sh / stop.ps1) & verifikasi E2E
docs/       Arsitektur, audit UI Tailwind, alokasi port
```

Rust memegang seluruh aturan bisnis, rekonsiliasi, otorisasi, dan audit trail. Python hanya digunakan untuk OCR dan ekstraksi dokumen.

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
- Rust (cargo), Node.js 20+, Python 3.10+

### Instalasi
```powershell
# Python worker
python -m venv doc-intel\.venv
.\doc-intel\.venv\Scripts\pip install -r doc-intel\requirements.txt

# Frontend
cd frontend; npm install; node .\node_modules\next\dist\bin\next build; cd ..
```

### Menjalankan (Windows PowerShell)
```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\launch.ps1          # interaktif
powershell -ExecutionPolicy Bypass -File .\scripts\launch.ps1 -Detached # background
powershell -ExecutionPolicy Bypass -File .\scripts\stop.ps1            # hentikan
```
Saat pertama kali dijalankan, launcher membuat file `.env` dengan secret acak. Lihat `BOOTSTRAP_ADMIN_PASSWORD` di `.env` untuk login awal (`admin@abifr.local`).

Buka **http://127.0.0.1:14300**.

> Linux/macOS: salin `.env.example` ke `.env`, lalu jalankan `bash scripts/launch.sh`.

---

## 🧪 Pengujian

```powershell
cd backend; cargo test --test comprehensive_tests             # 10 tes Rust
.\doc-intel\.venv\Scripts\python.exe -m pytest doc-intel\tests # 5 tes Python
cd frontend; node .\node_modules\vitest\vitest.mjs run          # 2 tes frontend
.\doc-intel\.venv\Scripts\python.exe scripts\verify_e2e.py      # 12 cek E2E (stack harus berjalan)
```

Hasil terakhir yang dieksekusi: Rust 10/10, Python 5/5, Vitest 2/2, `tsc` & `next build` sukses, E2E live 12/12.

---

## ⚠️ Keterbatasan

- `next@15.2.4` memiliki advisory keamanan (CVE-2025-66478) — disarankan upgrade ke versi patch sebelum deployment.
- Provider AI eksternal (OpenAI/Gemini) sudah diimplementasikan namun nonaktif secara default dan belum diuji dengan API key nyata.
- Redis bersifat opsional; antrean default menggunakan PostgreSQL (`FOR UPDATE SKIP LOCKED`).
- Belum ada konversi kurs otomatis (sengaja — selisih mata uang ditandai, bukan dikonversi).
- Tidak ada eksekusi pembayaran otomatis ke sistem keuangan.
- Proyek ini adalah prototipe terverifikasi secara lokal, **belum siap produksi** tanpa TLS, manajemen secret, backup database, dan uji penetrasi.

---

## 📄 Dokumentasi & Lisensi
- [Arsitektur](docs/ARCHITECTURE.md)
- [Audit UI Tailwind](docs/TAILWIND_UI_AUDIT.md)
- [Alokasi Port](docs/PORTS.md)
- **Lisensi**: Dilindungi di bawah lisensi [Apache License 2.0](LICENSE) (`Apache-2.0`).
