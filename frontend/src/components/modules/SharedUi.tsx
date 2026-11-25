"use client";

import React from "react";
import { PaginationMeta } from "@/lib/api";

export function KpiCard({
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

export function EmptyBox({ text }: { text: string }) {
  return (
    <div className="rounded-lg border border-dashed border-slate-700 bg-slate-950/60 p-8 text-center text-xs text-slate-400">
      {text}
    </div>
  );
}

export function PaginationBar({
  pagination,
  onPageChange,
}: {
  pagination?: PaginationMeta;
  onPageChange: (nextOffset: number) => void;
}) {
  if (!pagination || pagination.total <= pagination.limit) return null;
  const page = Math.floor(pagination.offset / pagination.limit) + 1;
  const totalPages = Math.max(1, Math.ceil(pagination.total / pagination.limit));
  const hasPrev = pagination.offset > 0;
  const hasNext = pagination.offset + pagination.limit < pagination.total;

  return (
    <div className="flex items-center justify-between border-t border-slate-800 bg-slate-950/60 px-4 py-2.5 text-xs text-slate-400">
      <span className="font-mono tabular-nums">
        Showing {pagination.offset + 1}–{Math.min(pagination.offset + pagination.limit, pagination.total)} of {pagination.total}
      </span>
      <div className="flex items-center gap-2">
        <button
          disabled={!hasPrev}
          onClick={() => onPageChange(Math.max(0, pagination.offset - pagination.limit))}
          className="rounded border border-slate-700 px-2.5 py-1 text-slate-200 hover:bg-slate-800 disabled:opacity-40"
        >
          Prev
        </button>
        <span className="font-mono tabular-nums">
          Page {page} / {totalPages}
        </span>
        <button
          disabled={!hasNext}
          onClick={() => onPageChange(pagination.offset + pagination.limit)}
          className="rounded border border-slate-700 px-2.5 py-1 text-slate-200 hover:bg-slate-800 disabled:opacity-40"
        >
          Next
        </button>
      </div>
    </div>
  );
}
