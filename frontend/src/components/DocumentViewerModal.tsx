"use client";

import React, { useEffect, useState } from "react";
import { X, AlertTriangle, CheckCircle2, FileText, Edit3 } from "lucide-react";
import { api } from "@/lib/api";
import { StatusBadge } from "./StatusBadge";

interface Props {
  documentId: string;
  canReview: boolean;
  onClose: () => void;
  onUpdated: () => void;
}

const CATEGORIES = [
  "contract",
  "contract_amendment",
  "invoice",
  "purchase_order",
  "delivery_receipt",
  "service_acceptance",
  "payment_record",
  "credit_note",
  "debit_note",
  "other_supporting",
  "unknown",
];

export function DocumentViewerModal({ documentId, canReview, onClose, onUpdated }: Props) {
  const [detail, setDetail] = useState<any>(null);
  const [pageData, setPageData] = useState<any>(null);
  const [pageNum, setPageNum] = useState(1);
  const [selectedFieldId, setSelectedFieldId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Manual reclassification state
  const [newCategory, setNewCategory] = useState("");
  const [classifyReason, setClassifyReason] = useState("");

  // Inline field correction state (preserves original AI output!)
  const [editingFieldId, setEditingFieldId] = useState<string | null>(null);
  const [correctedValue, setCorrectedValue] = useState("");
  const [correctionReason, setCorrectionReason] = useState("");
  const [actionMessage, setActionMessage] = useState<string | null>(null);

  const loadDetail = async () => {
    setLoading(true);
    setError(null);
    try {
      const res = await api.request(`/documents/${documentId}`);
      setDetail(res);
      setNewCategory(res.document.effective_category);
      const ext = (res.document.file_extension || "").toLowerCase();
      if (["pdf", "png", "jpg", "jpeg", "tif", "tiff"].includes(ext)) {
        try {
          const pg = await api.request(`/documents/${documentId}/page/${pageNum}`);
          setPageData(pg);
        } catch {
          setPageData(null);
        }
      }
    } catch (e: any) {
      setError(e.message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadDetail();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [documentId, pageNum]);

  const handleReclassify = async () => {
    if (!classifyReason.trim()) {
      setError("Please provide an audit reason before changing document classification.");
      return;
    }
    try {
      await api.request(`/documents/${documentId}/classify`, {
        method: "PATCH",
        body: JSON.stringify({ category: newCategory, reason: classifyReason }),
      });
      setActionMessage("Document classification updated (original AI prediction preserved).");
      setClassifyReason("");
      await loadDetail();
      onUpdated();
    } catch (e: any) {
      setError(e.message);
    }
  };

  const handleCorrectField = async (fieldId: string) => {
    if (!correctedValue.trim() || !correctionReason.trim()) {
      setError("Both corrected value and audit reason are required.");
      return;
    }
    try {
      await api.request(`/extracted-fields/${fieldId}/correct`, {
        method: "PATCH",
        body: JSON.stringify({ corrected_value: correctedValue, reason: correctionReason }),
      });
      setActionMessage("Field correction saved. Original extracted value preserved immutably.");
      setEditingFieldId(null);
      setCorrectedValue("");
      setCorrectionReason("");
      await loadDetail();
      onUpdated();
    } catch (e: any) {
      setError(e.message);
    }
  };

  const fields: any[] = detail?.extracted_fields || [];
  const currentPageBoxes = fields.filter(
    (f) => f.bounding_box && (f.page_number || 1) === pageNum
  );

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/75 p-4 backdrop-blur-sm"
      role="dialog"
      aria-modal="true"
      aria-label="Document Evidence Viewer"
    >
      <div className="flex max-h-[92vh] w-full max-w-7xl flex-col overflow-hidden rounded-xl border border-slate-800 bg-slate-900 shadow-2xl">
        {/* Header */}
        <div className="flex items-center justify-between border-b border-slate-800 px-5 py-3.5">
          <div className="flex items-center gap-3 min-w-0">
            <FileText className="h-5 w-5 text-indigo-400 shrink-0" />
            <div className="min-w-0">
              <h3 className="truncate text-base font-semibold text-slate-100">
                {detail?.document?.original_filename || "Loading Document..."}
              </h3>
              {detail?.document && (
                <p className="truncate font-mono text-xs text-slate-400">
                  SHA-256: {detail.document.sha256_hash} • {detail.document.detected_mime_type}
                </p>
              )}
            </div>
          </div>
          <button
            onClick={onClose}
            aria-label="Close document viewer"
            className="rounded-lg p-1.5 text-slate-400 hover:bg-slate-800 hover:text-white focus-visible:ring-2 focus-visible:ring-indigo-400"
          >
            <X className="h-5 w-5" />
          </button>
        </div>

        {error && (
          <div className="mx-5 mt-3 rounded-lg border border-rose-500/40 bg-rose-500/10 px-3 py-2 text-xs text-rose-200">
            {error}
          </div>
        )}
        {actionMessage && (
          <div className="mx-5 mt-3 flex items-center gap-2 rounded-lg border border-emerald-500/40 bg-emerald-500/10 px-3 py-2 text-xs text-emerald-200">
            <CheckCircle2 className="h-4 w-4" />
            {actionMessage}
          </div>
        )}

        {loading ? (
          <div className="p-12 text-center text-sm text-slate-400">
            Loading original document & extraction provenance...
          </div>
        ) : (
          <div className="grid flex-1 grid-cols-1 gap-4 overflow-y-auto p-5 lg:grid-cols-12">
            {/* Left: Original Page with Bounding Box Highlights */}
            <div className="flex flex-col rounded-lg border border-slate-800 bg-slate-950 p-3 lg:col-span-6">
              <div className="mb-2 flex items-center justify-between">
                <span className="text-xs font-medium uppercase tracking-wider text-slate-400">
                  Original Document & Source Evidence Overlay
                </span>
                {pageData && pageData.page_count > 1 && (
                  <div className="flex items-center gap-2 text-xs">
                    <button
                      disabled={pageNum <= 1}
                      onClick={() => setPageNum((p) => Math.max(1, p - 1))}
                      className="rounded border border-slate-700 px-2 py-0.5 text-slate-300 disabled:opacity-40"
                    >
                      Prev
                    </button>
                    <span className="tabular-nums text-slate-400">
                      Page {pageNum} / {pageData.page_count}
                    </span>
                    <button
                      disabled={pageNum >= pageData.page_count}
                      onClick={() => setPageNum((p) => Math.min(pageData.page_count, p + 1))}
                      className="rounded border border-slate-700 px-2 py-0.5 text-slate-300 disabled:opacity-40"
                    >
                      Next
                    </button>
                  </div>
                )}
              </div>

              {detail?.document?.prompt_injection_flagged && (
                <div className="mb-3 flex items-start gap-2 rounded-lg border border-rose-500/40 bg-rose-500/10 p-2.5 text-xs text-rose-200">
                  <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-rose-400" />
                  <div>
                    <p className="font-semibold">Prompt Injection Signal Quarantined</p>
                    <p className="text-rose-300/90">
                      Untrusted instructions detected inside this document were isolated and blocked from altering workflow decisions.
                    </p>
                  </div>
                </div>
              )}

              {pageData ? (
                <div className="relative overflow-auto rounded border border-slate-800 bg-white">
                  <img
                    src={`data:image/png;base64,${pageData.data_base64}`}
                    alt={`Page ${pageNum}`}
                    className="block w-full h-auto"
                  />
                  <svg
                    viewBox={`0 0 ${pageData.coord_width} ${pageData.coord_height}`}
                    className="pointer-events-none absolute inset-0 h-full w-full"
                    preserveAspectRatio="none"
                  >
                    {currentPageBoxes.map((f) => {
                      const b = f.bounding_box;
                      const isSel = f.id === selectedFieldId;
                      return (
                        <rect
                          key={f.id}
                          x={b.x0}
                          y={b.y0}
                          width={Math.max(2, b.x1 - b.x0)}
                          height={Math.max(2, b.y1 - b.y0)}
                          fill={isSel ? "rgba(99,102,241,0.32)" : "rgba(16,185,129,0.14)"}
                          stroke={isSel ? "#6366f1" : "#10b981"}
                          strokeWidth={isSel ? 2.5 : 1.2}
                        />
                      );
                    })}
                  </svg>
                </div>
              ) : (
                <pre className="max-h-[480px] overflow-auto whitespace-pre-wrap rounded border border-slate-800 bg-slate-900 p-3 font-mono text-xs text-slate-300">
                  {detail?.extraction?.raw_text || "No extracted text recorded yet."}
                </pre>
              )}
            </div>

            {/* Right: Classification Override & Extracted Fields Table */}
            <div className="flex flex-col gap-4 lg:col-span-6">
              {/* Classification Card */}
              <div className="rounded-lg border border-slate-800 bg-slate-950 p-4">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <div>
                    <p className="text-xs uppercase tracking-wider text-slate-400">
                      Document Classification
                    </p>
                    <div className="mt-1 flex flex-wrap items-center gap-2 text-xs">
                      <span className="text-slate-400">AI:</span>
                      <StatusBadge value={detail?.document?.ai_category} />
                      <span className="text-slate-400">Effective:</span>
                      <StatusBadge value={detail?.document?.effective_category} />
                      <span className="text-slate-500">
                        Confidence:{" "}
                        {detail?.document?.ai_classification_confidence ?? "N/A (Deterministic)"}
                      </span>
                    </div>
                  </div>
                </div>
                {canReview && (
                  <div className="mt-3 flex flex-wrap items-center gap-2 border-t border-slate-800 pt-3">
                    <select
                      aria-label="Manual document category"
                      value={newCategory}
                      onChange={(e) => setNewCategory(e.target.value)}
                      className="rounded border border-slate-700 bg-slate-900 px-2.5 py-1.5 text-xs text-slate-200"
                    >
                      {CATEGORIES.map((c) => (
                        <option key={c} value={c}>
                          {c.replace(/_/g, " ")}
                        </option>
                      ))}
                    </select>
                    <input
                      type="text"
                      placeholder="Audit reason for reclassification..."
                      value={classifyReason}
                      onChange={(e) => setClassifyReason(e.target.value)}
                      className="min-w-[180px] flex-1 rounded border border-slate-700 bg-slate-900 px-2.5 py-1.5 text-xs text-slate-200 placeholder-slate-500"
                    />
                    <button
                      onClick={handleReclassify}
                      className="rounded bg-indigo-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-indigo-500"
                    >
                      Save Classification
                    </button>
                  </div>
                )}
              </div>

              {/* Extracted Fields with Provenance & Inline Human Correction */}
              <div className="flex-1 overflow-hidden rounded-lg border border-slate-800 bg-slate-950">
                <div className="border-b border-slate-800 px-4 py-2.5 flex items-center justify-between">
                  <span className="text-xs font-medium uppercase tracking-wider text-slate-400">
                    Extracted Fields ({fields.length}) — Click row to highlight bbox
                  </span>
                  {detail?.extraction && (
                    <span className="font-mono text-[11px] text-slate-500">
                      {detail.extraction.extraction_method} • {detail.extraction.model_version}
                    </span>
                  )}
                </div>
                <div className="max-h-[420px] overflow-auto">
                  {fields.length === 0 ? (
                    <p className="p-6 text-center text-xs text-slate-500">
                      No structured fields extracted for this document.
                    </p>
                  ) : (
                    <table className="w-full text-left text-xs">
                      <thead className="sticky top-0 bg-slate-900 text-slate-400">
                        <tr>
                          <th className="px-3 py-2">Field</th>
                          <th className="px-3 py-2">Extracted (Immutable)</th>
                          <th className="px-3 py-2">Normalized / Override</th>
                          <th className="px-3 py-2">Conf.</th>
                          <th className="px-3 py-2">Status</th>
                          {canReview && <th className="px-3 py-2">Action</th>}
                        </tr>
                      </thead>
                      <tbody className="divide-y divide-slate-800/80">
                        {fields.map((f) => (
                          <React.Fragment key={f.id}>
                            <tr
                              onClick={() => setSelectedFieldId(f.id)}
                              className={`cursor-pointer transition-colors ${
                                selectedFieldId === f.id
                                  ? "bg-indigo-500/15"
                                  : "hover:bg-slate-900/70"
                              }`}
                            >
                              <td className="px-3 py-2 font-mono text-slate-200">
                                {f.field_name}
                                {f.line_item_index ? ` #${f.line_item_index}` : ""}
                                <div className="text-[10px] text-slate-500">
                                  {f.source_location || `p.${f.page_number ?? "?"}`}
                                </div>
                              </td>
                              <td className="px-3 py-2 font-mono text-slate-300">
                                {f.extracted_value ?? <span className="text-slate-500">null</span>}
                              </td>
                              <td className="px-3 py-2 font-mono">
                                {f.human_override_value ? (
                                  <div>
                                    <span className="text-indigo-300 font-semibold">
                                      {f.human_override_value}
                                    </span>
                                    <div className="text-[10px] text-slate-500">
                                      Human override ({f.human_override_reason})
                                    </div>
                                  </div>
                                ) : (
                                  <span className="text-emerald-300">
                                    {f.normalized_value || "—"}
                                  </span>
                                )}
                              </td>
                              <td className="px-3 py-2 tabular-nums text-slate-400">
                                {f.provider_confidence !== null && f.provider_confidence !== undefined
                                  ? `${(Number(f.provider_confidence) * 100).toFixed(1)}%`
                                  : "N/A"}
                              </td>
                              <td className="px-3 py-2">
                                <StatusBadge value={f.validation_status} />
                              </td>
                              {canReview && (
                                <td className="px-3 py-2">
                                  <button
                                    onClick={(e) => {
                                      e.stopPropagation();
                                      setEditingFieldId(f.id);
                                      setCorrectedValue(
                                        f.human_override_value || f.extracted_value || ""
                                      );
                                    }}
                                    className="inline-flex items-center gap-1 rounded border border-slate-700 px-2 py-0.5 text-[11px] text-slate-300 hover:bg-slate-800"
                                  >
                                    <Edit3 className="h-3 w-3" /> Correct
                                  </button>
                                </td>
                              )}
                            </tr>
                            {editingFieldId === f.id && (
                              <tr className="bg-slate-900">
                                <td colSpan={6} className="px-3 py-2.5">
                                  <div className="flex flex-wrap items-center gap-2">
                                    <input
                                      type="text"
                                      value={correctedValue}
                                      onChange={(e) => setCorrectedValue(e.target.value)}
                                      placeholder="Corrected value"
                                      className="rounded border border-slate-700 bg-slate-950 px-2 py-1 text-xs text-slate-100"
                                    />
                                    <input
                                      type="text"
                                      value={correctionReason}
                                      onChange={(e) => setCorrectionReason(e.target.value)}
                                      placeholder="Audit reason (required)..."
                                      className="flex-1 rounded border border-slate-700 bg-slate-950 px-2 py-1 text-xs text-slate-100"
                                    />
                                    <button
                                      onClick={() => handleCorrectField(f.id)}
                                      className="rounded bg-indigo-600 px-2.5 py-1 text-xs text-white hover:bg-indigo-500"
                                    >
                                      Save Override
                                    </button>
                                    <button
                                      onClick={() => setEditingFieldId(null)}
                                      className="rounded border border-slate-700 px-2 py-1 text-xs text-slate-400"
                                    >
                                      Cancel
                                    </button>
                                  </div>
                                </td>
                              </tr>
                            )}
                          </React.Fragment>
                        ))}
                      </tbody>
                    </table>
                  )}
                </div>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
