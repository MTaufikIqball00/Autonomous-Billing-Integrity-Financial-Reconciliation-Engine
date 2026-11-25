"use client";

import React from "react";
import { Sparkles } from "lucide-react";
import { api, formatMoney, UserSession } from "@/lib/api";
import { StatusBadge } from "../StatusBadge";
import { EmptyBox, KpiCard } from "./SharedUi";

export function OverviewModule({
  user,
  overview,
  findings,
  onRefresh,
  setBanner,
}: {
  user: UserSession;
  overview: any;
  findings: any[];
  onRefresh: () => Promise<void>;
  setBanner: (b: { type: "ok" | "err"; text: string } | null) => void;
}) {
  const canReview = user.role === "admin" || user.role === "reviewer";

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

      <div className="rounded-xl border border-slate-800 bg-slate-900 p-5">
        <div className="mb-3">
          <h3 className="text-sm font-semibold text-slate-100">
            Financial Variance Breakdown by Currency
          </h3>
          <p className="text-xs text-slate-400">
            Amounts in different currencies are strictly segregated (no unverified FX conversion).
          </p>
        </div>
        {!overview?.currency_breakdown?.length ? (
          <EmptyBox text="No currency variance records in database yet. Upload documents or load clearly labeled synthetic test fixtures." />
        ) : (
          <div className="grid grid-cols-1 gap-3 md:grid-cols-3">
            {overview.currency_breakdown.map((b: any) => (
              <div key={b.currency} className="rounded-lg border border-slate-800 bg-slate-950 p-4">
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
                    <td className="px-4 py-2.5"><StatusBadge value={f.classification_type} /></td>
                    <td className="px-4 py-2.5"><StatusBadge value={f.severity} /></td>
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
