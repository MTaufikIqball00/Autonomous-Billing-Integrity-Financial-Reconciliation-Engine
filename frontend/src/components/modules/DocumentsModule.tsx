"use client";

import React, { useState } from "react";
import { Upload, Eye, RefreshCw, AlertTriangle, ShieldAlert } from "lucide-react";
import { api, PaginationMeta } from "@/lib/api";
import { StatusBadge } from "../StatusBadge";
import { EmptyBox, PaginationBar } from "./SharedUi";

export function DocumentsModule({
  documents,
  pagination,
  vendors,
  canWrite,
  onRefresh,
  onPageChange,
  onOpenDocument,
  setBanner,
}: {
  documents: any[];
  pagination?: PaginationMeta;
  vendors: any[];
  canWrite: boolean;
  onRefresh: () => Promise<void>;
  onPageChange: (offset: number) => void;
  onOpenDocument: (docId: string) => void;
  setBanner: (b: { type: "ok" | "err"; text: string } | null) => void;
}) {
  const [uploadPct, setUploadPct] = useState<number | null>(null);
  const [uploading, setUploading] = useState(false);
  const [selectedVendorId, setSelectedVendorId] = useState("");

  const handleUpload = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    setUploading(true);
    setUploadPct(0);
    setBanner(null);
    try {
      const res = await api.uploadDocument(
        file,
        (p) => setUploadPct(p),
        selectedVendorId || undefined
      );
      setBanner({
        type: "ok",
        text: `Uploaded '${res.original_filename}' (SHA-256: ${res.sha256_hash.slice(0, 12)}…)${
          res.is_duplicate_file ? " — DUPLICATE FILE FINGERPRINT DETECTED" : ""
        }`,
      });
      await onRefresh();
    } catch (err: any) {
      setBanner({ type: "err", text: err.message });
    } finally {
      setUploading(false);
      setUploadPct(null);
      e.target.value = "";
    }
  };

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-xl font-semibold text-slate-100">Document Processing Center</h2>
          <p className="text-xs text-slate-400">
            Verified formats: PDF (digital & scanned/hybrid OCR), PNG, JPEG, TIFF, DOCX, XLSX, CSV. Original bytes preserved immutably with SHA-256 fingerprinting.
          </p>
        </div>
        {canWrite && (
          <div className="flex flex-wrap items-center gap-2">
            <select
              aria-label="Optional Vendor Context"
              value={selectedVendorId}
              onChange={(e) => setSelectedVendorId(e.target.value)}
              className="rounded-lg border border-slate-700 bg-slate-900 px-2.5 py-2 text-xs text-slate-200"
            >
              <option value="">Auto-detect Vendor</option>
              {vendors.map((v) => (
                <option key={v.id} value={v.id}>
                  {v.legal_name} ({v.vendor_code})
                </option>
              ))}
            </select>
            <label className="inline-flex cursor-pointer items-center gap-2 rounded-lg bg-indigo-600 px-3.5 py-2 text-xs font-medium text-white hover:bg-indigo-500">
              <Upload className="h-4 w-4" />
              {uploading ? `Uploading ${uploadPct ?? 0}%...` : "Upload Financial Document"}
              <input
                type="file"
                accept=".pdf,.png,.jpg,.jpeg,.tif,.tiff,.docx,.xlsx,.csv"
                onChange={handleUpload}
                disabled={uploading}
                className="hidden"
              />
            </label>
          </div>
        )}
      </div>

      {uploadPct !== null && (
        <div className="h-2 w-full overflow-hidden rounded-full bg-slate-800">
          <div className="h-full bg-indigo-500 transition-all" style={{ width: `${uploadPct}%` }} />
        </div>
      )}

      <div className="rounded-xl border border-slate-800 bg-slate-900 overflow-hidden">
        {documents.length === 0 ? (
          <div className="p-8">
            <EmptyBox text="No documents uploaded yet. Upload a PDF, image, DOCX, XLSX, or CSV document to trigger the OCR & extraction pipeline." />
          </div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-left text-xs">
              <thead className="bg-slate-950 text-slate-400">
                <tr>
                  <th className="px-4 py-2.5">Document</th>
                  <th className="px-4 py-2.5">Category</th>
                  <th className="px-4 py-2.5">Processing</th>
                  <th className="px-4 py-2.5">Materialization</th>
                  <th className="px-4 py-2.5">Flags</th>
                  <th className="px-4 py-2.5">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-800/80">
                {documents.map((d) => (
                  <tr key={d.id} className="hover:bg-slate-800/40">
                    <td className="px-4 py-2.5">
                      <div className="font-medium text-slate-100">
                        {d.original_filename}
                        {d.is_synthetic_fixture && (
                          <span className="ml-2 rounded bg-sky-500/20 px-1.5 py-0.5 font-mono text-[10px] text-sky-300">
                            SYNTHETIC FIXTURE
                          </span>
                        )}
                      </div>
                      <div className="font-mono text-[11px] text-slate-500">
                        SHA-256: {d.sha256_hash.slice(0, 16)}… • {(d.file_size_bytes / 1024).toFixed(1)} KB • {d.page_count ?? "?"} pg
                      </div>
                      {d.failure_reason && (
                        <div className="mt-1 text-[11px] text-rose-400">Error: {d.failure_reason}</div>
                      )}
                    </td>
                    <td className="px-4 py-2.5"><StatusBadge value={d.effective_category} /></td>
                    <td className="px-4 py-2.5">
                      <div className="flex flex-wrap gap-1">
                        <StatusBadge value={d.processing_status} />
                        <StatusBadge value={d.ocr_status} />
                      </div>
                    </td>
                    <td className="px-4 py-2.5">
                      {d.materialized_entity_type ? (
                        <span className="rounded bg-emerald-500/15 px-2 py-0.5 font-mono text-[11px] text-emerald-300">
                          {d.materialized_entity_type}
                        </span>
                      ) : (
                        <span className="text-slate-500">Not materialized</span>
                      )}
                    </td>
                    <td className="px-4 py-2.5 space-y-1">
                      {d.is_duplicate_file && (
                        <span className="inline-flex items-center gap-1 rounded bg-rose-500/15 px-1.5 py-0.5 text-[11px] text-rose-300">
                          <AlertTriangle className="h-3 w-3" /> Duplicate SHA-256
                        </span>
                      )}
                      {d.prompt_injection_flagged && (
                        <span className="inline-flex items-center gap-1 rounded bg-amber-500/20 px-1.5 py-0.5 text-[11px] text-amber-300">
                          <ShieldAlert className="h-3 w-3" /> Prompt Injection Blocked
                        </span>
                      )}
                    </td>
                    <td className="px-4 py-2.5">
                      <div className="flex items-center gap-2">
                        <button
                          onClick={() => onOpenDocument(d.id)}
                          className="inline-flex items-center gap-1 rounded border border-indigo-500/40 bg-indigo-500/15 px-2.5 py-1 text-xs text-indigo-200 hover:bg-indigo-500/25"
                        >
                          <Eye className="h-3.5 w-3.5" /> Viewer & Materialize
                        </button>
                        {canWrite && (
                          <button
                            onClick={async () => {
                              try {
                                await api.request(`/documents/${d.id}/reprocess`, { method: "POST" });
                                setBanner({ type: "ok", text: "Document reprocessed." });
                                await onRefresh();
                              } catch (e: any) {
                                setBanner({ type: "err", text: e.message });
                              }
                            }}
                            aria-label="Reprocess document"
                            className="rounded border border-slate-700 p-1 text-slate-400 hover:bg-slate-800 hover:text-white"
                          >
                            <RefreshCw className="h-3.5 w-3.5" />
                          </button>
                        )}
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        <PaginationBar pagination={pagination} onPageChange={onPageChange} />
      </div>
    </div>
  );
}
