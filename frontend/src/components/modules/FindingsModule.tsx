"use client";

import React, { useState } from "react";
import { api, formatMoney, PaginationMeta } from "@/lib/api";
import { StatusBadge } from "../StatusBadge";
import { EmptyBox, PaginationBar } from "./SharedUi";

export function FindingsModule({
  findings,
  pagination,
  filters,
  onFilterChange,
  onPageChange,
  onOpenDocument,
  setBanner,
}: {
  findings: any[];
  pagination?: PaginationMeta;
  filters: { category: string; severity: string; status: string; classification_type: string };
  onFilterChange: (next: { category: string; severity: string; status: string; classification_type: string }) => void;
  onPageChange: (offset: number) => void;
  onOpenDocument: (docId: string) => void;
  setBanner: (b: { type: "ok" | "err"; text: string } | null) => void;
}) {
  const [selectedFinding, setSelectedFinding] = useState<any>(null);

  const handleInspectFinding = async (findingId: string) => {
    try {
      const detail = await api.request(`/findings/${findingId}`);
      setSelectedFinding(detail);
    } catch (err: any) {
      setBanner({ type: "err", text: err.message });
    }
  };

  return (
    <div className="space-y-5">
      <div>
        <h2 className="text-xl font-semibold text-slate-100">Evidence-Backed Audit Findings</h2>
        <p className="text-xs text-slate-400">
          Complete auditable ledger distinguishing confirmed rule violations from unresolved or potential anomalies, with traceable document evidence.
        </p>
      </div>

      {/* Filter Toolbar */}
      <div className="flex flex-wrap items-center gap-3 rounded-xl border border-slate-800 bg-slate-900 p-3 text-xs">
        <select
          aria-label="Filter by category"
          value={filters.category}
          onChange={(e) => onFilterChange({ ...filters, category: e.target.value })}
          className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 text-slate-200"
        >
          <option value="">All Categories</option>
          {[
            "CONTRACT_PRICE_MISMATCH",
            "QUANTITY_MISMATCH",
            "DISCOUNT_MISMATCH",
            "TAX_MISMATCH",
            "UNAUTHORIZED_CHARGE",
            "DUPLICATE_INVOICE",
            "PAYMENT_MISMATCH",
            "DUPLICATE_PAYMENT_CANDIDATE",
            "CONTRACT_EXPIRED",
            "CURRENCY_MISMATCH",
            "TOTAL_ARITHMETIC_MISMATCH",
            "DOCUMENT_MATCH_AMBIGUITY",
            "EXTRACTION_UNCERTAIN",
          ].map((c) => <option key={c} value={c}>{c}</option>)}
        </select>

        <select
          aria-label="Filter by severity"
          value={filters.severity}
          onChange={(e) => onFilterChange({ ...filters, severity: e.target.value })}
          className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 text-slate-200"
        >
          <option value="">All Severities</option>
          <option value="critical">critical</option>
          <option value="high">high</option>
          <option value="medium">medium</option>
          <option value="low">low</option>
        </select>

        <select
          aria-label="Filter by status"
          value={filters.status}
          onChange={(e) => onFilterChange({ ...filters, status: e.target.value })}
          className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 text-slate-200"
        >
          <option value="">All Statuses</option>
          <option value="open">open</option>
          <option value="requires_review">requires_review</option>
          <option value="approved">approved</option>
          <option value="resolved">resolved</option>
          <option value="rejected">rejected</option>
          <option value="escalated">escalated</option>
        </select>

        <select
          aria-label="Filter by classification"
          value={filters.classification_type}
          onChange={(e) => onFilterChange({ ...filters, classification_type: e.target.value })}
          className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 text-slate-200"
        >
          <option value="">All Classifications</option>
          <option value="confirmed_rule_violation">confirmed_rule_violation</option>
          <option value="exact_duplicate">exact_duplicate</option>
          <option value="probable_duplicate">probable_duplicate</option>
          <option value="potential_anomaly">potential_anomaly</option>
          <option value="unresolved_discrepancy">unresolved_discrepancy</option>
        </select>
      </div>

      <div className="rounded-xl border border-slate-800 bg-slate-900 overflow-hidden">
        {findings.length === 0 ? (
          <div className="p-8"><EmptyBox text="No findings match the current filter." /></div>
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
                {findings.map((f) => (
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
        <PaginationBar pagination={pagination} onPageChange={onPageChange} />
      </div>

      {selectedFinding && (
        <div className="rounded-xl border border-indigo-500/40 bg-slate-900 p-5 space-y-3">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <StatusBadge value={selectedFinding.finding.severity} />
              <StatusBadge value={selectedFinding.finding.classification_type} />
              <h3 className="font-mono text-sm font-semibold text-slate-100">{selectedFinding.finding.category}</h3>
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
