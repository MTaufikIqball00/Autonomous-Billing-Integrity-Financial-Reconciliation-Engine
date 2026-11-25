"use client";

import React from "react";
import { RefreshCw } from "lucide-react";
import { PaginationMeta } from "@/lib/api";
import { StatusBadge } from "../StatusBadge";
import { EmptyBox, PaginationBar } from "./SharedUi";

export function AgentsModule({
  agentRuns,
  pagination,
  onPageChange,
  onRefresh,
}: {
  agentRuns: any[];
  pagination?: PaginationMeta;
  onPageChange: (offset: number) => void;
  onRefresh: () => Promise<void>;
}) {
  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-xl font-semibold text-slate-100">AI Agent Activity & Traceability</h2>
          <p className="text-xs text-slate-400">
            Execution log of multi-agent document ingestion, validation, reconciliation, and reporting workflows.
          </p>
        </div>
        <button
          onClick={() => onRefresh()}
          className="inline-flex items-center gap-1.5 rounded-lg border border-slate-700 bg-slate-900 px-3 py-1.5 text-xs text-slate-200 hover:bg-slate-800"
        >
          <RefreshCw className="h-3.5 w-3.5" /> Refresh Runs
        </button>
      </div>

      <div className="rounded-xl border border-slate-800 bg-slate-900 overflow-hidden">
        {agentRuns.length === 0 ? (
          <div className="p-8">
            <EmptyBox text="No agent execution runs logged yet. Upload a document or reconcile an invoice to see workflow traces." />
          </div>
        ) : (
          <>
            <div className="overflow-x-auto">
              <table className="w-full text-left text-xs">
                <thead className="bg-slate-950 text-slate-400">
                  <tr>
                    <th className="px-4 py-2.5">Agent / Step</th>
                    <th className="px-4 py-2.5">Workflow Run</th>
                    <th className="px-4 py-2.5">Model / Rule Version</th>
                    <th className="px-4 py-2.5">Status</th>
                    <th className="px-4 py-2.5 text-right">Duration</th>
                    <th className="px-4 py-2.5">Summary / Error</th>
                    <th className="px-4 py-2.5">Started At</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-800">
                  {agentRuns.map((run) => (
                    <tr key={run.id} className="hover:bg-slate-800/30">
                      <td className="px-4 py-2.5 font-medium text-slate-100">
                        <div>{run.agent_name}</div>
                        <div className="font-mono text-[10px] text-slate-500">Step #{run.step_number}</div>
                      </td>
                      <td className="px-4 py-2.5 font-mono text-[11px] text-slate-400">
                        {String(run.workflow_run_id || "").slice(0, 8)}…
                      </td>
                      <td className="px-4 py-2.5 font-mono text-[11px] text-indigo-300">
                        {run.model_or_rule_version}
                      </td>
                      <td className="px-4 py-2.5">
                        <StatusBadge value={run.status} />
                      </td>
                      <td className="px-4 py-2.5 text-right font-mono tabular-nums text-slate-300">
                        {run.duration_ms != null ? `${run.duration_ms} ms` : "—"}
                      </td>
                      <td className="max-w-xs truncate px-4 py-2.5 font-mono text-[11px] text-slate-300">
                        {run.error_message ? (
                          <span className="text-rose-300">{run.error_message}</span>
                        ) : (
                          JSON.stringify(run.output_payload || {})
                        )}
                      </td>
                      <td className="px-4 py-2.5 font-mono text-[11px] text-slate-400">
                        {new Date(run.started_at).toLocaleString()}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <PaginationBar pagination={pagination} onPageChange={onPageChange} />
          </>
        )}
      </div>
    </div>
  );
}
