"use client";

import React, { useEffect, useState } from "react";
import { RefreshCw, FileSearch, X } from "lucide-react";
import { api, formatMoney } from "@/lib/api";
import { StatusBadge } from "./StatusBadge";

interface Props {
  invoiceId: string;
  onClose: () => void;
  onOpenDocument?: (docId: string) => void;
  onReconciled?: () => void;
}

export function ReconciliationWorkbench({
  invoiceId,
  onClose,
  onOpenDocument,
  onReconciled,
}: Props) {
  const [data, setData] = useState<any>(null);
  const [loading, setLoading] = useState(true);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      const res = await api.request(`/invoices/${invoiceId}`);
      setData(res);
    } catch (e: any) {
      setError(e.message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [invoiceId]);

  const handleRerun = async () => {
    setRunning(true);
    setError(null);
    try {
      await api.request(`/invoices/${invoiceId}/reconcile`, { method: "POST" });
      await load();
      onReconciled?.();
    } catch (e: any) {
      setError(e.message);
    } finally {
      setRunning(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/75 p-4 backdrop-blur-sm"
      role="dialog"
      aria-modal="true"
      aria-label="Invoice Reconciliation Workbench"
    >
      <div className="flex max-h-[92vh] w-full max-w-6xl flex-col overflow-hidden rounded-xl border border-slate-800 bg-slate-900 shadow-2xl">
        <div className="flex flex-wrap items-center justify-between gap-3 border-b border-slate-800 px-5 py-3.5">
          <div>
            <div className="flex items-center gap-2.5">
              <h3 className="text-base font-semibold text-slate-100">
                Reconciliation Workbench — {data?.invoice?.invoice_number || "..."}
              </h3>
              {data?.invoice && <StatusBadge value={data.invoice.reconciliation_status} />}
            </div>
            {data?.invoice && (
              <p className="mt-0.5 text-xs text-slate-400">
                Vendor: {data.invoice.vendor_name} • Contract:{" "}
                <span className="font-mono text-slate-200">
                  {data.invoice.matched_contract_number || data.invoice.referenced_contract_number || "Unmatched"}
                </span>{" "}
                {data.invoice.matched_contract_version_label && (
                  <span className="rounded bg-indigo-500/20 px-1.5 py-0.5 font-mono text-[11px] text-indigo-300">
                    {data.invoice.matched_contract_version_label}
                  </span>
                )}
              </p>
            )}
          </div>
          <div className="flex items-center gap-2">
            {data?.invoice?.document_id && onOpenDocument && (
              <button
                onClick={() => onOpenDocument(data.invoice.document_id)}
                className="inline-flex items-center gap-1.5 rounded-lg border border-slate-700 bg-slate-800 px-3 py-1.5 text-xs text-slate-200 hover:bg-slate-700"
              >
                <FileSearch className="h-3.5 w-3.5" /> Source Document
              </button>
            )}
            <button
              onClick={handleRerun}
              disabled={running}
              className="inline-flex items-center gap-1.5 rounded-lg bg-indigo-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-indigo-500 disabled:opacity-50"
            >
              <RefreshCw className={`h-3.5 w-3.5 ${running ? "animate-spin" : ""}`} />
              {running ? "Reconciling..." : "Re-run Deterministic Engine"}
            </button>
            <button
              onClick={onClose}
              aria-label="Close workbench"
              className="rounded-lg p-1.5 text-slate-400 hover:bg-slate-800 hover:text-white"
            >
              <X className="h-5 w-5" />
            </button>
          </div>
        </div>

        {error && (
          <div className="mx-5 mt-3 rounded-lg border border-rose-500/40 bg-rose-500/10 px-3 py-2 text-xs text-rose-200">
            {error}
          </div>
        )}

        {loading || !data ? (
          <div className="p-12 text-center text-sm text-slate-400">
            Loading deterministic reconciliation breakdown...
          </div>
        ) : (
          <div className="flex-1 space-y-5 overflow-y-auto p-5">
            {/* Totals Comparison Strip */}
            <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
              <div className="rounded-lg border border-slate-800 bg-slate-950 p-3">
                <p className="text-[11px] uppercase tracking-wider text-slate-400">Stated Invoice Total</p>
                <p className="mt-1 font-mono text-lg font-semibold tabular-nums text-slate-100">
                  {formatMoney(data.invoice.stated_total, data.invoice.currency)}
                </p>
              </div>
              <div className="rounded-lg border border-slate-800 bg-slate-950 p-3">
                <p className="text-[11px] uppercase tracking-wider text-slate-400">Deterministic Recalculated</p>
                <p className="mt-1 font-mono text-lg font-semibold tabular-nums text-emerald-300">
                  {formatMoney(data.invoice.recalculated_total, data.invoice.currency)}
                </p>
              </div>
              <div className="rounded-lg border border-slate-800 bg-slate-950 p-3">
                <p className="text-[11px] uppercase tracking-wider text-slate-400">Contract Match</p>
                <div className="mt-1.5">
                  <StatusBadge value={data.invoice.match_confidence_status} />
                </div>
              </div>
              <div className="rounded-lg border border-slate-800 bg-slate-950 p-3">
                <p className="text-[11px] uppercase tracking-wider text-slate-400">Findings Detected</p>
                <p className="mt-1 font-mono text-lg font-semibold tabular-nums text-amber-300">
                  {data.findings.length}
                </p>
              </div>
            </div>

            {/* Line-Item Contract vs Invoice Comparison */}
            <div className="rounded-lg border border-slate-800 bg-slate-950 overflow-hidden">
              <div className="border-b border-slate-800 px-4 py-2.5 text-xs font-semibold uppercase tracking-wider text-slate-400">
                Line-Item Comparison (Contract Price vs Invoiced Price)
              </div>
              <div className="overflow-x-auto">
                <table className="w-full text-left text-xs">
                  <thead className="bg-slate-900 text-slate-400">
                    <tr>
                      <th className="px-3 py-2">#</th>
                      <th className="px-3 py-2">Code / Description</th>
                      <th className="px-3 py-2 text-right">Qty</th>
                      <th className="px-3 py-2 text-right">Contract Price</th>
                      <th className="px-3 py-2 text-right">Invoice Price</th>
                      <th className="px-3 py-2 text-right">Tax</th>
                      <th className="px-3 py-2 text-right">Line Total</th>
                      <th className="px-3 py-2">Match Method</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-slate-800/80">
                    {data.line_items.map((l: any) => (
                      <tr key={l.id} className="hover:bg-slate-900/50">
                        <td className="px-3 py-2 font-mono text-slate-400">{l.line_number}</td>
                        <td className="px-3 py-2">
                          <div className="font-medium text-slate-200">{l.description}</div>
                          <div className="font-mono text-[11px] text-slate-500">
                            {l.item_code || "NO-CODE"} {l.is_additional_charge ? "• ADDITIONAL CHARGE" : ""}
                          </div>
                        </td>
                        <td className="px-3 py-2 text-right font-mono tabular-nums text-slate-200">
                          {l.quantity} {l.unit}
                        </td>
                        <td className="px-3 py-2 text-right font-mono tabular-nums text-emerald-300">
                          {l.contract_unit_price ?? "—"}
                        </td>
                        <td className="px-3 py-2 text-right font-mono tabular-nums text-slate-100">
                          {l.unit_price}
                        </td>
                        <td className="px-3 py-2 text-right font-mono tabular-nums text-slate-300">
                          {l.tax_amount}
                        </td>
                        <td className="px-3 py-2 text-right font-mono tabular-nums text-slate-100">
                          {l.stated_line_total}
                        </td>
                        <td className="px-3 py-2">
                          <StatusBadge value={l.match_method} />
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>

            {/* Stage A-L Deterministic Results */}
            <div className="rounded-lg border border-slate-800 bg-slate-950 overflow-hidden">
              <div className="border-b border-slate-800 px-4 py-2.5 text-xs font-semibold uppercase tracking-wider text-slate-400">
                Deterministic Reconciliation Stages (A–L)
              </div>
              <div className="overflow-x-auto">
                <table className="w-full text-left text-xs">
                  <thead className="bg-slate-900 text-slate-400">
                    <tr>
                      <th className="px-3 py-2">Stage</th>
                      <th className="px-3 py-2">Status</th>
                      <th className="px-3 py-2 text-right">Expected</th>
                      <th className="px-3 py-2 text-right">Actual</th>
                      <th className="px-3 py-2 text-right">Variance</th>
                      <th className="px-3 py-2">Tolerance Policy</th>
                      <th className="px-3 py-2">Rule Applied</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-slate-800/80">
                    {data.stages.map((s: any, idx: number) => (
                      <tr key={idx}>
                        <td className="px-3 py-2 font-medium text-slate-200">
                          <span className="mr-1.5 rounded bg-slate-800 px-1.5 py-0.5 font-mono text-[11px] text-indigo-300">
                            {s.stage_code}
                          </span>
                          {s.stage_name}
                        </td>
                        <td className="px-3 py-2">
                          <StatusBadge value={s.status} />
                        </td>
                        <td className="px-3 py-2 text-right font-mono tabular-nums text-slate-300">
                          {s.expected_value ?? "—"}
                        </td>
                        <td className="px-3 py-2 text-right font-mono tabular-nums text-slate-200">
                          {s.actual_value ?? "—"}
                        </td>
                        <td className="px-3 py-2 text-right font-mono tabular-nums text-rose-300">
                          {s.variance_amount ?? "—"}
                        </td>
                        <td className="px-3 py-2 font-mono text-[11px] text-amber-300">
                          {s.permitted_tolerance || "—"}
                        </td>
                        <td className="px-3 py-2 font-mono text-[11px] text-slate-400">
                          {s.rule_applied}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>

            {/* Findings for this Invoice */}
            {data.findings.length > 0 && (
              <div className="rounded-lg border border-slate-800 bg-slate-950 p-4 space-y-2.5">
                <h4 className="text-xs font-semibold uppercase tracking-wider text-slate-400">
                  Traceable Discrepancies & Anomalies
                </h4>
                {data.findings.map((f: any) => (
                  <div
                    key={f.id}
                    className="rounded-lg border border-slate-800 bg-slate-900 p-3 text-xs"
                  >
                    <div className="flex flex-wrap items-center gap-2">
                      <StatusBadge value={f.severity} />
                      <StatusBadge value={f.classification_type} />
                      <span className="font-mono font-semibold text-slate-100">{f.category}</span>
                      {f.variance_amount && (
                        <span className="ml-auto font-mono font-semibold tabular-nums text-rose-300">
                          Variance: {formatMoney(f.variance_amount, f.currency)}
                        </span>
                      )}
                    </div>
                    <p className="mt-1.5 text-slate-300">{f.explanation}</p>
                    <p className="mt-1 font-mono text-[11px] text-slate-500">
                      Rule: {f.rule_applied} • Reviewer Status: {f.reviewer_status}
                    </p>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
