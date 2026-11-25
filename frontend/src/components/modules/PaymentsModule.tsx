"use client";

import React, { useState } from "react";
import { api, formatMoney, PaginationMeta } from "@/lib/api";
import { StatusBadge } from "../StatusBadge";
import { EmptyBox, PaginationBar } from "./SharedUi";

export function PaymentsModule({
  payments,
  pagination,
  invoices,
  vendors,
  canWrite,
  canReview,
  onRefresh,
  onPageChange,
  setBanner,
}: {
  payments: any[];
  pagination?: PaginationMeta;
  invoices: any[];
  vendors: any[];
  canWrite: boolean;
  canReview: boolean;
  onRefresh: () => Promise<void>;
  onPageChange: (offset: number) => void;
  setBanner: (b: { type: "ok" | "err"; text: string } | null) => void;
}) {
  const [showCreate, setShowCreate] = useState(false);
  const [pRef, setPRef] = useState("");
  const [bankRef, setBankRef] = useState("");
  const [vendorId, setVendorId] = useState("");
  const [pDate, setPDate] = useState("2026-08-20");
  const [amount, setAmount] = useState("100.00");
  const [currency, setCurrency] = useState("USD");
  const [relInv, setRelInv] = useState("");

  const [allocPayId, setAllocPayId] = useState<string | null>(null);
  const [allocInvId, setAllocInvId] = useState("");
  const [allocAmount, setAllocAmount] = useState("");
  const [allocEvidence, setAllocEvidence] = useState("");

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    const vendor = vendors.find((v) => v.id === vendorId);
    try {
      await api.request("/payments", {
        method: "POST",
        body: JSON.stringify({
          payment_reference: pRef,
          bank_reference: bankRef || null,
          vendor_id: vendorId,
          beneficiary_name: vendor?.legal_name || "Vendor",
          payment_date: pDate,
          amount,
          currency,
          referenced_invoice_number: relInv || null,
        }),
      });
      setShowCreate(false);
      setPRef("");
      setBanner({ type: "ok", text: "Payment record created." });
      await onRefresh();
    } catch (err: any) {
      setBanner({ type: "err", text: err.message });
    }
  };

  const handleAllocatePayment = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!allocPayId) return;
    try {
      await api.request(`/payments/${allocPayId}/allocate`, {
        method: "POST",
        body: JSON.stringify({
          invoice_id: allocInvId,
          allocated_amount: allocAmount,
          matching_evidence: allocEvidence,
        }),
      });
      setAllocPayId(null);
      setAllocInvId("");
      setAllocAmount("");
      setAllocEvidence("");
      setBanner({ type: "ok", text: "Payment allocated with explicit evidence." });
      await onRefresh();
    } catch (err: any) {
      setBanner({ type: "err", text: err.message });
    }
  };

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-xl font-semibold text-slate-100">Payment Reconciliation</h2>
          <p className="text-xs text-slate-400">
            Requires explicit matching evidence (invoice reference, vendor, currency). Never matches solely on equal amounts; unmatched payments remain visible for manual investigation.
          </p>
        </div>
        {canWrite && (
          <button
            onClick={() => setShowCreate((v) => !v)}
            className="rounded-lg bg-indigo-600 px-3.5 py-2 text-xs font-medium text-white hover:bg-indigo-500"
          >
            {showCreate ? "Hide Form" : "+ Record Payment"}
          </button>
        )}
      </div>

      {showCreate && canWrite && (
        <form onSubmit={handleCreate} className="rounded-xl border border-indigo-500/30 bg-slate-900 p-4 grid grid-cols-1 gap-3 sm:grid-cols-3">
          <input required value={pRef} onChange={(e) => setPRef(e.target.value)} placeholder="Payment Ref (PAY-2026-01)" className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
          <input value={bankRef} onChange={(e) => setBankRef(e.target.value)} placeholder="Bank Wire Ref" className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
          <select required value={vendorId} onChange={(e) => setVendorId(e.target.value)} className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 text-xs text-slate-100">
            <option value="">Select Vendor...</option>
            {vendors.map((v) => <option key={v.id} value={v.id}>{v.legal_name}</option>)}
          </select>
          <input required type="date" value={pDate} onChange={(e) => setPDate(e.target.value)} className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
          <input required value={amount} onChange={(e) => setAmount(e.target.value)} placeholder="Amount" className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
          <input required value={currency} onChange={(e) => setCurrency(e.target.value)} placeholder="Currency" className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
          <input value={relInv} onChange={(e) => setRelInv(e.target.value)} placeholder="Referenced Invoice # (optional)" className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100 sm:col-span-2" />
          <button type="submit" className="rounded-lg bg-indigo-600 px-3.5 py-1.5 text-xs font-medium text-white hover:bg-indigo-500">Save Payment</button>
        </form>
      )}

      <div className="rounded-xl border border-slate-800 bg-slate-900 overflow-hidden">
        {payments.length === 0 ? (
          <div className="p-8"><EmptyBox text="No payment records loaded yet." /></div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-left text-xs">
              <thead className="bg-slate-950 text-slate-400">
                <tr>
                  <th className="px-4 py-2.5">Payment Ref / Bank Ref</th>
                  <th className="px-4 py-2.5">Vendor</th>
                  <th className="px-4 py-2.5">Date</th>
                  <th className="px-4 py-2.5">Referenced Invoice</th>
                  <th className="px-4 py-2.5 text-right">Amount</th>
                  <th className="px-4 py-2.5 text-right">Allocated</th>
                  <th className="px-4 py-2.5">Allocation Status</th>
                  {canReview && <th className="px-4 py-2.5">Action</th>}
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-800/80">
                {payments.map((p) => (
                  <tr key={p.id}>
                    <td className="px-4 py-2.5">
                      <div className="font-mono text-indigo-300">{p.payment_reference}</div>
                      <div className="font-mono text-[11px] text-slate-500">{p.bank_reference || "—"}</div>
                    </td>
                    <td className="px-4 py-2.5 text-slate-200">{p.vendor_name}</td>
                    <td className="px-4 py-2.5 font-mono text-slate-400">{p.payment_date}</td>
                    <td className="px-4 py-2.5 font-mono text-slate-300">
                      {p.referenced_invoice_number || <span className="text-amber-400">Unreferenced</span>}
                    </td>
                    <td className="px-4 py-2.5 text-right font-mono tabular-nums text-slate-100">{formatMoney(p.amount, p.currency)}</td>
                    <td className="px-4 py-2.5 text-right font-mono tabular-nums text-emerald-300">{formatMoney(p.allocated_total, p.currency)}</td>
                    <td className="px-4 py-2.5"><StatusBadge value={p.allocation_status} /></td>
                    {canReview && (
                      <td className="px-4 py-2.5">
                        <button
                          onClick={() => {
                            setAllocPayId(p.id);
                            setAllocAmount(String(p.amount));
                          }}
                          className="rounded border border-slate-700 px-2 py-1 text-xs text-slate-200 hover:bg-slate-800"
                        >
                          Allocate
                        </button>
                      </td>
                    )}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        <PaginationBar pagination={pagination} onPageChange={onPageChange} />
      </div>

      {allocPayId && (
        <form onSubmit={handleAllocatePayment} className="rounded-xl border border-indigo-500/40 bg-slate-900 p-4 space-y-3">
          <h3 className="text-sm font-semibold text-slate-100">Allocate Payment with Explicit Evidence</h3>
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
            <select required value={allocInvId} onChange={(e) => setAllocInvId(e.target.value)} className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 text-xs text-slate-100">
              <option value="">Select Invoice...</option>
              {invoices.map((inv) => (
                <option key={inv.id} value={inv.id}>
                  {inv.invoice_number} ({formatMoney(inv.stated_total, inv.currency)})
                </option>
              ))}
            </select>
            <input required value={allocAmount} onChange={(e) => setAllocAmount(e.target.value)} placeholder="Allocated amount" className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
            <input required value={allocEvidence} onChange={(e) => setAllocEvidence(e.target.value)} placeholder="Explicit matching evidence..." className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 text-xs text-slate-100" />
          </div>
          <div className="flex gap-2">
            <button type="submit" className="rounded bg-indigo-600 px-3 py-1.5 text-xs text-white hover:bg-indigo-500">Confirm Allocation</button>
            <button type="button" onClick={() => setAllocPayId(null)} className="rounded border border-slate-700 px-3 py-1.5 text-xs text-slate-400">Cancel</button>
          </div>
        </form>
      )}
    </div>
  );
}
