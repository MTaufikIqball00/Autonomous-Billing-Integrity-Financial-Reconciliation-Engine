"use client";

import React from "react";

const STATUS_STYLES: Record<string, string> = {
  // Positive / Completed
  reconciled: "bg-emerald-500/15 text-emerald-300 border-emerald-500/30",
  completed: "bg-emerald-500/15 text-emerald-300 border-emerald-500/30",
  pass: "bg-emerald-500/15 text-emerald-300 border-emerald-500/30",
  approved: "bg-emerald-500/15 text-emerald-300 border-emerald-500/30",
  resolved: "bg-emerald-500/15 text-emerald-300 border-emerald-500/30",
  fully_paid: "bg-emerald-500/15 text-emerald-300 border-emerald-500/30",
  fully_allocated: "bg-emerald-500/15 text-emerald-300 border-emerald-500/30",
  deterministic_matched: "bg-emerald-500/15 text-emerald-300 border-emerald-500/30",
  valid: "bg-emerald-500/15 text-emerald-300 border-emerald-500/30",
  active: "bg-emerald-500/15 text-emerald-300 border-emerald-500/30",
  // Confirmed Discrepancies / Critical / Failed
  discrepancy_confirmed: "bg-rose-500/15 text-rose-300 border-rose-500/30",
  discrepancy: "bg-rose-500/15 text-rose-300 border-rose-500/30",
  confirmed_rule_violation: "bg-rose-500/15 text-rose-300 border-rose-500/30",
  exact_duplicate: "bg-rose-500/15 text-rose-300 border-rose-500/30",
  critical: "bg-rose-500/20 text-rose-200 border-rose-500/40 font-semibold",
  high: "bg-orange-500/15 text-orange-300 border-orange-500/30",
  failed: "bg-rose-500/15 text-rose-300 border-rose-500/30",
  rejected: "bg-rose-500/15 text-rose-300 border-rose-500/30",
  overpaid: "bg-rose-500/15 text-rose-300 border-rose-500/30",
  escalated: "bg-fuchsia-500/15 text-fuchsia-300 border-fuchsia-500/30",
  // Review / Potential / Ambiguous
  requires_review: "bg-amber-500/15 text-amber-300 border-amber-500/30",
  routed_to_human: "bg-amber-500/15 text-amber-300 border-amber-500/30",
  probable_duplicate: "bg-amber-500/15 text-amber-300 border-amber-500/30",
  potential_anomaly: "bg-amber-500/15 text-amber-300 border-amber-500/30",
  unresolved_discrepancy: "bg-amber-500/15 text-amber-300 border-amber-500/30",
  ambiguous: "bg-amber-500/15 text-amber-300 border-amber-500/30",
  ambiguous_needs_review: "bg-amber-500/15 text-amber-300 border-amber-500/30",
  medium: "bg-amber-500/15 text-amber-300 border-amber-500/30",
  partially_paid: "bg-amber-500/15 text-amber-300 border-amber-500/30",
  partially_allocated: "bg-amber-500/15 text-amber-300 border-amber-500/30",
  // Processing / Informational
  processing: "bg-sky-500/15 text-sky-300 border-sky-500/30",
  running: "bg-sky-500/15 text-sky-300 border-sky-500/30",
  human_corrected: "bg-indigo-500/15 text-indigo-300 border-indigo-500/30",
  human_confirmed: "bg-indigo-500/15 text-indigo-300 border-indigo-500/30",
  low: "bg-sky-500/15 text-sky-300 border-sky-500/30",
  // Pending / Neutral
  pending: "bg-slate-700/60 text-slate-300 border-slate-600",
  open: "bg-slate-700/60 text-slate-200 border-slate-600",
  unmatched: "bg-slate-700/60 text-slate-300 border-slate-600",
  unknown: "bg-slate-700/60 text-slate-300 border-slate-600",
};

export function StatusBadge({ value }: { value: string | null | undefined }) {
  const key = (value || "unknown").toLowerCase();
  const cls = STATUS_STYLES[key] || "bg-slate-800 text-slate-300 border-slate-700";
  const label = (value || "unknown").replace(/_/g, " ");
  return (
    <span
      className={`inline-flex items-center rounded-md border px-2 py-0.5 text-xs capitalize whitespace-nowrap ${cls}`}
    >
      {label}
    </span>
  );
}
