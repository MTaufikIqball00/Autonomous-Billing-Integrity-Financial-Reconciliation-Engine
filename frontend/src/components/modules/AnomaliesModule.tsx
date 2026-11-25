"use client";

import React from "react";
import { ShieldAlert, AlertTriangle } from "lucide-react";
import { formatMoney, isPotentialAnomaly } from "@/lib/api";
import { StatusBadge } from "../StatusBadge";
import { EmptyBox } from "./SharedUi";

export function AnomaliesModule({
  findings,
  documents,
  onOpenDocument,
}: {
  findings: any[];
  documents: any[];
  onOpenDocument: (docId: string) => void;
}) {
  const candidates = findings.filter(isPotentialAnomaly);
  const flaggedDocs = documents.filter((d) => d.prompt_injection_flagged || d.is_duplicate_file);

  return (
    <div className="space-y-5">
      <div>
        <h2 className="text-xl font-semibold text-slate-100">Anomaly Detection Center</h2>
        <p className="text-xs text-slate-400">
          Dedicated investigation view for potential anomalies, probable duplicates, unresolved discrepancies, and security quarantine signals. Suspicion is never treated as proof of fraud.
        </p>
      </div>

      {flaggedDocs.length > 0 && (
        <div className="rounded-xl border border-amber-500/30 bg-slate-900 p-4 space-y-2">
          <h3 className="flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-amber-300">
            <ShieldAlert className="h-4 w-4" /> Document Integrity & Prompt Injection Signals ({flaggedDocs.length})
          </h3>
          <div className="space-y-2">
            {flaggedDocs.map((d) => (
              <div key={d.id} className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 text-xs">
                <div className="flex items-center gap-2">
                  <AlertTriangle className="h-3.5 w-3.5 text-amber-400" />
                  <span className="font-medium text-slate-100">{d.original_filename}</span>
                  {d.prompt_injection_flagged && <StatusBadge value="requires_review" />}
                  {d.is_duplicate_file && <span className="font-mono text-[11px] text-rose-300">Duplicate SHA-256</span>}
                </div>
                <button
                  onClick={() => onOpenDocument(d.id)}
                  className="rounded border border-indigo-500/40 bg-indigo-500/15 px-2.5 py-1 text-xs text-indigo-200 hover:bg-indigo-500/25"
                >
                  Inspect Evidence
                </button>
              </div>
            ))}
          </div>
        </div>
      )}

      <div className="rounded-xl border border-slate-800 bg-slate-900 overflow-hidden">
        <div className="border-b border-slate-800 px-4 py-3 text-xs font-semibold uppercase tracking-wider text-slate-400">
          Potential & Unresolved Anomaly Candidates ({candidates.length})
        </div>
        {candidates.length === 0 ? (
          <div className="p-8"><EmptyBox text="No unconfirmed anomaly candidates currently flagged." /></div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-left text-xs">
              <thead className="bg-slate-950 text-slate-400">
                <tr>
                  <th className="px-4 py-2.5">Category</th>
                  <th className="px-4 py-2.5">Candidate Type</th>
                  <th className="px-4 py-2.5">Severity</th>
                  <th className="px-4 py-2.5">Vendor / Invoice</th>
                  <th className="px-4 py-2.5 text-right">Potential Impact</th>
                  <th className="px-4 py-2.5">Explanation</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-800/80">
                {candidates.map((f) => (
                  <tr key={f.id} className="hover:bg-slate-800/40">
                    <td className="px-4 py-2.5 font-mono text-slate-100">{f.category}</td>
                    <td className="px-4 py-2.5"><StatusBadge value={f.classification_type} /></td>
                    <td className="px-4 py-2.5"><StatusBadge value={f.severity} /></td>
                    <td className="px-4 py-2.5">
                      <div className="text-slate-200">{f.vendor_name || "—"}</div>
                      <div className="font-mono text-[11px] text-slate-400">{f.invoice_number || "Document"}</div>
                    </td>
                    <td className="px-4 py-2.5 text-right font-mono tabular-nums text-amber-300">
                      {formatMoney(f.variance_amount, f.currency)}
                    </td>
                    <td className="px-4 py-2.5 text-slate-300 max-w-md">{f.explanation}</td>
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
