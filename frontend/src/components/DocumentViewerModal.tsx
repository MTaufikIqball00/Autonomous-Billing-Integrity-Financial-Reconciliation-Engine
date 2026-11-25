"use client";

import React, { useEffect, useState } from "react";
import { X, AlertTriangle, CheckCircle2, FileText, Edit3, Layers } from "lucide-react";
import { api } from "@/lib/api";
import { StatusBadge } from "./StatusBadge";

interface Props {
  documentId: string;
  canReview: boolean;
  canWrite?: boolean;
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

const MATERIALIZABLE_CATEGORIES = new Set([
  "invoice",
  "credit_note",
  "debit_note",
  "contract",
  "contract_amendment",
  "purchase_order",
  "payment_record",
]);

export function DocumentViewerModal({ documentId, canReview, canWrite, onClose, onUpdated }: Props) {
  const allowWrite = canWrite ?? canReview;
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

  // Document-to-Entity Materialization Preview & Confirmation state
  const [preview, setPreview] = useState<any>(null);
  const [vendors, setVendors] = useState<any[]>([]);
  const [matVendorId, setMatVendorId] = useState("");
  const [matDocNumber, setMatDocNumber] = useState("");
  const [matCurrency, setMatCurrency] = useState("USD");
  const [matDate, setMatDate] = useState("");
  const [matTotal, setMatTotal] = useState("");
  const [matContractRef, setMatContractRef] = useState("");
  const [matPoRef, setMatPoRef] = useState("");
  const [materializing, setMaterializing] = useState(false);

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

      if (MATERIALIZABLE_CATEGORIES.has(res.document.effective_category)) {
        try {
          const [prev, vResp] = await Promise.all([
            api.request(`/documents/${documentId}/materialization-preview`),
            api.request("/vendors?limit=100&offset=0").catch(() => ({ vendors: [] })),
          ]);
          setPreview(prev);
          setVendors(vResp.vendors || []);
          setMatVendorId(prev.matched_vendor_id || "");
          setMatDocNumber(prev.draft?.document_number || "");
          setMatCurrency(prev.draft?.currency || "USD");
          setMatDate(prev.draft?.document_date || prev.draft?.effective_date || "");
          setMatTotal(prev.draft?.total_amount != null ? String(prev.draft.total_amount) : "");
          setMatContractRef(prev.draft?.contract_number || "");
          setMatPoRef(prev.draft?.po_number || "");
        } catch {
          setPreview(null);
        }
      } else {
        setPreview(null);
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

  const handleMaterialize = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setMaterializing(true);
    try {
      const payload: Record<string, any> = {
        auto_reconcile: true,
      };
      if (matVendorId) payload.vendor_id = matVendorId;
      if (matDocNumber.trim()) payload.document_number = matDocNumber.trim();
      if (matCurrency.trim()) payload.currency = matCurrency.trim().toUpperCase();
      if (matDate.trim()) {
        if (preview?.effective_category === "contract" || preview?.effective_category === "contract_amendment") {
          payload.effective_date = matDate.trim();
        } else {
          payload.document_date = matDate.trim();
        }
      }
      if (matTotal.trim()) payload.total_amount = matTotal.trim();
      if (matContractRef.trim()) payload.contract_number = matContractRef.trim();
      if (matPoRef.trim()) payload.po_number = matPoRef.trim();

      const res = await api.request(`/documents/${documentId}/materialize`, {
        method: "POST",
        body: JSON.stringify(payload),
      });
      const recMsg = res.reconciliation_status
        ? ` Auto-reconciliation: ${res.reconciliation_status}.`
        : "";
      setActionMessage(
        `Materialized into ${res.entity_type} (${res.entity_id}).${recMsg}`
      );
      await loadDetail();
      onUpdated();
    } catch (err: any) {
      setError(err.message);
    } finally {
      setMaterializing(false);
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

            {/* Right: Classification Override, Materialization Bridge & Extracted Fields */}
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

              {/* Document-to-Entity Materialization Panel */}
              {(detail?.document?.materialized_entity_id || preview) && (
                <div className="rounded-lg border border-slate-800 bg-slate-950 p-4 space-y-3">
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <div className="flex items-center gap-2">
                      <Layers className="h-4 w-4 text-indigo-400" />
                      <span className="text-xs font-semibold uppercase tracking-wider text-slate-300">
                        Entity Materialization Bridge
                      </span>
                    </div>
                    {detail?.document?.materialized_entity_id ? (
                      <span className="rounded-full border border-emerald-500/40 bg-emerald-500/15 px-2.5 py-0.5 font-mono text-[11px] text-emerald-300">
                        Materialized: {detail.document.materialized_entity_type} •{" "}
                        {String(detail.document.materialized_entity_id).slice(0, 8)}…
                      </span>
                    ) : (
                      <StatusBadge
                        value={preview?.ready_for_auto_materialize ? "completed" : "requires_review"}
                      />
                    )}
                  </div>

                  {detail?.document?.materialized_entity_id ? (
                    <p className="font-mono text-[11px] text-slate-400">
                      Linked record ID: {detail.document.materialized_entity_id} (
                      {detail.document.materialized_at
                        ? new Date(detail.document.materialized_at).toLocaleString()
                        : "linked"}
                      )
                    </p>
                  ) : (
                    preview && (
                      <>
                        {preview.missing_or_ambiguous_fields?.length > 0 && (
                          <div className="rounded border border-amber-500/30 bg-amber-500/10 p-2.5 text-[11px] text-amber-200">
                            <p className="font-semibold">Fields needing human confirmation:</p>
                            <ul className="mt-1 list-disc pl-4 space-y-0.5 font-mono">
                              {preview.missing_or_ambiguous_fields.map((msg: string, idx: number) => (
                                <li key={idx}>{msg}</li>
                              ))}
                            </ul>
                          </div>
                        )}

                        {allowWrite && (
                          <form onSubmit={handleMaterialize} className="grid grid-cols-2 gap-2 text-xs sm:grid-cols-3">
                            <select
                              aria-label="Materialize vendor"
                              required
                              value={matVendorId}
                              onChange={(e) => setMatVendorId(e.target.value)}
                              className="rounded border border-slate-700 bg-slate-900 px-2 py-1.5 text-slate-100"
                            >
                              <option value="">Select vendor...</option>
                              {vendors.map((v) => (
                                <option key={v.id} value={v.id}>
                                  {v.vendor_code} — {v.legal_name}
                                </option>
                              ))}
                            </select>
                            <input
                              type="text"
                              required
                              placeholder="Document / Invoice #"
                              value={matDocNumber}
                              onChange={(e) => setMatDocNumber(e.target.value)}
                              className="rounded border border-slate-700 bg-slate-900 px-2 py-1.5 text-slate-100"
                            />
                            <input
                              type="date"
                              required
                              value={matDate}
                              onChange={(e) => setMatDate(e.target.value)}
                              className="rounded border border-slate-700 bg-slate-900 px-2 py-1.5 font-mono text-slate-100"
                            />
                            <input
                              type="text"
                              required
                              maxLength={3}
                              placeholder="Currency (USD)"
                              value={matCurrency}
                              onChange={(e) => setMatCurrency(e.target.value.toUpperCase())}
                              className="rounded border border-slate-700 bg-slate-900 px-2 py-1.5 font-mono uppercase text-slate-100"
                            />
                            <input
                              type="text"
                              placeholder="Total Amount"
                              value={matTotal}
                              onChange={(e) => setMatTotal(e.target.value)}
                              className="rounded border border-slate-700 bg-slate-900 px-2 py-1.5 font-mono text-slate-100"
                            />
                            <input
                              type="text"
                              placeholder="Contract Ref (opt)"
                              value={matContractRef}
                              onChange={(e) => setMatContractRef(e.target.value)}
                              className="rounded border border-slate-700 bg-slate-900 px-2 py-1.5 font-mono text-slate-100"
                            />
                            <input
                              type="text"
                              placeholder="PO Ref (opt)"
                              value={matPoRef}
                              onChange={(e) => setMatPoRef(e.target.value)}
                              className="rounded border border-slate-700 bg-slate-900 px-2 py-1.5 font-mono text-slate-100"
                            />
                            <button
                              type="submit"
                              disabled={materializing}
                              className="col-span-2 rounded bg-indigo-600 px-3 py-1.5 font-medium text-white hover:bg-indigo-500 disabled:opacity-50 sm:col-span-2"
                            >
                              {materializing
                                ? "Materializing & Reconciling..."
                                : "Confirm & Materialize Record"}
                            </button>
                          </form>
                        )}
                      </>
                    )
                  )}
                </div>
              )}

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
