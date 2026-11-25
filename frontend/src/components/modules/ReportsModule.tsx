"use client";

import React, { useState } from "react";
import { Download, FileSpreadsheet, FileText } from "lucide-react";
import { api, PaginationMeta } from "@/lib/api";
import { EmptyBox, PaginationBar } from "./SharedUi";

export function ReportsModule({
  reports,
  pagination,
  onRefresh,
  onPageChange,
  setBanner,
}: {
  reports: any[];
  pagination?: PaginationMeta;
  onRefresh: () => Promise<void>;
  onPageChange: (offset: number) => void;
  setBanner: (b: { type: "ok" | "err"; text: string } | null) => void;
}) {
  const [reportType, setReportType] = useState("periodic");
  const [reportTitle, setReportTitle] = useState("");

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

  return (
    <div className="space-y-5">
      <div>
        <h2 className="text-xl font-semibold text-slate-100">Financial Reconciliation & Audit Reports</h2>
        <p className="text-xs text-slate-400">
          Reproducible reports with segregated currency totals, version metadata, CSV formula-injection neutralization, and full multi-page PDF exports.
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

      <div className="rounded-xl border border-slate-800 bg-slate-900 overflow-hidden">
        {reports.length === 0 ? (
          <div className="p-8"><EmptyBox text="No saved audit report snapshots yet. Generate one above to export CSV, XLSX, or PDF." /></div>
        ) : (
          <div className="divide-y divide-slate-800">
            {reports.map((r) => (
              <div key={r.id} className="p-4 flex flex-wrap items-center justify-between gap-4">
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
            ))}
          </div>
        )}
        <PaginationBar pagination={pagination} onPageChange={onPageChange} />
      </div>
    </div>
  );
}
