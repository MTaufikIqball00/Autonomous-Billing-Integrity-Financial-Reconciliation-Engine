"use client";

import React, { useState } from "react";
import { api, formatMoney, PaginationMeta } from "@/lib/api";
import { StatusBadge } from "../StatusBadge";
import { EmptyBox, PaginationBar } from "./SharedUi";

export function PurchaseOrdersModule({
  purchaseOrders,
  pagination,
  vendors,
  contracts,
  canWrite,
  onRefresh,
  onPageChange,
  setBanner,
}: {
  purchaseOrders: any[];
  pagination?: PaginationMeta;
  vendors: any[];
  contracts: any[];
  canWrite: boolean;
  onRefresh: () => Promise<void>;
  onPageChange: (offset: number) => void;
  setBanner: (b: { type: "ok" | "err"; text: string } | null) => void;
}) {
  const [showCreate, setShowCreate] = useState(false);
  const [poNumber, setPoNumber] = useState("");
  const [vendorId, setVendorId] = useState("");
  const [contractId, setContractId] = useState("");
  const [issueDate, setIssueDate] = useState("2026-08-01");
  const [currency, setCurrency] = useState("USD");
  const [sku, setSku] = useState("SKU-01");
  const [desc, setDesc] = useState("");
  const [qty, setQty] = useState("10");
  const [price, setPrice] = useState("100.00");

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    try {
      await api.request("/purchase-orders", {
        method: "POST",
        body: JSON.stringify({
          po_number: poNumber,
          vendor_id: vendorId,
          contract_id: contractId || null,
          issue_date: issueDate,
          currency,
          line_items: [
            {
              item_code: sku || null,
              description: desc,
              unit: "unit",
              ordered_quantity: qty,
              received_quantity: qty,
              unit_price: price,
            },
          ],
        }),
      });
      setShowCreate(false);
      setPoNumber("");
      setDesc("");
      setBanner({ type: "ok", text: "Purchase order created." });
      await onRefresh();
    } catch (err: any) {
      setBanner({ type: "err", text: err.message });
    }
  };

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h2 className="text-xl font-semibold text-slate-100">Purchase Order Matching</h2>
        {canWrite && (
          <button
            onClick={() => setShowCreate((v) => !v)}
            className="rounded-lg bg-indigo-600 px-3.5 py-2 text-xs font-medium text-white hover:bg-indigo-500"
          >
            {showCreate ? "Hide Form" : "+ New Purchase Order"}
          </button>
        )}
      </div>

      {showCreate && canWrite && (
        <form onSubmit={handleCreate} className="rounded-xl border border-indigo-500/30 bg-slate-900 p-4 space-y-3">
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
            <input required value={poNumber} onChange={(e) => setPoNumber(e.target.value)} placeholder="PO Number" className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
            <select required value={vendorId} onChange={(e) => setVendorId(e.target.value)} className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 text-xs text-slate-100">
              <option value="">Select Vendor...</option>
              {vendors.map((v) => <option key={v.id} value={v.id}>{v.legal_name}</option>)}
            </select>
            <select value={contractId} onChange={(e) => setContractId(e.target.value)} className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 text-xs text-slate-100">
              <option value="">Optional Contract...</option>
              {contracts.map((c) => <option key={c.id} value={c.id}>{c.contract_number}</option>)}
            </select>
            <input required type="date" value={issueDate} onChange={(e) => setIssueDate(e.target.value)} className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
            <input required value={currency} onChange={(e) => setCurrency(e.target.value)} placeholder="USD" className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
          </div>
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-4 border-t border-slate-800 pt-3">
            <input value={sku} onChange={(e) => setSku(e.target.value)} placeholder="SKU" className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
            <input required value={desc} onChange={(e) => setDesc(e.target.value)} placeholder="Item Description" className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 text-xs text-slate-100" />
            <input required value={qty} onChange={(e) => setQty(e.target.value)} placeholder="Qty" className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
            <input required value={price} onChange={(e) => setPrice(e.target.value)} placeholder="Unit Price" className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
          </div>
          <button type="submit" className="rounded-lg bg-indigo-600 px-3.5 py-1.5 text-xs font-medium text-white hover:bg-indigo-500">Save PO</button>
        </form>
      )}

      <div className="rounded-xl border border-slate-800 bg-slate-900 overflow-hidden">
        {purchaseOrders.length === 0 ? (
          <div className="p-8"><EmptyBox text="No purchase orders recorded yet." /></div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-left text-xs">
              <thead className="bg-slate-950 text-slate-400">
                <tr>
                  <th className="px-4 py-2.5">PO #</th>
                  <th className="px-4 py-2.5">Vendor</th>
                  <th className="px-4 py-2.5">Contract #</th>
                  <th className="px-4 py-2.5">Issue Date</th>
                  <th className="px-4 py-2.5 text-right">Total</th>
                  <th className="px-4 py-2.5">Lines</th>
                  <th className="px-4 py-2.5">Status</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-800/80">
                {purchaseOrders.map((po) => (
                  <tr key={po.id}>
                    <td className="px-4 py-2.5 font-mono text-indigo-300">{po.po_number}</td>
                    <td className="px-4 py-2.5 text-slate-200">{po.vendor_name}</td>
                    <td className="px-4 py-2.5 font-mono text-slate-400">{po.contract_number || "—"}</td>
                    <td className="px-4 py-2.5 font-mono text-slate-400">{po.issue_date}</td>
                    <td className="px-4 py-2.5 text-right font-mono tabular-nums">{formatMoney(po.total_amount, po.currency)}</td>
                    <td className="px-4 py-2.5 font-mono">{po.line_count}</td>
                    <td className="px-4 py-2.5"><StatusBadge value={po.status} /></td>
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
