"use client";

import React, { useState } from "react";
import { api } from "@/lib/api";
import { StatusBadge } from "../StatusBadge";

export function ModelsModule({
  models,
  externalAiEnv,
  isAdmin,
  onRefresh,
  setBanner,
}: {
  models: any[];
  externalAiEnv: boolean;
  isAdmin: boolean;
  onRefresh: () => Promise<void>;
  setBanner: (b: { type: "ok" | "err"; text: string } | null) => void;
}) {
  const [modelIds, setModelIds] = useState<Record<string, string>>({});

  const updateProvider = async (provider: string, payload: Record<string, any>) => {
    try {
      await api.request(`/models/${provider}`, {
        method: "PATCH",
        body: JSON.stringify(payload),
      });
      setBanner({ type: "ok", text: `Updated provider '${provider}'.` });
      await onRefresh();
    } catch (e: any) {
      setBanner({ type: "err", text: e.message });
    }
  };

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
          <div key={m.id} className="rounded-xl border border-slate-800 bg-slate-900 p-4 space-y-3">
            <div className="flex items-center justify-between">
              <span className="font-mono text-xs font-semibold text-indigo-300">{m.provider_name}</span>
              <StatusBadge value={m.is_default ? "active" : m.is_enabled ? "completed" : "pending"} />
            </div>
            <h3 className="text-sm font-semibold text-slate-100">{m.display_name}</h3>
            <p className="font-mono text-[11px] text-slate-400">Current Model: {m.model_identifier}</p>
            <div className="text-[11px] text-slate-400 space-y-0.5">
              <div>External Data Egress: {m.requires_external_egress ? "Yes (Gated)" : "None (Local Only)"}</div>
              <div>PII/Account Redaction: {m.redaction_required ? "Enforced" : "Not Required"}</div>
            </div>
            {isAdmin && (
              <div className="space-y-2 border-t border-slate-800 pt-3">
                <div className="flex gap-1.5">
                  <input
                    type="text"
                    placeholder={m.model_identifier}
                    value={modelIds[m.provider_name] ?? ""}
                    onChange={(e) => setModelIds((prev) => ({ ...prev, [m.provider_name]: e.target.value }))}
                    className="flex-1 rounded border border-slate-700 bg-slate-950 px-2 py-1 font-mono text-[11px] text-slate-100"
                  />
                  <button
                    onClick={() => updateProvider(m.provider_name, { model_identifier: modelIds[m.provider_name] })}
                    disabled={!modelIds[m.provider_name]}
                    className="rounded border border-slate-700 px-2 py-1 text-[11px] text-slate-200 hover:bg-slate-800 disabled:opacity-40"
                  >
                    Save ID
                  </button>
                </div>
                <div className="flex flex-wrap gap-2">
                  <button
                    onClick={() => updateProvider(m.provider_name, { is_enabled: !m.is_enabled })}
                    className="rounded border border-slate-700 px-2 py-1 text-[11px] text-slate-200 hover:bg-slate-800"
                  >
                    {m.is_enabled ? "Disable" : "Enable"}
                  </button>
                  {m.requires_external_egress && (
                    <button
                      onClick={() => updateProvider(m.provider_name, { redaction_required: !m.redaction_required })}
                      className="rounded border border-slate-700 px-2 py-1 text-[11px] text-slate-200 hover:bg-slate-800"
                    >
                      {m.redaction_required ? "Redaction: ON" : "Redaction: OFF"}
                    </button>
                  )}
                  {!m.is_default && m.is_enabled && (
                    <button
                      onClick={() => updateProvider(m.provider_name, { is_default: true })}
                      className="rounded border border-indigo-500/40 bg-indigo-500/15 px-2.5 py-1 text-[11px] text-indigo-200"
                    >
                      Set Default
                    </button>
                  )}
                </div>
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
