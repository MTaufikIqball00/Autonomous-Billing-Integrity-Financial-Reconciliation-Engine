"use client";

import React, { useState } from "react";
import {
  Upload,
  RefreshCw,
  Eye,
  Download,
  CheckCircle,
  XCircle,
  AlertTriangle,
  ShieldAlert,
  Sparkles,
  FileSpreadsheet,
  FileText,
} from "lucide-react";
import { api, formatMoney, UserSession } from "@/lib/api";
import { StatusBadge } from "./StatusBadge";

interface ModuleProps {
  activeTab: string;
  user: UserSession;
  overview: any;
  documents: any[];
  vendors: any[];
  contracts: any[];
  purchaseOrders: any[];
  invoices: any[];
  payments: any[];
  findings: any[];
  reviewQueue: any;
  agentRuns: any[];
  reports: any[];
  models: any[];
  settings: any[];
  auditLogs: any[];
  externalAiEnv: boolean;
  onRefresh: () => Promise<void>;
  onOpenDocument: (docId: string) => void;
  onOpenInvoice: (invId: string) => void;
  setBanner: (b: { type: "ok" | "err"; text: string } | null) => void;
}

export function DashboardModules(props: ModuleProps) {
  const {
    activeTab,
    user,
    overview,
    documents,
    vendors,
    contracts,
    purchaseOrders,
    invoices,
    payments,
    findings,
    reviewQueue,
    agentRuns,
    reports,
    models,
    settings,
    auditLogs,
    externalAiEnv,
    onRefresh,
    onOpenDocument,
    onOpenInvoice,
    setBanner,
  } = props;

  const canReview = user.role === "admin" || user.role === "reviewer";
  const canWrite = canReview || user.role === "analyst";
  const isAdmin = user.role === "admin";

  // Upload state
  const [uploadPct, setUploadPct] = useState<number | null>(null);
  const [uploading, setUploading] = useState(false);

  // Finding review state
  const [reviewNotes, setReviewNotes] = useState<Record<string, string>>({});
  const [selectedFinding, setSelectedFinding] = useState<any>(null);

  // Report generation state
  const [reportType, setReportType] = useState("periodic");
  const [reportTitle, setReportTitle] = useState("");

  // Vendor quick-create state
  const [vCode, setVCode] = useState("");
  const [vName, setVName] = useState("");
  const [vCurrency, setVCurrency] = useState("USD");

  // Payment allocation modal state
  const [allocPayId, setAllocPayId] = useState<string | null>(null);
  const [allocInvId, setAllocInvId] = useState("");
  const [allocAmount, setAllocAmount] = useState("");
  const [allocEvidence, setAllocEvidence] = useState("");

  // Contract detail modal
  const [contractDetail, setContractDetail] = useState<any>(null);

  const handleUpload = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    setUploading(true);
    setUploadPct(0);
    setBanner(null);
    try {
      const res = await api.uploadDocument(file, (p) => setUploadPct(p));
      setBanner({
        type: "ok",
        text: `Uploaded '${res.original_filename}' (SHA-256: ${res.sha256_hash.slice(0, 12)}…)${
          res.is_duplicate_file ? " — DUPLICATE FILE FINGERPRINT DETECTED" : ""
        }`,
      });
      await onRefresh();
    } catch (err: any) {
      setBanner({ type: "err", text: err.message });
    } finally {
      setUploading(false);
      setUploadPct(null);
      e.target.value = "";
    }
  };

  const handleLoadSyntheticFixtures = async () => {
    if (!confirm("Load clearly labeled [SYNTHETIC TEST FIXTURE] records into PostgreSQL?")) return;
    try {
      const res = await api.request("/fixtures/load-synthetic", { method: "POST" });
      setBanner({
        type: "ok",
        text: `Synthetic fixtures loaded (${res.label}): ${res.invoices_reconciled} invoices reconciled.`,
      });
      await onRefresh();
    } catch (err: any) {
      setBanner({ type: "err", text: err.message });
    }
  };

  const handleFindingReview = async (findingId: string, action: string) => {
    const notes = (reviewNotes[findingId] || "").trim();
    if (!notes) {
      setBanner({ type: "err", text: "Enter an audit explanation note before submitting review action." });
      return;
    }
    if (!confirm(`Confirm reviewer action '${action.toUpperCase()}' on finding?`)) return;
    try {
      await api.request(`/findings/${findingId}/review`, {
        method: "POST",
        body: JSON.stringify({ action, notes }),
      });
      setBanner({ type: "ok", text: `Finding updated with action '${action}'.` });
      setReviewNotes((m) => ({ ...m, [findingId]: "" }));
      await onRefresh();
    } catch (err: any) {
      setBanner({ type: "err", text: err.message });
    }
  };

  const handleInspectFinding = async (findingId: string) => {
    try {
      const detail = await api.request(`/findings/${findingId}`);
      setSelectedFinding(detail);
    } catch (err: any) {
      setBanner({ type: "err", text: err.message });
    }
  };

  const handleCreateVendor = async (e: React.FormEvent) => {
    e.preventDefault();
    try {
      await api.request("/vendors", {
        method: "POST",
        body: JSON.stringify({
          vendor_code: vCode,
          legal_name: vName,
          default_currency: vCurrency,
        }),
      });
      setVCode("");
      setVName("");
      setBanner({ type: "ok", text: "Vendor created." });
      await onRefresh();
    } catch (err: any) {
      setBanner({ type: "err", text: err.message });
    }
  };

  const handleGenerateReport = async () => {
    try {
      await api.request("/reports/generate", {
        method: "POST",
        body: JSON.stringify({
          report_type: reportType,
          title: reportTitle.trim() || undefined,
        }),
      });
      setReportTitle("");
      setBanner({ type: "ok", text: "Structured audit report generated and persisted." });
      await onRefresh();
    } catch (err: any) {
      setBanner({ type: "err", text: err.message });
    }
  };

  const handleAllocatePayment = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!allocPayId) return;
    try {
      await api.request(`/payments/${allocPayId}/allocate`, {
        method: "POST",
        body: JSON.stringify({
          invoice_id: allocInvId,
          allocated_amount: allocAmount,
          matching_evidence: allocEvidence,
        }),
      });
      setAllocPayId(null);
      setAllocInvId("");
      setAllocAmount("");
      setAllocEvidence("");
      setBanner({ type: "ok", text: "Payment allocated with explicit evidence." });
      await onRefresh();
    } catch (err: any) {
      setBanner({ type: "err", text: err.message });
    }
  };

  // =========================================================================
  // 1. EXECUTIVE OVERVIEW
  // =========================================================================
  if (activeTab === "overview") {
    return (
      <div className="space-y-6">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <h2 className="text-xl font-semibold text-slate-100">Executive Overview</h2>
            <p className="text-xs text-slate-400">
              Live metrics queried directly from PostgreSQL. Zero fabricated financial values.
            </p>
          </div>
          {canReview && (
            <button
              onClick={handleLoadSyntheticFixtures}
              className="inline-flex items-center gap-1.5 rounded-lg border border-indigo-500/40 bg-indigo-500/15 px-3 py-1.5 text-xs font-medium text-indigo-200 hover:bg-indigo-500/25"
            >
              <Sparkles className="h-3.5 w-3.5" /> Load Synthetic Test Fixtures
            </button>
          )}
        </div>

        {/* KPI Grid */}
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
          <KpiCard
            label="Documents Ingested"
            value={overview?.documents?.total ?? 0}
            sub={`${overview?.documents?.requires_review ?? 0} require review • ${overview?.documents?.duplicate_files ?? 0} duplicate files`}
          />
          <KpiCard
            label="Invoices Tracked"
            value={overview?.invoices?.total ?? 0}
            sub={`${overview?.invoices?.reconciled ?? 0} reconciled • ${overview?.invoices?.discrepancy_confirmed ?? 0} discrepancy confirmed`}
          />
          <KpiCard
            label="Confirmed Violations"
            value={overview?.findings?.confirmed ?? 0}
            accent="text-rose-300"
            sub={`${overview?.findings?.potential ?? 0} potential anomalies • ${overview?.findings?.unresolved ?? 0} unresolved`}
          />
          <KpiCard
            label="Pending Human Review"
            value={overview?.findings?.pending_review ?? 0}
            accent="text-amber-300"
            sub={`${overview?.counts?.unmatched_payments ?? 0} unmatched payments • ${overview?.documents?.prompt_injection_flagged ?? 0} injection flags`}
          />
        </div>

        {/* Currency-Segregated Financial Variance (NEVER mixed across currencies!) */}
        <div className="rounded-xl border border-slate-800 bg-slate-900 p-5">
          <div className="mb-3 flex items-center justify-between">
            <div>
              <h3 className="text-sm font-semibold text-slate-100">
                Financial Variance Breakdown by Currency
              </h3>
              <p className="text-xs text-slate-400">
                Amounts in different currencies are strictly segregated (no unverified FX conversion).
              </p>
            </div>
          </div>
          {!overview?.currency_breakdown?.length ? (
            <EmptyBox text="No currency variance records in database yet. Upload documents or load clearly labeled synthetic test fixtures." />
          ) : (
            <div className="grid grid-cols-1 gap-3 md:grid-cols-3">
              {overview.currency_breakdown.map((b: any) => (
                <div
                  key={b.currency}
                  className="rounded-lg border border-slate-800 bg-slate-950 p-4"
                >
                  <div className="flex items-center justify-between">
                    <span className="rounded bg-indigo-500/20 px-2 py-0.5 font-mono text-xs font-bold text-indigo-300">
                      {b.currency}
                    </span>
                    <span className="text-xs text-slate-400">
                      {b.confirmed_count} confirmed / {b.potential_count} potential
                    </span>
                  </div>
                  <div className="mt-3 space-y-1">
                    <div className="flex justify-between text-xs">
                      <span className="text-slate-400">Confirmed Variance:</span>
                      <span className="font-mono font-semibold tabular-nums text-rose-300">
                        {formatMoney(b.confirmed_variance, b.currency)}
                      </span>
                    </div>
                    <div className="flex justify-between text-xs">
                      <span className="text-slate-400">Potential / Unresolved:</span>
                      <span className="font-mono tabular-nums text-amber-300">
                        {formatMoney(b.potential_variance, b.currency)}
                      </span>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Recent Findings Preview */}
        <div className="rounded-xl border border-slate-800 bg-slate-900 overflow-hidden">
          <div className="border-b border-slate-800 px-5 py-3 text-sm font-semibold text-slate-100">
            Recent Evidence-Backed Findings
          </div>
          {findings.length === 0 ? (
            <div className="p-6">
              <EmptyBox text="No audit findings recorded yet." />
            </div>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-left text-xs">
                <thead className="bg-slate-950 text-slate-400">
                  <tr>
                    <th className="px-4 py-2.5">Category</th>
                    <th className="px-4 py-2.5">Classification</th>
                    <th className="px-4 py-2.5">Severity</th>
                    <th className="px-4 py-2.5">Invoice</th>
                    <th className="px-4 py-2.5 text-right">Variance</th>
                    <th className="px-4 py-2.5">Rule Applied</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-800/80">
                  {findings.slice(0, 8).map((f) => (
                    <tr key={f.id} className="hover:bg-slate-800/40">
                      <td className="px-4 py-2.5 font-mono text-slate-200">{f.category}</td>
                      <td className="px-4 py-2.5">
                        <StatusBadge value={f.classification_type} />
                      </td>
                      <td className="px-4 py-2.5">
                        <StatusBadge value={f.severity} />
                      </td>
                      <td className="px-4 py-2.5 font-mono text-slate-300">
                        {f.invoice_number || "Document-level"}
                      </td>
                      <td className="px-4 py-2.5 text-right font-mono tabular-nums text-rose-300">
                        {formatMoney(f.variance_amount, f.currency)}
                      </td>
                      <td className="px-4 py-2.5 font-mono text-[11px] text-slate-400">
                        {f.rule_applied}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      </div>
    );
  }

  // =========================================================================
  // 2. DOCUMENT PROCESSING CENTER
  // =========================================================================
  if (activeTab === "documents") {
    return (
      <div className="space-y-5">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <h2 className="text-xl font-semibold text-slate-100">Document Processing Center</h2>
            <p className="text-xs text-slate-400">
              Verified formats: PDF (digital & scanned OCR), PNG, JPEG, TIFF, DOCX, XLSX, CSV. Original bytes preserved immutably with SHA-256 fingerprinting.
            </p>
          </div>
          {canWrite && (
            <label className="inline-flex cursor-pointer items-center gap-2 rounded-lg bg-indigo-600 px-3.5 py-2 text-xs font-medium text-white hover:bg-indigo-500">
              <Upload className="h-4 w-4" />
              {uploading ? `Uploading ${uploadPct ?? 0}%...` : "Upload Financial Document"}
              <input
                type="file"
                accept=".pdf,.png,.jpg,.jpeg,.tif,.tiff,.docx,.xlsx,.csv"
                onChange={handleUpload}
                disabled={uploading}
                className="hidden"
              />
            </label>
          )}
        </div>

        {uploadPct !== null && (
          <div className="h-2 w-full overflow-hidden rounded-full bg-slate-800">
            <div
              className="h-full bg-indigo-500 transition-all"
              style={{ width: `${uploadPct}%` }}
            />
          </div>
        )}

        <div className="rounded-xl border border-slate-800 bg-slate-900 overflow-hidden">
          {documents.length === 0 ? (
            <div className="p-8">
              <EmptyBox text="No documents uploaded yet. Upload a PDF, image, DOCX, XLSX, or CSV document to trigger the OCR & extraction pipeline." />
            </div>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-left text-xs">
                <thead className="bg-slate-950 text-slate-400">
                  <tr>
                    <th className="px-4 py-2.5">Document</th>
                    <th className="px-4 py-2.5">Category</th>
                    <th className="px-4 py-2.5">Processing</th>
                    <th className="px-4 py-2.5">OCR / Extract</th>
                    <th className="px-4 py-2.5">Flags</th>
                    <th className="px-4 py-2.5">Actions</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-800/80">
                  {documents.map((d) => (
                    <tr key={d.id} className="hover:bg-slate-800/40">
                      <td className="px-4 py-2.5">
                        <div className="font-medium text-slate-100">
                          {d.original_filename}
                          {d.is_synthetic_fixture && (
                            <span className="ml-2 rounded bg-sky-500/20 px-1.5 py-0.5 font-mono text-[10px] text-sky-300">
                              SYNTHETIC FIXTURE
                            </span>
                          )}
                        </div>
                        <div className="font-mono text-[11px] text-slate-500">
                          SHA-256: {d.sha256_hash.slice(0, 16)}… • {(d.file_size_bytes / 1024).toFixed(1)} KB • {d.page_count ?? "?"} pg
                        </div>
                        {d.failure_reason && (
                          <div className="mt-1 text-[11px] text-rose-400">Error: {d.failure_reason}</div>
                        )}
                      </td>
                      <td className="px-4 py-2.5">
                        <StatusBadge value={d.effective_category} />
                      </td>
                      <td className="px-4 py-2.5">
                        <StatusBadge value={d.processing_status} />
                      </td>
                      <td className="px-4 py-2.5">
                        <div className="flex flex-wrap gap-1">
                          <StatusBadge value={d.ocr_status} />
                          <StatusBadge value={d.extraction_status} />
                        </div>
                      </td>
                      <td className="px-4 py-2.5 space-y-1">
                        {d.is_duplicate_file && (
                          <span className="inline-flex items-center gap-1 rounded bg-rose-500/15 px-1.5 py-0.5 text-[11px] text-rose-300">
                            <AlertTriangle className="h-3 w-3" /> Duplicate SHA-256
                          </span>
                        )}
                        {d.prompt_injection_flagged && (
                          <span className="inline-flex items-center gap-1 rounded bg-amber-500/20 px-1.5 py-0.5 text-[11px] text-amber-300">
                            <ShieldAlert className="h-3 w-3" /> Prompt Injection Blocked
                          </span>
                        )}
                      </td>
                      <td className="px-4 py-2.5">
                        <div className="flex items-center gap-2">
                          <button
                            onClick={() => onOpenDocument(d.id)}
                            className="inline-flex items-center gap-1 rounded border border-indigo-500/40 bg-indigo-500/15 px-2.5 py-1 text-xs text-indigo-200 hover:bg-indigo-500/25"
                          >
                            <Eye className="h-3.5 w-3.5" /> Evidence Viewer
                          </button>
                          {canWrite && (
                            <button
                              onClick={async () => {
                                try {
                                  await api.request(`/documents/${d.id}/reprocess`, { method: "POST" });
                                  setBanner({ type: "ok", text: "Document reprocessed." });
                                  await onRefresh();
                                } catch (e: any) {
                                  setBanner({ type: "err", text: e.message });
                                }
                              }}
                              aria-label="Reprocess document"
                              className="rounded border border-slate-700 p-1 text-slate-400 hover:bg-slate-800 hover:text-white"
                            >
                              <RefreshCw className="h-3.5 w-3.5" />
                            </button>
                          )}
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      </div>
    );
  }

  // =========================================================================
  // 3. CONTRACT REPOSITORY
  // =========================================================================
  if (activeTab === "contracts") {
    return (
      <div className="space-y-5">
        <div>
          <h2 className="text-xl font-semibold text-slate-100">Contract Repository</h2>
          <p className="text-xs text-slate-400">
            Tracks effective contract versions, amendments, pricing schedules, and explicit tolerance policies (marked UNDEFINED when undocumented).
          </p>
        </div>

        <div className="rounded-xl border border-slate-800 bg-slate-900 overflow-hidden">
          {contracts.length === 0 ? (
            <div className="p-8"><EmptyBox text="No contracts registered yet." /></div>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-left text-xs">
                <thead className="bg-slate-950 text-slate-400">
                  <tr>
                    <th className="px-4 py-2.5">Contract #</th>
                    <th className="px-4 py-2.5">Title & Vendor</th>
                    <th className="px-4 py-2.5">Currency</th>
                    <th className="px-4 py-2.5">Versions</th>
                    <th className="px-4 py-2.5">Latest Expiry</th>
                    <th className="px-4 py-2.5">Status</th>
                    <th className="px-4 py-2.5">Inspect</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-800/80">
                  {contracts.map((c) => (
                    <tr key={c.id} className="hover:bg-slate-800/40">
                      <td className="px-4 py-2.5 font-mono text-indigo-300">{c.contract_number}</td>
                      <td className="px-4 py-2.5">
                        <div className="font-medium text-slate-100">{c.title}</div>
                        <div className="text-slate-400">{c.vendor_name}</div>
                      </td>
                      <td className="px-4 py-2.5 font-mono">{c.currency}</td>
                      <td className="px-4 py-2.5 font-mono">{c.version_count}</td>
                      <td className="px-4 py-2.5 font-mono">{c.latest_expiration || "—"}</td>
                      <td className="px-4 py-2.5"><StatusBadge value={c.status} /></td>
                      <td className="px-4 py-2.5">
                        <button
                          onClick={async () => {
                            const d = await api.request(`/contracts/${c.id}`);
                            setContractDetail(d);
                          }}
                          className="rounded border border-slate-700 px-2.5 py-1 text-xs text-slate-200 hover:bg-slate-800"
                        >
                          Versions & Terms
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>

        {contractDetail && (
          <div className="rounded-xl border border-indigo-500/40 bg-slate-900 p-5 space-y-4">
            <div className="flex items-center justify-between">
              <div>
                <h3 className="text-sm font-semibold text-slate-100">
                  {contractDetail.contract.contract_number} — {contractDetail.contract.title}
                </h3>
                <p className="text-xs text-slate-400">{contractDetail.contract.vendor_name}</p>
              </div>
              <button
                onClick={() => setContractDetail(null)}
                className="text-xs text-slate-400 hover:text-white"
              >
                Close
              </button>
            </div>
            {contractDetail.versions.map((v: any) => (
              <div key={v.id} className="rounded-lg border border-slate-800 bg-slate-950 p-4 space-y-2">
                <div className="flex flex-wrap items-center gap-2 text-xs">
                  <span className="rounded bg-indigo-500/20 px-2 py-0.5 font-mono font-semibold text-indigo-300">
                    {v.version_label}
                  </span>
                  {v.is_amendment && <StatusBadge value="contract_amendment" />}
                  <span className="font-mono text-slate-300">
                    Effective: {v.effective_date} → {v.expiration_date}
                  </span>
                  <span className="font-mono text-amber-300">
                    Price Tolerance: {v.price_tolerance_pct ? `${v.price_tolerance_pct}%` : v.price_tolerance_abs ?? "UNDEFINED"}
                  </span>
                  <span className="font-mono text-slate-400">
                    Tax Rule: {v.default_tax_rate ? `${v.default_tax_rate}%` : "Not Documented"}
                  </span>
                </div>
                <table className="w-full text-left text-xs mt-2">
                  <thead className="text-slate-500">
                    <tr>
                      <th className="py-1">SKU</th>
                      <th className="py-1">Description</th>
                      <th className="py-1 text-right">Agreed Price</th>
                      <th className="py-1 text-right">Discount %</th>
                      <th className="py-1 text-right">Tax %</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-slate-800/60">
                    {v.line_items.map((li: any) => (
                      <tr key={li.id}>
                        <td className="py-1 font-mono text-slate-400">{li.item_code || "—"}</td>
                        <td className="py-1 text-slate-200">{li.description}</td>
                        <td className="py-1 text-right font-mono tabular-nums text-emerald-300">
                          {formatMoney(li.unit_price, li.currency)} / {li.unit}
                        </td>
                        <td className="py-1 text-right font-mono tabular-nums">{li.discount_pct ?? 0}%</td>
                        <td className="py-1 text-right font-mono tabular-nums">{li.tax_rate ?? "—"}%</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            ))}
          </div>
        )}
      </div>
    );
  }

  // =========================================================================
  // 4. INVOICE MANAGEMENT & RECONCILIATION
  // =========================================================================
  if (activeTab === "invoices") {
    return (
      <div className="space-y-5">
        <div>
          <h2 className="text-xl font-semibold text-slate-100">Invoice Management & Reconciliation</h2>
          <p className="text-xs text-slate-400">
            Click &ldquo;Reconcile Workbench&rdquo; on any invoice to inspect Contract vs Invoice values, Stage A–L deterministic checks, and supporting evidence.
          </p>
        </div>
        <div className="rounded-xl border border-slate-800 bg-slate-900 overflow-hidden">
          {invoices.length === 0 ? (
            <div className="p-8"><EmptyBox text="No invoices recorded yet." /></div>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-left text-xs">
                <thead className="bg-slate-950 text-slate-400">
                  <tr>
                    <th className="px-4 py-2.5">Invoice #</th>
                    <th className="px-4 py-2.5">Vendor</th>
                    <th className="px-4 py-2.5">Date</th>
                    <th className="px-4 py-2.5 text-right">Stated Total</th>
                    <th className="px-4 py-2.5 text-right">Recalculated</th>
                    <th className="px-4 py-2.5">Status</th>
                    <th className="px-4 py-2.5">Findings</th>
                    <th className="px-4 py-2.5">Action</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-800/80">
                  {invoices.map((inv) => (
                    <tr key={inv.id} className="hover:bg-slate-800/40">
                      <td className="px-4 py-2.5 font-mono text-indigo-300">{inv.invoice_number}</td>
                      <td className="px-4 py-2.5 text-slate-200">{inv.vendor_name}</td>
                      <td className="px-4 py-2.5 font-mono text-slate-400">{inv.invoice_date}</td>
                      <td className="px-4 py-2.5 text-right font-mono tabular-nums text-slate-100">
                        {formatMoney(inv.stated_total, inv.currency)}
                      </td>
                      <td className="px-4 py-2.5 text-right font-mono tabular-nums text-emerald-300">
                        {formatMoney(inv.recalculated_total, inv.currency)}
                      </td>
                      <td className="px-4 py-2.5"><StatusBadge value={inv.reconciliation_status} /></td>
                      <td className="px-4 py-2.5 font-mono">{inv.findings_count}</td>
                      <td className="px-4 py-2.5">
                        <button
                          onClick={() => onOpenInvoice(inv.id)}
                          className="rounded border border-indigo-500/40 bg-indigo-500/15 px-2.5 py-1 text-xs text-indigo-200 hover:bg-indigo-500/25"
                        >
                          Reconcile Workbench
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      </div>
    );
  }

  // =========================================================================
  // 5. VENDOR MANAGEMENT
  // =========================================================================
  if (activeTab === "vendors") {
    return (
      <div className="space-y-5">
        <h2 className="text-xl font-semibold text-slate-100">Vendor Management</h2>
        {canWrite && (
          <form onSubmit={handleCreateVendor} className="flex flex-wrap items-end gap-3 rounded-xl border border-slate-800 bg-slate-900 p-4">
            <div>
              <label className="block text-[11px] text-slate-400">Vendor Code</label>
              <input required value={vCode} onChange={(e) => setVCode(e.target.value)} placeholder="VEND-100" className="mt-1 rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 text-xs text-slate-100" />
            </div>
            <div className="flex-1 min-w-[200px]">
              <label className="block text-[11px] text-slate-400">Legal Name</label>
              <input required value={vName} onChange={(e) => setVName(e.target.value)} placeholder="Acme Enterprise Services Ltd." className="mt-1 w-full rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 text-xs text-slate-100" />
            </div>
            <div>
              <label className="block text-[11px] text-slate-400">Currency</label>
              <input value={vCurrency} onChange={(e) => setVCurrency(e.target.value)} className="mt-1 w-20 rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
            </div>
            <button type="submit" className="rounded-lg bg-indigo-600 px-3.5 py-1.5 text-xs font-medium text-white hover:bg-indigo-500">
              Add Vendor
            </button>
          </form>
        )}
        <div className="rounded-xl border border-slate-800 bg-slate-900 overflow-hidden">
          {vendors.length === 0 ? (
            <div className="p-8"><EmptyBox text="No vendors registered yet." /></div>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-left text-xs">
                <thead className="bg-slate-950 text-slate-400">
                  <tr>
                    <th className="px-4 py-2.5">Code</th>
                    <th className="px-4 py-2.5">Legal Name</th>
                    <th className="px-4 py-2.5">Tax ID</th>
                    <th className="px-4 py-2.5">Currency</th>
                    <th className="px-4 py-2.5">Contracts</th>
                    <th className="px-4 py-2.5">Invoices</th>
                    <th className="px-4 py-2.5">Open Findings</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-800/80">
                  {vendors.map((v) => (
                    <tr key={v.id}>
                      <td className="px-4 py-2.5 font-mono text-indigo-300">{v.vendor_code}</td>
                      <td className="px-4 py-2.5 font-medium text-slate-100">{v.legal_name}</td>
                      <td className="px-4 py-2.5 font-mono text-slate-400">{v.tax_id || "—"}</td>
                      <td className="px-4 py-2.5 font-mono">{v.default_currency || "—"}</td>
                      <td className="px-4 py-2.5 font-mono">{v.contract_count}</td>
                      <td className="px-4 py-2.5 font-mono">{v.invoice_count}</td>
                      <td className="px-4 py-2.5 font-mono text-amber-300">{v.open_findings}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      </div>
    );
  }

  // =========================================================================
  // 6. PURCHASE ORDER MATCHING
  // =========================================================================
  if (activeTab === "purchase_orders") {
    return (
      <div className="space-y-5">
        <h2 className="text-xl font-semibold text-slate-100">Purchase Order Matching</h2>
        <div className="rounded-xl border border-slate-800 bg-slate-900 overflow-hidden">
          {purchaseOrders.length === 0 ? (
            <div className="p-8"><EmptyBox text="No purchase orders recorded yet." /></div>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-left text-xs">
                <thead className="bg-slate-950 text-slate-400">
                  <tr>
                    <th className="px-4 py-2.5">PO #</th>
                    <th className="px-4 py-2.5">Vendor</th>
                    <th className="px-4 py-2.5">Contract #</th>
                    <th className="px-4 py-2.5">Issue Date</th>
                    <th className="px-4 py-2.5 text-right">Total</th>
                    <th className="px-4 py-2.5">Lines</th>
                    <th className="px-4 py-2.5">Status</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-800/80">
                  {purchaseOrders.map((po) => (
                    <tr key={po.id}>
                      <td className="px-4 py-2.5 font-mono text-indigo-300">{po.po_number}</td>
                      <td className="px-4 py-2.5 text-slate-200">{po.vendor_name}</td>
                      <td className="px-4 py-2.5 font-mono text-slate-400">{po.contract_number || "—"}</td>
                      <td className="px-4 py-2.5 font-mono text-slate-400">{po.issue_date}</td>
                      <td className="px-4 py-2.5 text-right font-mono tabular-nums">
                        {formatMoney(po.total_amount, po.currency)}
                      </td>
                      <td className="px-4 py-2.5 font-mono">{po.line_count}</td>
                      <td className="px-4 py-2.5"><StatusBadge value={po.status} /></td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      </div>
    );
  }

  // =========================================================================
  // 7. PAYMENT RECONCILIATION
  // =========================================================================
  if (activeTab === "payments") {
    return (
      <div className="space-y-5">
        <div>
          <h2 className="text-xl font-semibold text-slate-100">Payment Reconciliation</h2>
          <p className="text-xs text-slate-400">
            Requires explicit matching evidence (invoice reference, vendor, currency). Never matches solely on equal amounts; unmatched payments remain visible for manual investigation.
          </p>
        </div>

        <div className="rounded-xl border border-slate-800 bg-slate-900 overflow-hidden">
          {payments.length === 0 ? (
            <div className="p-8"><EmptyBox text="No payment records loaded yet." /></div>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-left text-xs">
                <thead className="bg-slate-950 text-slate-400">
                  <tr>
                    <th className="px-4 py-2.5">Payment Ref / Bank Ref</th>
                    <th className="px-4 py-2.5">Vendor</th>
                    <th className="px-4 py-2.5">Date</th>
                    <th className="px-4 py-2.5">Referenced Invoice</th>
                    <th className="px-4 py-2.5 text-right">Amount</th>
                    <th className="px-4 py-2.5 text-right">Allocated</th>
                    <th className="px-4 py-2.5">Allocation Status</th>
                    {canReview && <th className="px-4 py-2.5">Action</th>}
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-800/80">
                  {payments.map((p) => (
                    <tr key={p.id}>
                      <td className="px-4 py-2.5">
                        <div className="font-mono text-indigo-300">{p.payment_reference}</div>
                        <div className="font-mono text-[11px] text-slate-500">{p.bank_reference || "—"}</div>
                      </td>
                      <td className="px-4 py-2.5 text-slate-200">{p.vendor_name}</td>
                      <td className="px-4 py-2.5 font-mono text-slate-400">{p.payment_date}</td>
                      <td className="px-4 py-2.5 font-mono text-slate-300">
                        {p.referenced_invoice_number || <span className="text-amber-400">Unreferenced</span>}
                      </td>
                      <td className="px-4 py-2.5 text-right font-mono tabular-nums text-slate-100">
                        {formatMoney(p.amount, p.currency)}
                      </td>
                      <td className="px-4 py-2.5 text-right font-mono tabular-nums text-emerald-300">
                        {formatMoney(p.allocated_total, p.currency)}
                      </td>
                      <td className="px-4 py-2.5"><StatusBadge value={p.allocation_status} /></td>
                      {canReview && (
                        <td className="px-4 py-2.5">
                          <button
                            onClick={() => {
                              setAllocPayId(p.id);
                              setAllocAmount(String(p.amount));
                            }}
                            className="rounded border border-slate-700 px-2 py-1 text-xs text-slate-200 hover:bg-slate-800"
                          >
                            Allocate
                          </button>
                        </td>
                      )}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>

        {allocPayId && (
          <form onSubmit={handleAllocatePayment} className="rounded-xl border border-indigo-500/40 bg-slate-900 p-4 space-y-3">
            <h3 className="text-sm font-semibold text-slate-100">Allocate Payment with Explicit Evidence</h3>
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
              <select required value={allocInvId} onChange={(e) => setAllocInvId(e.target.value)} className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 text-xs text-slate-100">
                <option value="">Select Invoice...</option>
                {invoices.map((inv) => (
                  <option key={inv.id} value={inv.id}>
                    {inv.invoice_number} ({formatMoney(inv.stated_total, inv.currency)})
                  </option>
                ))}
              </select>
              <input required value={allocAmount} onChange={(e) => setAllocAmount(e.target.value)} placeholder="Allocated amount" className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
              <input required value={allocEvidence} onChange={(e) => setAllocEvidence(e.target.value)} placeholder="Explicit matching evidence (e.g. bank remittance #)..." className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 text-xs text-slate-100" />
            </div>
            <div className="flex gap-2">
              <button type="submit" className="rounded bg-indigo-600 px-3 py-1.5 text-xs text-white hover:bg-indigo-500">Confirm Allocation</button>
              <button type="button" onClick={() => setAllocPayId(null)} className="rounded border border-slate-700 px-3 py-1.5 text-xs text-slate-400">Cancel</button>
            </div>
          </form>
        )}
      </div>
    );
  }

  // =========================================================================
  // 8. AI AGENT ACTIVITY
  // =========================================================================
  if (activeTab === "agents") {
    return (
      <div className="space-y-5">
        <div>
          <h2 className="text-xl font-semibold text-slate-100">AI Agent Activity Ledger</h2>
          <p className="text-xs text-slate-400">
            Auditable execution trace across all 8 workflow agents. Arithmetic calculations remain strictly owned by the deterministic Rust reconciliation engine.
          </p>
        </div>
        <div className="rounded-xl border border-slate-800 bg-slate-900 overflow-hidden">
          {agentRuns.length === 0 ? (
            <div className="p-8"><EmptyBox text="No agent executions recorded yet." /></div>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-left text-xs">
                <thead className="bg-slate-950 text-slate-400">
                  <tr>
                    <th className="px-4 py-2.5">Step / Agent</th>
                    <th className="px-4 py-2.5">Status</th>
                    <th className="px-4 py-2.5">Model / Rule Version</th>
                    <th className="px-4 py-2.5 text-right">Duration</th>
                    <th className="px-4 py-2.5">Output Summary</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-800/80">
                  {agentRuns.map((a) => (
                    <tr key={a.id}>
                      <td className="px-4 py-2.5 font-mono text-indigo-300">
                        #{a.step_number} {a.agent_name}
                      </td>
                      <td className="px-4 py-2.5"><StatusBadge value={a.status} /></td>
                      <td className="px-4 py-2.5 font-mono text-[11px] text-slate-400">{a.model_or_rule_version}</td>
                      <td className="px-4 py-2.5 text-right font-mono tabular-nums text-slate-400">{a.duration_ms ?? 0} ms</td>
                      <td className="px-4 py-2.5 font-mono text-[11px] text-slate-300 max-w-md truncate">
                        {JSON.stringify(a.output_payload)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      </div>
    );
  }

  // =========================================================================
  // 9 & 10. ANOMALY DETECTION CENTER & AUDIT FINDINGS
  // =========================================================================
  if (activeTab === "anomalies" || activeTab === "findings") {
    const confirmedList = findings.filter(
      (f) => f.classification_type === "confirmed_rule_violation" || f.classification_type === "exact_duplicate"
    );
    const potentialList = findings.filter(
      (f) => f.classification_type !== "confirmed_rule_violation" && f.classification_type !== "exact_duplicate"
    );
    const displayList = activeTab === "anomalies" ? findings : findings;

    return (
      <div className="space-y-5">
        <div>
          <h2 className="text-xl font-semibold text-slate-100">
            {activeTab === "anomalies" ? "Anomaly Detection Center" : "Evidence-Backed Audit Findings"}
          </h2>
          <p className="text-xs text-slate-400">
            Explicit separation: {confirmedList.length} confirmed deterministic violations vs {potentialList.length} potential/unresolved anomalies. An AI or statistical suspicion is never proof of fraud.
          </p>
        </div>

        <div className="rounded-xl border border-slate-800 bg-slate-900 overflow-hidden">
          {displayList.length === 0 ? (
            <div className="p-8"><EmptyBox text="No findings detected." /></div>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-left text-xs">
                <thead className="bg-slate-950 text-slate-400">
                  <tr>
                    <th className="px-4 py-2.5">Category & Rule</th>
                    <th className="px-4 py-2.5">Classification</th>
                    <th className="px-4 py-2.5">Severity</th>
                    <th className="px-4 py-2.5">Vendor / Invoice</th>
                    <th className="px-4 py-2.5 text-right">Variance</th>
                    <th className="px-4 py-2.5">Review Status</th>
                    <th className="px-4 py-2.5">Evidence</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-800/80">
                  {displayList.map((f) => (
                    <tr key={f.id} className="hover:bg-slate-800/40">
                      <td className="px-4 py-2.5">
                        <div className="font-mono font-medium text-slate-100">{f.category}</div>
                        <div className="font-mono text-[10px] text-slate-500">{f.rule_applied}</div>
                      </td>
                      <td className="px-4 py-2.5"><StatusBadge value={f.classification_type} /></td>
                      <td className="px-4 py-2.5"><StatusBadge value={f.severity} /></td>
                      <td className="px-4 py-2.5">
                        <div className="text-slate-200">{f.vendor_name || "—"}</div>
                        <div className="font-mono text-[11px] text-slate-400">{f.invoice_number || "Document"}</div>
                      </td>
                      <td className="px-4 py-2.5 text-right font-mono tabular-nums text-rose-300">
                        {formatMoney(f.variance_amount, f.currency)}
                      </td>
                      <td className="px-4 py-2.5"><StatusBadge value={f.reviewer_status} /></td>
                      <td className="px-4 py-2.5">
                        <button
                          onClick={() => handleInspectFinding(f.id)}
                          className="rounded border border-indigo-500/40 bg-indigo-500/15 px-2.5 py-1 text-xs text-indigo-200 hover:bg-indigo-500/25"
                        >
                          Inspect ({f.evidence_count})
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>

        {selectedFinding && (
          <div className="rounded-xl border border-indigo-500/40 bg-slate-900 p-5 space-y-3">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <StatusBadge value={selectedFinding.finding.severity} />
                <StatusBadge value={selectedFinding.finding.classification_type} />
                <h3 className="font-mono text-sm font-semibold text-slate-100">
                  {selectedFinding.finding.category}
                </h3>
              </div>
              <button onClick={() => setSelectedFinding(null)} className="text-xs text-slate-400 hover:text-white">Close</button>
            </div>
            <p className="text-xs text-slate-200">{selectedFinding.finding.explanation}</p>
            <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
              <div className="rounded-lg border border-slate-800 bg-slate-950 p-3">
                <p className="text-[11px] uppercase tracking-wider text-slate-400">Calculation Details</p>
                <pre className="mt-1.5 overflow-auto font-mono text-[11px] text-emerald-300">
                  {JSON.stringify(selectedFinding.finding.calculation_details, null, 2)}
                </pre>
              </div>
              <div className="rounded-lg border border-slate-800 bg-slate-950 p-3 space-y-2">
                <p className="text-[11px] uppercase tracking-wider text-slate-400">
                  Traceable Evidence References ({selectedFinding.evidence.length})
                </p>
                {selectedFinding.evidence.map((ev: any) => (
                  <div key={ev.id} className="rounded border border-slate-800 bg-slate-900 p-2 text-xs">
                    <div className="flex items-center justify-between">
                      <span className="font-mono text-indigo-300">{ev.evidence_role}</span>
                      {ev.document_id && (
                        <button
                          onClick={() => onOpenDocument(ev.document_id)}
                          className="text-[11px] text-indigo-400 underline"
                        >
                          Open Source Doc (p.{ev.page_number ?? 1})
                        </button>
                      )}
                    </div>
                    <p className="mt-1 font-mono text-[11px] text-slate-300">{ev.quoted_text || ev.source_location}</p>
                  </div>
                ))}
              </div>
            </div>
          </div>
        )}
      </div>
    );
  }

  // =========================================================================
  // 11. HUMAN REVIEW QUEUE
  // =========================================================================
  if (activeTab === "review_queue") {
    const qFindings: any[] = reviewQueue?.pending_findings || [];
    const qDocs: any[] = reviewQueue?.documents_needing_review || [];

    return (
      <div className="space-y-5">
        <div>
          <h2 className="text-xl font-semibold text-slate-100">Human-in-the-Loop Review Queue</h2>
          <p className="text-xs text-slate-400">
            Role-gated review workflow ({user.role}). Every decision is logged append-only without overwriting original AI outputs.
          </p>
        </div>

        {qDocs.length > 0 && (
          <div className="rounded-xl border border-amber-500/30 bg-slate-900 p-4 space-y-2">
            <h3 className="text-xs font-semibold uppercase tracking-wider text-amber-300">
              Documents Requiring Manual Classification or Extraction Review ({qDocs.length})
            </h3>
            {qDocs.map((d) => (
              <div key={d.id} className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 text-xs">
                <div>
                  <span className="font-medium text-slate-100">{d.original_filename}</span>
                  <span className="ml-2"><StatusBadge value={d.effective_category} /></span>
                  {d.prompt_injection_flagged && <span className="ml-2 text-rose-400 font-semibold">[Prompt Injection Quarantined]</span>}
                </div>
                <button
                  onClick={() => onOpenDocument(d.id)}
                  className="rounded bg-indigo-600 px-2.5 py-1 text-xs text-white hover:bg-indigo-500"
                >
                  Open Viewer & Classify
                </button>
              </div>
            ))}
          </div>
        )}

        <div className="space-y-3">
          {qFindings.length === 0 ? (
            <EmptyBox text="Review queue is clear — zero pending findings." />
          ) : (
            qFindings.map((f) => (
              <div key={f.id} className="rounded-xl border border-slate-800 bg-slate-900 p-4 space-y-3">
                <div className="flex flex-wrap items-center gap-2">
                  <StatusBadge value={f.severity} />
                  <StatusBadge value={f.classification_type} />
                  <span className="font-mono text-xs font-semibold text-slate-100">{f.category}</span>
                  <span className="text-xs text-slate-400">
                    {f.vendor_name} {f.invoice_number ? `• ${f.invoice_number}` : ""}
                  </span>
                  {f.variance_amount && (
                    <span className="ml-auto font-mono text-xs font-semibold tabular-nums text-rose-300">
                      {formatMoney(f.variance_amount, f.currency)}
                    </span>
                  )}
                </div>
                <p className="text-xs text-slate-300">{f.explanation}</p>
                {canReview ? (
                  <div className="flex flex-wrap items-center gap-2 border-t border-slate-800 pt-3">
                    <input
                      type="text"
                      placeholder="Mandatory audit explanation / resolution notes..."
                      value={reviewNotes[f.id] || ""}
                      onChange={(e) => setReviewNotes((m) => ({ ...m, [f.id]: e.target.value }))}
                      className="min-w-[240px] flex-1 rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 text-xs text-slate-100 placeholder-slate-500"
                    />
                    <button onClick={() => handleFindingReview(f.id, "approve")} className="inline-flex items-center gap-1 rounded bg-emerald-600 px-2.5 py-1.5 text-xs text-white hover:bg-emerald-500">
                      <CheckCircle className="h-3.5 w-3.5" /> Approve
                    </button>
                    <button onClick={() => handleFindingReview(f.id, "resolve")} className="rounded bg-indigo-600 px-2.5 py-1.5 text-xs text-white hover:bg-indigo-500">
                      Resolve
                    </button>
                    <button onClick={() => handleFindingReview(f.id, "reject")} className="inline-flex items-center gap-1 rounded bg-rose-600 px-2.5 py-1.5 text-xs text-white hover:bg-rose-500">
                      <XCircle className="h-3.5 w-3.5" /> Reject
                    </button>
                    <button onClick={() => handleFindingReview(f.id, "escalate")} className="rounded border border-fuchsia-500/40 bg-fuchsia-500/15 px-2.5 py-1.5 text-xs text-fuchsia-200 hover:bg-fuchsia-500/25">
                      Escalate
                    </button>
                  </div>
                ) : (
                  <p className="text-[11px] text-slate-500">
                    Read-only view for role &ldquo;{user.role}&rdquo;. Reviewer or Admin role required to resolve findings.
                  </p>
                )}
              </div>
            ))
          )}
        </div>
      </div>
    );
  }

  // =========================================================================
  // 12. FINANCIAL RECONCILIATION REPORTS
  // =========================================================================
  if (activeTab === "reports") {
    return (
      <div className="space-y-5">
        <div>
          <h2 className="text-xl font-semibold text-slate-100">Financial Reconciliation & Audit Reports</h2>
          <p className="text-xs text-slate-400">
            Reproducible reports with segregated currency totals, version metadata, and verified CSV, XLSX, and PDF exports.
          </p>
        </div>

        <div className="flex flex-wrap items-end gap-3 rounded-xl border border-slate-800 bg-slate-900 p-4">
          <div>
            <label className="block text-[11px] text-slate-400">Report Scope Type</label>
            <select value={reportType} onChange={(e) => setReportType(e.target.value)} className="mt-1 rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 text-xs text-slate-100">
              <option value="periodic">Periodic Reconciliation Report</option>
              <option value="exception">Exception Report</option>
              <option value="duplicate">Duplicate Detection Report</option>
              <option value="payment">Payment Reconciliation Report</option>
              <option value="per_vendor">Per-Vendor Report</option>
              <option value="per_contract">Per-Contract Report</option>
              <option value="per_invoice">Per-Invoice Report</option>
            </select>
          </div>
          <div className="flex-1 min-w-[220px]">
            <label className="block text-[11px] text-slate-400">Report Title (optional)</label>
            <input value={reportTitle} onChange={(e) => setReportTitle(e.target.value)} placeholder="Q3 2026 Enterprise Billing Integrity Audit" className="mt-1 w-full rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 text-xs text-slate-100" />
          </div>
          <button onClick={handleGenerateReport} className="rounded-lg bg-indigo-600 px-3.5 py-1.5 text-xs font-medium text-white hover:bg-indigo-500">
            Generate Snapshot Report
          </button>
        </div>

        <div className="space-y-3">
          {reports.length === 0 ? (
            <EmptyBox text="No saved audit report snapshots yet. Generate one above to export CSV, XLSX, or PDF." />
          ) : (
            reports.map((r) => (
              <div key={r.id} className="rounded-xl border border-slate-800 bg-slate-900 p-4 flex flex-wrap items-center justify-between gap-4">
                <div>
                  <h3 className="text-sm font-semibold text-slate-100">{r.title}</h3>
                  <p className="mt-0.5 font-mono text-[11px] text-slate-400">
                    Type: {r.report_type} • Rule: {r.rule_version} • Confirmed: {r.report_payload?.confirmed_discrepancies_count ?? 0} • Potential: {r.report_payload?.potential_anomalies_count ?? 0}
                  </p>
                </div>
                <div className="flex items-center gap-2">
                  <button onClick={() => api.downloadReport(r.id, "csv")} className="inline-flex items-center gap-1 rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 text-xs text-slate-200 hover:bg-slate-800">
                    <Download className="h-3.5 w-3.5" /> CSV
                  </button>
                  <button onClick={() => api.downloadReport(r.id, "xlsx")} className="inline-flex items-center gap-1 rounded border border-emerald-500/40 bg-emerald-500/10 px-2.5 py-1.5 text-xs text-emerald-200 hover:bg-emerald-500/20">
                    <FileSpreadsheet className="h-3.5 w-3.5" /> XLSX
                  </button>
                  <button onClick={() => api.downloadReport(r.id, "pdf")} className="inline-flex items-center gap-1 rounded border border-indigo-500/40 bg-indigo-500/10 px-2.5 py-1.5 text-xs text-indigo-200 hover:bg-indigo-500/20">
                    <FileText className="h-3.5 w-3.5" /> PDF
                  </button>
                </div>
              </div>
            ))
          )}
        </div>
      </div>
    );
  }

  // =========================================================================
  // 13. MODEL & PROVIDER CONFIGURATION
  // =========================================================================
  if (activeTab === "models") {
    return (
      <div className="space-y-5">
        <div>
          <h2 className="text-xl font-semibold text-slate-100">Model & Provider Configuration</h2>
          <p className="text-xs text-slate-400">
            External AI egress is gated by server environment <code className="font-mono text-amber-300">ENABLE_EXTERNAL_AI={String(externalAiEnv)}</code>. Sensitive financial documents never leave localhost unless explicitly enabled and redacted.
          </p>
        </div>
        <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
          {models.map((m) => (
            <div key={m.id} className="rounded-xl border border-slate-800 bg-slate-900 p-4 space-y-2.5">
              <div className="flex items-center justify-between">
                <span className="font-mono text-xs font-semibold text-indigo-300">{m.provider_name}</span>
                <StatusBadge value={m.is_default ? "active" : m.is_enabled ? "completed" : "pending"} />
              </div>
              <h3 className="text-sm font-semibold text-slate-100">{m.display_name}</h3>
              <p className="font-mono text-[11px] text-slate-400">Model: {m.model_identifier}</p>
              <div className="text-[11px] text-slate-400 space-y-0.5">
                <div>External Data Egress: {m.requires_external_egress ? "Yes (Gated)" : "None (Local Only)"}</div>
                <div>PII/Account Redaction: {m.redaction_required ? "Enforced" : "Not Required"}</div>
              </div>
              {isAdmin && !m.is_default && m.is_enabled && (
                <button
                  onClick={async () => {
                    try {
                      await api.request(`/models/${m.provider_name}`, {
                        method: "PATCH",
                        body: JSON.stringify({ is_default: true }),
                      });
                      setBanner({ type: "ok", text: `Switched default provider to ${m.provider_name}` });
                      await onRefresh();
                    } catch (e: any) {
                      setBanner({ type: "err", text: e.message });
                    }
                  }}
                  className="rounded border border-indigo-500/40 bg-indigo-500/15 px-2.5 py-1 text-xs text-indigo-200"
                >
                  Set as Default
                </button>
              )}
            </div>
          ))}
        </div>
      </div>
    );
  }

  // =========================================================================
  // 14. SYSTEM SETTINGS & AUDIT TRAIL
  // =========================================================================
  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-xl font-semibold text-slate-100">System Settings & Immutable Audit Trail</h2>
        <p className="text-xs text-slate-400">
          Documented severity threshold rules, classification policies, and append-only security audit log.
        </p>
      </div>

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
        {settings.map((s) => (
          <div key={s.key} className="rounded-xl border border-slate-800 bg-slate-900 p-4">
            <h3 className="font-mono text-xs font-semibold text-indigo-300">{s.key}</h3>
            <p className="mt-1 text-xs text-slate-400">{s.description}</p>
            <pre className="mt-2 overflow-auto rounded bg-slate-950 p-2.5 font-mono text-[11px] text-slate-300">
              {JSON.stringify(s.value, null, 2)}
            </pre>
          </div>
        ))}
      </div>

      <div className="rounded-xl border border-slate-800 bg-slate-900 overflow-hidden">
        <div className="border-b border-slate-800 px-4 py-3 text-xs font-semibold uppercase tracking-wider text-slate-400">
          Recent Security & Operations Audit Log ({auditLogs.length})
        </div>
        <div className="overflow-x-auto max-h-[380px]">
          <table className="w-full text-left text-xs">
            <thead className="sticky top-0 bg-slate-950 text-slate-400">
              <tr>
                <th className="px-4 py-2">Timestamp</th>
                <th className="px-4 py-2">Actor</th>
                <th className="px-4 py-2">Action</th>
                <th className="px-4 py-2">Resource</th>
                <th className="px-4 py-2">Details</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-800/80">
              {auditLogs.map((l) => (
                <tr key={l.id}>
                  <td className="px-4 py-2 font-mono text-[11px] text-slate-400 whitespace-nowrap">
                    {new Date(l.created_at).toLocaleString()}
                  </td>
                  <td className="px-4 py-2 text-slate-300">
                    {l.actor_email || "system"} {l.actor_role ? `(${l.actor_role})` : ""}
                  </td>
                  <td className="px-4 py-2 font-mono text-indigo-300">{l.action}</td>
                  <td className="px-4 py-2 font-mono text-slate-400">{l.resource_type}</td>
                  <td className="px-4 py-2 font-mono text-[11px] text-slate-400 max-w-md truncate">
                    {JSON.stringify(l.details)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}

function KpiCard({
  label,
  value,
  sub,
  accent = "text-slate-100",
}: {
  label: string;
  value: number | string;
  sub: string;
  accent?: string;
}) {
  return (
    <div className="rounded-xl border border-slate-800 bg-slate-900 p-4">
      <p className="text-xs uppercase tracking-wider text-slate-400">{label}</p>
      <p className={`mt-1.5 font-mono text-2xl font-bold tabular-nums ${accent}`}>{value}</p>
      <p className="mt-1 text-xs text-slate-400">{sub}</p>
    </div>
  );
}

function EmptyBox({ text }: { text: string }) {
  return (
    <div className="rounded-lg border border-dashed border-slate-700 bg-slate-950/60 p-8 text-center text-xs text-slate-400">
      {text}
    </div>
  );
}
