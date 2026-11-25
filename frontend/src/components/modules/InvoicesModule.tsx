"use client";

import React, { useState } from "react";
import { api, formatMoney, PaginationMeta } from "@/lib/api";
import { StatusBadge } from "../StatusBadge";
import { EmptyBox, PaginationBar } from "./SharedUi";

export function InvoicesModule({
  invoices,
  pagination,
  vendors,
  canWrite,
  onRefresh,
  onPageChange,
  onOpenInvoice,
  setBanner,
}: {
  invoices: any[];
  pagination?: PaginationMeta;
  vendors: any[];
  canWrite: boolean;
  onRefresh: () => Promise<void>;
  onPageChange: (offset: number) => void;
  onOpenInvoice: (invId: string) => void;
  setBanner: (b: { type: "ok" | "err"; text: string } | null) => void;
}) {
  const [showCreate, setShowCreate] = useState(false);
  const [invNum, setInvNum] = useState("");
  const [vendorId, setVendorId] = useState("");
  const [invDate, setInvDate] = useState("2026-08-15");
  const [currency, setCurrency] = useState("USD");
  const [contractRef, setContractRef] = useState("");
  const [poRef, setPoRef] = useState("");
  const [taxTotal, setTaxTotal] = useState("0");
  const [sku, setSku] = useState("SKU-01");
  const [desc, setDesc] = useState("");
  const [qty, setQty] = useState("1");
  const [unitPrice, setUnitPrice] = useState("100.00");

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    try {
      // Send strings directly to Rust Decimal parser — zero float math!
      const res = await api.request("/invoices", {
        method: "POST",
        body: JSON.stringify({
          invoice_number: invNum,
          vendor_id: vendorId,
          invoice_date: invDate,
          currency,
          referenced_contract_number: contractRef || null,
          referenced_po_number: poRef || null,
          stated_subtotal: unitPrice,
          stated_tax_total: taxTotal || "0",
          stated_total: unitPrice,
          auto_reconcile: true,
          line_items: [
            {
              item_code: sku || null,
              description: desc,
              quantity: qty,
              unit: "unit",
              unit_price: unitPrice,
              tax_amount: taxTotal || "0",
              stated_line_subtotal: unitPrice,
              stated_line_total: unitPrice,
            },
          ],
        }),
      });
      setShowCreate(false);
      setInvNum("");
      setDesc("");
      setBanner({
        type: "ok",
        text: `Invoice created & reconciled (${res.reconciliation?.overall_status ?? "queued"}).`,
      });
      await onRefresh();
    } catch (err: any) {
      setBanner({ type: "err", text: err.message });
    }
  };

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-xl font-semibold text-slate-100">Invoice Management & Reconciliation</h2>
          <p className="text-xs text-slate-400">
            Click &ldquo;Reconcile Workbench&rdquo; on any invoice to inspect Contract vs Invoice values, Stage A–L deterministic checks, and supporting evidence.
          </p>
        </div>
        {canWrite && (
          <button
            onClick={() => setShowCreate((v) => !v)}
            className="rounded-lg bg-indigo-600 px-3.5 py-2 text-xs font-medium text-white hover:bg-indigo-500"
          >
            {showCreate ? "Hide Form" : "+ New Invoice"}
          </button>
        )}
      </div>

      {showCreate && canWrite && (
        <form onSubmit={handleCreate} className="rounded-xl border border-indigo-500/30 bg-slate-900 p-4 space-y-3">
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
            <input required value={invNum} onChange={(e) => setInvNum(e.target.value)} placeholder="Invoice # (INV-2026-01)" className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
            <select required value={vendorId} onChange={(e) => setVendorId(e.target.value)} className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 text-xs text-slate-100">
              <option value="">Select Vendor...</option>
              {vendors.map((v) => <option key={v.id} value={v.id}>{v.legal_name}</option>)}
            </select>
            <input required type="date" value={invDate} onChange={(e) => setInvDate(e.target.value)} className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
            <input required value={currency} onChange={(e) => setCurrency(e.target.value)} placeholder="Currency (USD)" className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
            <input value={contractRef} onChange={(e) => setContractRef(e.target.value)} placeholder="Contract Ref (optional)" className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
            <input value={poRef} onChange={(e) => setPoRef(e.target.value)} placeholder="PO Ref (optional)" className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
          </div>
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-5 border-t border-slate-800 pt-3">
            <input value={sku} onChange={(e) => setSku(e.target.value)} placeholder="SKU" className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
            <input required value={desc} onChange={(e) => setDesc(e.target.value)} placeholder="Line Item Description" className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 text-xs text-slate-100 sm:col-span-2" />
            <input required value={qty} onChange={(e) => setQty(e.target.value)} placeholder="Qty" className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
            <input required value={unitPrice} onChange={(e) => setUnitPrice(e.target.value)} placeholder="Unit / Line Total" className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
          </div>
          <div className="flex items-center gap-3">
            <input value={taxTotal} onChange={(e) => setTaxTotal(e.target.value)} placeholder="Tax amount (0)" className="w-40 rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
            <button type="submit" className="rounded-lg bg-indigo-600 px-3.5 py-1.5 text-xs font-medium text-white hover:bg-indigo-500">
              Create & Reconcile
            </button>
          </div>
        </form>
      )}

      <div className="rounded-xl border border-slate-800 bg-slate-900 overflow-hidden">
        {invoices.length === 0 ? (
          <div className="p-8"><EmptyBox text="No invoices recorded yet." /></div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-left text-xs">
              <thead className="bg-slate-950 text-slate-400">
                <tr>
                  <th className="px-4 py-2.5">Invoice #</th>
                  <th className="px-4 py-2.5">Vendor</th>
                  <th className="px-4 py-2.5">Date</th>
                  <th className="px-4 py-2.5 text-right">Stated Total</th>
                  <th className="px-4 py-2.5 text-right">Recalculated</th>
                  <th className="px-4 py-2.5">Status</th>
                  <th className="px-4 py-2.5">Findings</th>
                  <th className="px-4 py-2.5">Action</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-800/80">
                {invoices.map((inv) => (
                  <tr key={inv.id} className="hover:bg-slate-800/40">
                    <td className="px-4 py-2.5 font-mono text-indigo-300">{inv.invoice_number}</td>
                    <td className="px-4 py-2.5 text-slate-200">{inv.vendor_name}</td>
                    <td className="px-4 py-2.5 font-mono text-slate-400">{inv.invoice_date}</td>
                    <td className="px-4 py-2.5 text-right font-mono tabular-nums text-slate-100">
                      {formatMoney(inv.stated_total, inv.currency)}
                    </td>
                    <td className="px-4 py-2.5 text-right font-mono tabular-nums text-emerald-300">
                      {formatMoney(inv.recalculated_total, inv.currency)}
                    </td>
                    <td className="px-4 py-2.5"><StatusBadge value={inv.reconciliation_status} /></td>
                    <td className="px-4 py-2.5 font-mono">{inv.findings_count}</td>
                    <td className="px-4 py-2.5">
                      <button
                        onClick={() => onOpenInvoice(inv.id)}
                        className="rounded border border-indigo-500/40 bg-indigo-500/15 px-2.5 py-1 text-xs text-indigo-200 hover:bg-indigo-500/25"
                      >
                        Reconcile Workbench
                      </button>
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
