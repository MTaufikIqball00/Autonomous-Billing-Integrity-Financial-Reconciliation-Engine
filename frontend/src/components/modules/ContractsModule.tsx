"use client";

import React, { useState } from "react";
import { api, formatMoney, PaginationMeta } from "@/lib/api";
import { StatusBadge } from "../StatusBadge";
import { EmptyBox, PaginationBar } from "./SharedUi";

export function ContractsModule({
  contracts,
  pagination,
  vendors,
  canWrite,
  onRefresh,
  onPageChange,
  setBanner,
}: {
  contracts: any[];
  pagination?: PaginationMeta;
  vendors: any[];
  canWrite: boolean;
  onRefresh: () => Promise<void>;
  onPageChange: (offset: number) => void;
  setBanner: (b: { type: "ok" | "err"; text: string } | null) => void;
}) {
  const [contractDetail, setContractDetail] = useState<any>(null);
  const [showCreate, setShowCreate] = useState(false);

  // Create Contract state
  const [cNum, setCNum] = useState("");
  const [cVendor, setCVendor] = useState("");
  const [cTitle, setCTitle] = useState("");
  const [cCurrency, setCCurrency] = useState("USD");
  const [cEff, setCEff] = useState("2026-01-01");
  const [cExp, setCExp] = useState("2026-12-31");
  const [cTolPct, setCTolPct] = useState("");
  const [cTax, setCTax] = useState("");
  const [cSku, setCSku] = useState("SKU-01");
  const [cDesc, setCDesc] = useState("");
  const [cUnit, setCUnit] = useState("unit");
  const [cPrice, setCPrice] = useState("100.00");

  // Add Amendment state
  const [showAmend, setShowAmend] = useState(false);
  const [amLabel, setAmLabel] = useState("v2.0-amendment");
  const [amEff, setAmEff] = useState("2026-07-01");
  const [amExp, setAmExp] = useState("2026-12-31");
  const [amSku, setAmSku] = useState("SKU-01");
  const [amDesc, setAmDesc] = useState("");
  const [amPrice, setAmPrice] = useState("95.00");

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    try {
      await api.request("/contracts", {
        method: "POST",
        body: JSON.stringify({
          contract_number: cNum,
          vendor_id: cVendor,
          title: cTitle,
          currency: cCurrency,
          effective_date: cEff,
          expiration_date: cExp,
          price_tolerance_pct: cTolPct ? cTolPct : null,
          default_tax_rate: cTax ? cTax : null,
          line_items: [
            {
              item_code: cSku || null,
              description: cDesc,
              unit: cUnit,
              unit_price: cPrice,
            },
          ],
        }),
      });
      setShowCreate(false);
      setCNum("");
      setCTitle("");
      setCDesc("");
      setBanner({ type: "ok", text: "Contract created." });
      await onRefresh();
    } catch (err: any) {
      setBanner({ type: "err", text: err.message });
    }
  };

  const handleAddAmendment = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!contractDetail?.contract?.id) return;
    try {
      await api.request(`/contracts/${contractDetail.contract.id}/versions`, {
        method: "POST",
        body: JSON.stringify({
          version_label: amLabel,
          effective_date: amEff,
          expiration_date: amExp,
          line_items: [
            {
              item_code: amSku || null,
              description: amDesc,
              unit: "unit",
              unit_price: amPrice,
            },
          ],
        }),
      });
      setShowAmend(false);
      const updated = await api.request(`/contracts/${contractDetail.contract.id}`);
      setContractDetail(updated);
      setBanner({ type: "ok", text: "Contract amendment version added." });
      await onRefresh();
    } catch (err: any) {
      setBanner({ type: "err", text: err.message });
    }
  };

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-xl font-semibold text-slate-100">Contract Repository</h2>
          <p className="text-xs text-slate-400">
            Tracks effective contract versions, amendments, pricing schedules, and explicit tolerance policies (marked UNDEFINED when undocumented).
          </p>
        </div>
        {canWrite && (
          <button
            onClick={() => setShowCreate((v) => !v)}
            className="rounded-lg bg-indigo-600 px-3.5 py-2 text-xs font-medium text-white hover:bg-indigo-500"
          >
            {showCreate ? "Hide Form" : "+ New Contract"}
          </button>
        )}
      </div>

      {showCreate && canWrite && (
        <form onSubmit={handleCreate} className="rounded-xl border border-indigo-500/30 bg-slate-900 p-4 space-y-3">
          <h3 className="text-xs font-semibold uppercase tracking-wider text-indigo-300">Create Contract & Initial Line Item</h3>
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
            <input required value={cNum} onChange={(e) => setCNum(e.target.value)} placeholder="Contract # (CTR-2026-01)" className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
            <select required value={cVendor} onChange={(e) => setCVendor(e.target.value)} className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 text-xs text-slate-100">
              <option value="">Select Vendor...</option>
              {vendors.map((v) => <option key={v.id} value={v.id}>{v.legal_name}</option>)}
            </select>
            <input required value={cTitle} onChange={(e) => setCTitle(e.target.value)} placeholder="Contract Title" className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 text-xs text-slate-100" />
            <input required value={cCurrency} onChange={(e) => setCCurrency(e.target.value)} placeholder="Currency (USD)" className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
            <input required type="date" value={cEff} onChange={(e) => setCEff(e.target.value)} className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
            <input required type="date" value={cExp} onChange={(e) => setCExp(e.target.value)} className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
            <input value={cTolPct} onChange={(e) => setCTolPct(e.target.value)} placeholder="Price Tolerance % (leave blank if undefined)" className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
            <input value={cTax} onChange={(e) => setCTax(e.target.value)} placeholder="Default Tax Rate % (optional)" className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
          </div>
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-4 border-t border-slate-800 pt-3">
            <input value={cSku} onChange={(e) => setCSku(e.target.value)} placeholder="Item Code" className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
            <input required value={cDesc} onChange={(e) => setCDesc(e.target.value)} placeholder="Agreed Item Description" className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 text-xs text-slate-100" />
            <input required value={cUnit} onChange={(e) => setCUnit(e.target.value)} placeholder="Unit" className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 text-xs text-slate-100" />
            <input required value={cPrice} onChange={(e) => setCPrice(e.target.value)} placeholder="Unit Price" className="rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono text-xs text-slate-100" />
          </div>
          <button type="submit" className="rounded-lg bg-indigo-600 px-3.5 py-1.5 text-xs font-medium text-white hover:bg-indigo-500">
            Save Contract
          </button>
        </form>
      )}

      <div className="rounded-xl border border-slate-800 bg-slate-900 overflow-hidden">
        {contracts.length === 0 ? (
          <div className="p-8"><EmptyBox text="No contracts registered yet." /></div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-left text-xs">
              <thead className="bg-slate-950 text-slate-400">
                <tr>
                  <th className="px-4 py-2.5">Contract #</th>
                  <th className="px-4 py-2.5">Title & Vendor</th>
                  <th className="px-4 py-2.5">Currency</th>
                  <th className="px-4 py-2.5">Versions</th>
                  <th className="px-4 py-2.5">Latest Expiry</th>
                  <th className="px-4 py-2.5">Status</th>
                  <th className="px-4 py-2.5">Inspect</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-800/80">
                {contracts.map((c) => (
                  <tr key={c.id} className="hover:bg-slate-800/40">
                    <td className="px-4 py-2.5 font-mono text-indigo-300">{c.contract_number}</td>
                    <td className="px-4 py-2.5">
                      <div className="font-medium text-slate-100">{c.title}</div>
                      <div className="text-slate-400">{c.vendor_name}</div>
                    </td>
                    <td className="px-4 py-2.5 font-mono">{c.currency}</td>
                    <td className="px-4 py-2.5 font-mono">{c.version_count}</td>
                    <td className="px-4 py-2.5 font-mono">{c.latest_expiration || "—"}</td>
                    <td className="px-4 py-2.5"><StatusBadge value={c.status} /></td>
                    <td className="px-4 py-2.5">
                      <button
                        onClick={async () => {
                          const d = await api.request(`/contracts/${c.id}`);
                          setContractDetail(d);
                          setShowAmend(false);
                        }}
                        className="rounded border border-slate-700 px-2.5 py-1 text-xs text-slate-200 hover:bg-slate-800"
                      >
                        Versions & Terms
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

      {contractDetail && (
        <div className="rounded-xl border border-indigo-500/40 bg-slate-900 p-5 space-y-4">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div>
              <h3 className="text-sm font-semibold text-slate-100">
                {contractDetail.contract.contract_number} — {contractDetail.contract.title}
              </h3>
              <p className="text-xs text-slate-400">{contractDetail.contract.vendor_name}</p>
            </div>
            <div className="flex items-center gap-2">
              {canWrite && (
                <button
                  onClick={() => setShowAmend((v) => !v)}
                  className="rounded border border-indigo-500/40 bg-indigo-500/15 px-2.5 py-1 text-xs text-indigo-200"
                >
                  + Add Amendment
                </button>
              )}
              <button onClick={() => setContractDetail(null)} className="text-xs text-slate-400 hover:text-white">Close</button>
            </div>
          </div>

          {showAmend && canWrite && (
            <form onSubmit={handleAddAmendment} className="rounded-lg border border-slate-800 bg-slate-950 p-3 space-y-2">
              <div className="grid grid-cols-1 gap-2 sm:grid-cols-3">
                <input required value={amLabel} onChange={(e) => setAmLabel(e.target.value)} placeholder="Version label" className="rounded border border-slate-700 bg-slate-900 px-2 py-1 text-xs text-slate-100" />
                <input required type="date" value={amEff} onChange={(e) => setAmEff(e.target.value)} className="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono text-xs text-slate-100" />
                <input required type="date" value={amExp} onChange={(e) => setAmExp(e.target.value)} className="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono text-xs text-slate-100" />
                <input value={amSku} onChange={(e) => setAmSku(e.target.value)} placeholder="SKU" className="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono text-xs text-slate-100" />
                <input required value={amDesc} onChange={(e) => setAmDesc(e.target.value)} placeholder="Amended item description" className="rounded border border-slate-700 bg-slate-900 px-2 py-1 text-xs text-slate-100" />
                <input required value={amPrice} onChange={(e) => setAmPrice(e.target.value)} placeholder="New unit price" className="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono text-xs text-slate-100" />
              </div>
              <button type="submit" className="rounded bg-indigo-600 px-3 py-1 text-xs text-white">Save Amendment</button>
            </form>
          )}

          {contractDetail.versions.map((v: any) => (
            <div key={v.id} className="rounded-lg border border-slate-800 bg-slate-950 p-4 space-y-2">
              <div className="flex flex-wrap items-center gap-2 text-xs">
                <span className="rounded bg-indigo-500/20 px-2 py-0.5 font-mono font-semibold text-indigo-300">{v.version_label}</span>
                {v.is_amendment && <StatusBadge value="contract_amendment" />}
                <span className="font-mono text-slate-300">Effective: {v.effective_date} → {v.expiration_date}</span>
                <span className="font-mono text-amber-300">
                  Price Tolerance: {v.price_tolerance_pct ? `${v.price_tolerance_pct}%` : v.price_tolerance_abs ?? "UNDEFINED"}
                </span>
                <span className="font-mono text-slate-400">Tax Rule: {v.default_tax_rate ? `${v.default_tax_rate}%` : "Not Documented"}</span>
              </div>
              <table className="w-full text-left text-xs mt-2">
                <thead className="text-slate-500">
                  <tr>
                    <th className="py-1">SKU</th>
                    <th className="py-1">Description</th>
                    <th className="py-1 text-right">Agreed Price</th>
                    <th className="py-1 text-right">Discount %</th>
                    <th className="py-1 text-right">Tax %</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-800/60">
                  {v.line_items.map((li: any) => (
                    <tr key={li.id}>
                      <td className="py-1 font-mono text-slate-400">{li.item_code || "—"}</td>
                      <td className="py-1 text-slate-200">{li.description}</td>
                      <td className="py-1 text-right font-mono tabular-nums text-emerald-300">{formatMoney(li.unit_price, li.currency)} / {li.unit}</td>
                      <td className="py-1 text-right font-mono tabular-nums">{li.discount_pct ?? 0}%</td>
                      <td className="py-1 text-right font-mono tabular-nums">{li.tax_rate ?? "—"}%</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
