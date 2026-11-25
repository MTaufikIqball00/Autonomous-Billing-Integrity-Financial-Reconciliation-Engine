"use client";

import React, { useState } from "react";
import { CheckCircle, XCircle } from "lucide-react";
import { api, formatMoney, UserSession } from "@/lib/api";
import { StatusBadge } from "../StatusBadge";
import { EmptyBox } from "./SharedUi";

export function ReviewQueueModule({
  user,
  reviewQueue,
  onRefresh,
  onOpenDocument,
  setBanner,
}: {
  user: UserSession;
  reviewQueue: any;
  onRefresh: () => Promise<void>;
  onOpenDocument: (docId: string) => void;
  setBanner: (b: { type: "ok" | "err"; text: string } | null) => void;
}) {
  const [reviewNotes, setReviewNotes] = useState<Record<string, string>>({});
  const canReview = user.role === "admin" || user.role === "reviewer";

  const qFindings: any[] = reviewQueue?.pending_findings || [];
  const qDocs: any[] = reviewQueue?.documents_needing_review || [];

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
                <span className="text-xs text-slate-400">{f.vendor_name} {f.invoice_number ? `• ${f.invoice_number}` : ""}</span>
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
