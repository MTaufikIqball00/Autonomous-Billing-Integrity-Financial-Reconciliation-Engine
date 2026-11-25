"use client";

import React, { useState } from "react";
import { api, PaginationMeta } from "@/lib/api";
import { StatusBadge } from "../StatusBadge";
import { EmptyBox, PaginationBar } from "./SharedUi";

export function VendorsModule({
  vendors,
  pagination,
  canWrite,
  onRefresh,
  onPageChange,
  setBanner,
}: {
  vendors: any[];
  pagination?: PaginationMeta;
  canWrite: boolean;
  onRefresh: () => Promise<void>;
  onPageChange: (offset: number) => void;
  setBanner: (b: { type: "ok" | "err"; text: string } | null) => void;
}) {
  const [vCode, setVCode] = useState("");
  const [vName, setVName] = useState("");
  const [vTax, setVTax] = useState("");
  const [vCurrency, setVCurrency] = useState("USD");
  const [vTerms, setVTerms] = useState("Net 30");

  const [editingId, setEditingId] = useState<string | null>(null);
  const [editName, setEditName] = useState("");
  const [editCurrency, setEditCurrency] = useState("");
  const [editStatus, setEditStatus] = useState("active");

  const handleCreateVendor = async (e: React.FormEvent) => {
    e.preventDefault();
    try {
      await api.request("/vendors", {
        method: "POST",
        body: JSON.stringify({
          vendor_code: vCode,
          legal_name: vName,
          tax_id: vTax || null,
          default_currency: vCurrency,
          payment_terms: vTerms || null,
        }),
      });
      setVCode("");
      setVName("");
      setVTax("");
      setBanner({ type: "ok", text: "Vendor created." });
      await onRefresh();
    } catch (err: any) {
      setBanner({ type: "err", text: err.message });
    }
  };

  const handleSaveEdit = async (id: string) => {
    try {
      await api.request(`/vendors/${id}`, {
        method: "PATCH",
        body: JSON.stringify({
          legal_name: editName,
          default_currency: editCurrency,
          status: editStatus,
        }),
      });
      setEditingId(null);
      setBanner({ type: "ok", text: "Vendor updated." });
      await onRefresh();
    } catch (err: any) {
      setBanner({ type: "err", text: err.message });
    }
  };

  return (
    <div className="space-y-5">
      <h2 className="text-xl font-semibold text-slate-100">Vendor Management</h2>
      {canWrite && (
        <form onSubmit={handleCreateVendor} className="flex flex-wrap items-end gap-3 rounded-xl border border-slate-800 bg-slate-900 p-4">
          <div>
            <label className="block text-[11px] text-slate-400">Vendor Code</label>
            <input required value={vCode} onChange={(e) => setVCode(e.target.value)} placeholder="VEND-100" className="mt-1 rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 text-xs text-slate-100" />
          </div>
          <div className="flex-1 min-w-[200px]">
            <label className="block text-[11px] text-slate-400">Legal Name</label>
            <input required value={vName} onChange={(e) => setVName(e.target.value)} placeholder="Acme Enterprise Services Ltd." className="mt-1 w-full rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 text-xs text-slate-100" />
          </div>
          <div>
            <label className="block text-[11px] text-slate-400">Tax ID</label>
            <input value={vTax} onChange={(e) => setVTax(e.target.value)} placeholder="US-12-3456" className="mt-1 w-28 rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
          </div>
          <div>
            <label className="block text-[11px] text-slate-400">Currency</label>
            <input value={vCurrency} onChange={(e) => setVCurrency(e.target.value)} className="mt-1 w-20 rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
          </div>
          <div>
            <label className="block text-[11px] text-slate-400">Terms</label>
            <input value={vTerms} onChange={(e) => setVTerms(e.target.value)} className="mt-1 w-24 rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 text-xs text-slate-100" />
          </div>
          <button type="submit" className="rounded-lg bg-indigo-600 px-3.5 py-1.5 text-xs font-medium text-white hover:bg-indigo-500">
            Add Vendor
          </button>
        </form>
      )}
      <div className="rounded-xl border border-slate-800 bg-slate-900 overflow-hidden">
        {vendors.length === 0 ? (
          <div className="p-8"><EmptyBox text="No vendors registered yet." /></div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-left text-xs">
              <thead className="bg-slate-950 text-slate-400">
                <tr>
                  <th className="px-4 py-2.5">Code</th>
                  <th className="px-4 py-2.5">Legal Name</th>
                  <th className="px-4 py-2.5">Tax ID</th>
                  <th className="px-4 py-2.5">Currency</th>
                  <th className="px-4 py-2.5">Status</th>
                  <th className="px-4 py-2.5">Contracts / Invoices</th>
                  <th className="px-4 py-2.5">Open Findings</th>
                  {canWrite && <th className="px-4 py-2.5">Edit</th>}
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-800/80">
                {vendors.map((v) => (
                  <React.Fragment key={v.id}>
                    <tr>
                      <td className="px-4 py-2.5 font-mono text-indigo-300">{v.vendor_code}</td>
                      <td className="px-4 py-2.5 font-medium text-slate-100">{v.legal_name}</td>
                      <td className="px-4 py-2.5 font-mono text-slate-400">{v.tax_id || "—"}</td>
                      <td className="px-4 py-2.5 font-mono">{v.default_currency || "—"}</td>
                      <td className="px-4 py-2.5"><StatusBadge value={v.status} /></td>
                      <td className="px-4 py-2.5 font-mono">{v.contract_count} / {v.invoice_count}</td>
                      <td className="px-4 py-2.5 font-mono text-amber-300">{v.open_findings}</td>
                      {canWrite && (
                        <td className="px-4 py-2.5">
                          <button
                            onClick={() => {
                              setEditingId(v.id);
                              setEditName(v.legal_name);
                              setEditCurrency(v.default_currency || "USD");
                              setEditStatus(v.status || "active");
                            }}
                            className="rounded border border-slate-700 px-2 py-0.5 text-[11px] text-slate-200 hover:bg-slate-800"
                          >
                            Edit
                          </button>
                        </td>
                      )}
                    </tr>
                    {editingId === v.id && (
                      <tr className="bg-slate-950">
                        <td colSpan={8} className="px-4 py-2.5">
                          <div className="flex flex-wrap items-center gap-2">
                            <input value={editName} onChange={(e) => setEditName(e.target.value)} className="min-w-[200px] flex-1 rounded border border-slate-700 bg-slate-900 px-2 py-1 text-xs text-slate-100" />
                            <input value={editCurrency} onChange={(e) => setEditCurrency(e.target.value)} className="w-20 rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono text-xs text-slate-100" />
                            <select value={editStatus} onChange={(e) => setEditStatus(e.target.value)} className="rounded border border-slate-700 bg-slate-900 px-2 py-1 text-xs text-slate-100">
                              <option value="active">active</option>
                              <option value="inactive">inactive</option>
                              <option value="suspended">suspended</option>
                            </select>
                            <button onClick={() => handleSaveEdit(v.id)} className="rounded bg-indigo-600 px-2.5 py-1 text-xs text-white">Save</button>
                            <button onClick={() => setEditingId(null)} className="rounded border border-slate-700 px-2.5 py-1 text-xs text-slate-400">Cancel</button>
                          </div>
                        </td>
                      </tr>
                    )}
                  </React.Fragment>
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
