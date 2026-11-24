# Tailwind CSS UI Audit & Enterprise Design System
## Agentic Billing Integrity & Financial Reconciliation (ABIFR)

### 1. Pre-Implementation Workspace Audit
- **Existing frontend**: None (standalone greenfield workspace confirmed in Phase 1).
- **Target runtime**: Node.js `v24.15.0`, npm `12.0.1`, Next.js 15 (App Router), React 19, Tailwind CSS 3.4, TypeScript 5.
- **Data integrity rule**: Zero hardcoded financial KPIs or mock discrepancies. Every card, table, badge, chart, and report renders exclusively from live Rust backend queries (`/api/v1/*`) against PostgreSQL.

### 2. Visual System, Contrast & Accessibility Checklist
- **Palette (WCAG AA+ >= 4.5:1 text contrast)**:
  - Canvas background: `bg-slate-950` (`#020617`) / Light-surface operational tables: `bg-slate-900` (`#0f172a`) with `border-slate-800` and crisp `text-slate-100` primary text, `text-slate-300` secondary labels, `text-slate-400` metadata.
  - Primary accent: `indigo-500` / `indigo-400` for navigation active states & primary CTAs.
  - Semantic Financial Statuses (explicitly differentiated, never conflating suspicion with confirmed violation):
    - `Reconciled` / `Completed` / `Pass`: `emerald-500/15 text-emerald-300 border-emerald-500/30`
    - `Discrepancy Confirmed` / `Exact Duplicate` / `Critical` / `Failed`: `rose-500/15 text-rose-300 border-rose-500/30`
    - `Requires Review` / `Probable Duplicate` / `Potential Anomaly` / `Warning`: `amber-500/15 text-amber-300 border-amber-500/30`
    - `Processing` / `Running`: `sky-500/15 text-sky-300 border-sky-500/30`
    - `Pending` / `Unmatched`: `slate-700/60 text-slate-300 border-slate-600`
- **Typography**:
  - Primary UI font: `Inter` / system sans-serif with clear hierarchy (`text-2xl font-semibold` page headers, `text-xs uppercase tracking-wider` table headers).
  - Tabular numerals (`tabular-nums font-mono`) enforced on EVERY monetary amount, quantity, variance, percentage, SHA-256 fingerprint, and invoice/contract identifier so decimal columns align cleanly.
- **Responsive Layout**:
  - Collapsible mobile drawer sidebar (`lg:flex` fixed 260px enterprise rail on desktop; hamburger toggle on `<1024px`).
  - Horizontal scroll containers (`overflow-x-auto`) on dense financial ledgers and reconciliation matrices to prevent horizontal clipping.
- **Explicit Interactive States**:
  - Loading skeletons/spinners for every async fetch.
  - Clear empty states explaining zero records exist in PostgreSQL (with optional clearly-labeled "Load Synthetic Test Fixtures" action for reviewer/admin roles).
  - Structured error banners displaying safe API error codes without raw stack traces.
  - Explicit confirmation modals/prompts before sensitive actions (review resolution, field overrides, external AI toggle).
  - Keyboard-accessible buttons, `aria-label` attributes, `focus-visible:ring-2 focus-visible:ring-indigo-400` focus indicators.

### 3. Module Coverage Map (All 14 Enterprise Modules)
1. Executive Overview
2. Document Processing Center (+ Evidence & Bounding-Box Viewer + Inline Correction)
3. Contract Repository (+ Amendments & Undefined Tolerance Visibility)
4. Invoice Management (+ 12-Stage Deterministic Reconciliation Workbench)
5. Vendor Management
6. Purchase Order Matching
7. Payment Reconciliation (+ Allocation Modal & Unmatched Preservation)
8. AI Agent Activity (8-Agent Execution Ledger)
9. Anomaly Detection Center (Confirmed vs Potential Anomaly Distinction)
10. Audit Findings (+ Evidence Chain & Calculation Breakdown)
11. Human Review Queue (+ RBAC Reviewer Actions)
12. Financial Reconciliation Reports (+ Currency Segregation & CSV/XLSX/PDF Exports)
13. Model & Provider Configuration (+ External Egress Guard & Redaction Policy)
14. System Settings & Audit Trail (+ Severity Threshold Rules & Immutable Audit Logs)
