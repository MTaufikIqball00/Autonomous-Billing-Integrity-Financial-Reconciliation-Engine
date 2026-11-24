"use client";

import React, { useEffect, useState, useCallback } from "react";
import {
  LayoutDashboard,
  FileUp,
  FileSignature,
  Receipt,
  Building2,
  ClipboardList,
  CreditCard,
  Bot,
  ShieldAlert,
  FileSearch,
  UserCheck,
  BarChart3,
  Cpu,
  Settings,
  LogOut,
  Menu,
  X,
  RefreshCw,
  ShieldCheck,
} from "lucide-react";
import { api, UserSession } from "@/lib/api";
import { DashboardModules } from "@/components/DashboardModules";
import { DocumentViewerModal } from "@/components/DocumentViewerModal";
import { ReconciliationWorkbench } from "@/components/ReconciliationWorkbench";

const NAV_ITEMS = [
  { id: "overview", label: "Executive Overview", icon: LayoutDashboard },
  { id: "documents", label: "Document Processing", icon: FileUp },
  { id: "contracts", label: "Contract Repository", icon: FileSignature },
  { id: "invoices", label: "Invoice Management", icon: Receipt },
  { id: "vendors", label: "Vendor Management", icon: Building2 },
  { id: "purchase_orders", label: "PO Matching", icon: ClipboardList },
  { id: "payments", label: "Payment Reconciliation", icon: CreditCard },
  { id: "agents", label: "AI Agent Activity", icon: Bot },
  { id: "anomalies", label: "Anomaly Center", icon: ShieldAlert },
  { id: "findings", label: "Audit Findings", icon: FileSearch },
  { id: "review_queue", label: "Human Review Queue", icon: UserCheck },
  { id: "reports", label: "Audit Reports", icon: BarChart3 },
  { id: "models", label: "Model & Providers", icon: Cpu },
  { id: "settings", label: "Settings & Audit Log", icon: Settings },
];

export default function HomePage() {
  const [user, setUser] = useState<UserSession | null>(null);
  const [authChecked, setAuthChecked] = useState(false);
  const [email, setEmail] = useState("admin@abifr.local");
  const [password, setPassword] = useState("");
  const [loginError, setLoginError] = useState<string | null>(null);
  const [loggingIn, setLoggingIn] = useState(false);

  const [activeTab, setActiveTab] = useState("overview");
  const [mobileMenu, setMobileMenu] = useState(false);
  const [loading, setLoading] = useState(false);
  const [banner, setBanner] = useState<{ type: "ok" | "err"; text: string } | null>(null);
  const [health, setHealth] = useState<any>(null);

  // Live DB state
  const [overview, setOverview] = useState<any>(null);
  const [documents, setDocuments] = useState<any[]>([]);
  const [vendors, setVendors] = useState<any[]>([]);
  const [contracts, setContracts] = useState<any[]>([]);
  const [purchaseOrders, setPurchaseOrders] = useState<any[]>([]);
  const [invoices, setInvoices] = useState<any[]>([]);
  const [payments, setPayments] = useState<any[]>([]);
  const [findings, setFindings] = useState<any[]>([]);
  const [reviewQueue, setReviewQueue] = useState<any>(null);
  const [agentRuns, setAgentRuns] = useState<any[]>([]);
  const [reports, setReports] = useState<any[]>([]);
  const [models, setModels] = useState<any[]>([]);
  const [externalAiEnv, setExternalAiEnv] = useState(false);
  const [settings, setSettings] = useState<any[]>([]);
  const [auditLogs, setAuditLogs] = useState<any[]>([]);

  // Active modals
  const [viewerDocId, setViewerDocId] = useState<string | null>(null);
  const [workbenchInvId, setWorkbenchInvId] = useState<string | null>(null);

  const refreshAll = useCallback(async () => {
    setLoading(true);
    try {
      const [h, ov, docs, vends, ctrs, pos, invs, pays, fnds, rq, ag, rpts, mdls, stgs] =
        await Promise.all([
          api.request("/health").catch(() => null),
          api.request("/dashboard/overview"),
          api.request("/documents"),
          api.request("/vendors"),
          api.request("/contracts"),
          api.request("/purchase-orders"),
          api.request("/invoices"),
          api.request("/payments"),
          api.request("/findings"),
          api.request("/review-queue"),
          api.request("/agents/runs"),
          api.request("/reports"),
          api.request("/models"),
          api.request("/settings"),
        ]);
      setHealth(h);
      setOverview(ov);
      setDocuments(docs.documents || []);
      setVendors(vends.vendors || []);
      setContracts(ctrs.contracts || []);
      setPurchaseOrders(pos.purchase_orders || []);
      setInvoices(invs.invoices || []);
      setPayments(pays.payments || []);
      setFindings(fnds.findings || []);
      setReviewQueue(rq);
      setAgentRuns(ag.agent_runs || []);
      setReports(rpts.reports || []);
      setModels(mdls.models || []);
      setExternalAiEnv(Boolean(mdls.enable_external_ai_env));
      setSettings(stgs.settings || []);

      try {
        const logs = await api.request("/audit-logs");
        setAuditLogs(logs.audit_logs || []);
      } catch {
        setAuditLogs([]);
      }
    } catch (e: any) {
      setBanner({ type: "err", text: e.message });
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    api.hydrate();
    api
      .request<UserSession>("/auth/me")
      .then((u) => {
        setUser(u);
      })
      .catch(() => {
        setUser(null);
      })
      .finally(() => setAuthChecked(true));
  }, []);

  useEffect(() => {
    if (user) {
      refreshAll();
    }
  }, [user, refreshAll]);

  const handleLogin = async (e: React.FormEvent) => {
    e.preventDefault();
    setLoggingIn(true);
    setLoginError(null);
    try {
      const res = await api.request("/auth/login", {
        method: "POST",
        body: JSON.stringify({ email, password }),
      });
      api.setAuth(res.token, res.csrf_token);
      setUser(res.user);
      setPassword("");
    } catch (err: any) {
      setLoginError(err.message);
    } finally {
      setLoggingIn(false);
    }
  };

  const handleLogout = async () => {
    await api.request("/auth/logout", { method: "POST" }).catch(() => {});
    api.setAuth(null, null);
    setUser(null);
  };

  if (!authChecked) {
    return (
      <div className="flex min-h-screen items-center justify-center bg-slate-950 text-sm text-slate-400">
        Verifying authenticated session...
      </div>
    );
  }

  if (!user) {
    return (
      <main className="flex min-h-screen items-center justify-center bg-slate-950 p-4">
        <div className="w-full max-w-md rounded-2xl border border-slate-800 bg-slate-900 p-7 shadow-2xl">
          <div className="flex items-center gap-3">
            <div className="flex h-10 w-10 items-center justify-center rounded-xl bg-indigo-600/20 text-indigo-400">
              <ShieldCheck className="h-6 w-6" />
            </div>
            <div>
              <h1 className="text-base font-bold tracking-tight text-slate-100">
                ABIFR Enterprise Platform
              </h1>
              <p className="text-xs text-slate-400">
                Agentic Billing Integrity & Financial Reconciliation
              </p>
            </div>
          </div>

          <form onSubmit={handleLogin} className="mt-6 space-y-4">
            <div>
              <label htmlFor="email" className="block text-xs font-medium text-slate-300">
                Work Email
              </label>
              <input
                id="email"
                type="email"
                required
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                className="mt-1.5 w-full rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 text-sm text-slate-100 focus:border-indigo-500 focus:outline-none focus:ring-2 focus:ring-indigo-500/30"
              />
            </div>
            <div>
              <label htmlFor="password" className="block text-xs font-medium text-slate-300">
                Password
              </label>
              <input
                id="password"
                type="password"
                required
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder="Enter your account password"
                className="mt-1.5 w-full rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 text-sm text-slate-100 focus:border-indigo-500 focus:outline-none focus:ring-2 focus:ring-indigo-500/30"
              />
            </div>

            {loginError && (
              <div className="rounded-lg border border-rose-500/40 bg-rose-500/10 px-3 py-2 text-xs text-rose-200">
                {loginError}
              </div>
            )}

            <button
              type="submit"
              disabled={loggingIn}
              className="w-full rounded-lg bg-indigo-600 py-2.5 text-sm font-medium text-white hover:bg-indigo-500 disabled:opacity-50"
            >
              {loggingIn ? "Signing in..." : "Sign in to Rust Backend"}
            </button>
          </form>

          <div className="mt-5 rounded-lg border border-slate-800 bg-slate-950 p-3 text-[11px] text-slate-400">
            <p className="font-semibold text-slate-300">RBAC Local Roles Provisioned:</p>
            <p className="mt-1 font-mono">admin@abifr.local • reviewer@abifr.local</p>
            <p className="font-mono">analyst@abifr.local • auditor@abifr.local</p>
          </div>
        </div>
      </main>
    );
  }

  return (
    <div className="flex min-h-screen bg-slate-950 text-slate-100">
      {/* Desktop Sidebar */}
      <aside className="hidden w-64 shrink-0 flex-col border-r border-slate-800 bg-slate-900/90 lg:flex">
        <div className="flex items-center gap-2.5 border-b border-slate-800 px-4 py-4">
          <ShieldCheck className="h-6 w-6 text-indigo-400 shrink-0" />
          <div className="min-w-0">
            <h1 className="truncate text-sm font-bold text-slate-100">ABIFR Engine</h1>
            <p className="truncate text-[10px] text-slate-400">Evidence-Driven Financial AI</p>
          </div>
        </div>

        <nav className="flex-1 space-y-0.5 overflow-y-auto p-2.5" aria-label="Primary Navigation">
          {NAV_ITEMS.map((item) => {
            const Icon = item.icon;
            const active = activeTab === item.id;
            return (
              <button
                key={item.id}
                onClick={() => setActiveTab(item.id)}
                className={`flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-xs font-medium transition-colors ${
                  active
                    ? "bg-indigo-600/20 text-indigo-200 border border-indigo-500/30"
                    : "text-slate-400 hover:bg-slate-800/70 hover:text-slate-200"
                }`}
              >
                <Icon className="h-4 w-4 shrink-0" />
                <span className="truncate">{item.label}</span>
                {item.id === "review_queue" && (overview?.findings?.pending_review ?? 0) > 0 && (
                  <span className="ml-auto rounded-full bg-amber-500/20 px-1.5 py-0.5 font-mono text-[10px] text-amber-300">
                    {overview.findings.pending_review}
                  </span>
                )}
              </button>
            );
          })}
        </nav>

        <div className="border-t border-slate-800 p-3 text-xs">
          <div className="mb-2 rounded-lg bg-slate-950 p-2.5">
            <p className="truncate font-medium text-slate-200">{user.full_name}</p>
            <p className="truncate font-mono text-[10px] text-indigo-300 uppercase">{user.role}</p>
          </div>
          <button
            onClick={handleLogout}
            className="flex w-full items-center justify-center gap-1.5 rounded-lg border border-slate-700 py-1.5 text-xs text-slate-300 hover:bg-slate-800"
          >
            <LogOut className="h-3.5 w-3.5" /> Sign Out
          </button>
        </div>
      </aside>

      {/* Mobile Drawer */}
      {mobileMenu && (
        <div className="fixed inset-0 z-50 flex lg:hidden">
          <div className="w-64 bg-slate-900 border-r border-slate-800 p-3 flex flex-col">
            <div className="flex items-center justify-between pb-3 border-b border-slate-800">
              <span className="text-sm font-bold">ABIFR Engine</span>
              <button onClick={() => setMobileMenu(false)} aria-label="Close menu">
                <X className="h-5 w-5 text-slate-400" />
              </button>
            </div>
            <div className="mt-2 flex-1 space-y-1 overflow-y-auto">
              {NAV_ITEMS.map((item) => (
                <button
                  key={item.id}
                  onClick={() => {
                    setActiveTab(item.id);
                    setMobileMenu(false);
                  }}
                  className={`block w-full rounded px-3 py-2 text-left text-xs ${
                    activeTab === item.id ? "bg-indigo-600/20 text-indigo-200" : "text-slate-300"
                  }`}
                >
                  {item.label}
                </button>
              ))}
            </div>
          </div>
          <div className="flex-1 bg-black/60" onClick={() => setMobileMenu(false)} />
        </div>
      )}

      {/* Main Content */}
      <div className="flex min-w-0 flex-1 flex-col">
        <header className="flex flex-wrap items-center justify-between gap-3 border-b border-slate-800 bg-slate-900/60 px-4 py-3 lg:px-6">
          <div className="flex items-center gap-3">
            <button
              onClick={() => setMobileMenu(true)}
              aria-label="Open menu"
              className="rounded-lg border border-slate-700 p-1.5 text-slate-300 lg:hidden"
            >
              <Menu className="h-4 w-4" />
            </button>
            <div className="flex flex-wrap items-center gap-2 text-[11px] font-mono">
              <span className="inline-flex items-center gap-1.5 rounded-full border border-emerald-500/30 bg-emerald-500/10 px-2.5 py-0.5 text-emerald-300">
                <span className="h-1.5 w-1.5 rounded-full bg-emerald-400" />
                Rust API :{health?.backend_port ?? 14800}
              </span>
              <span className="rounded-full border border-slate-700 bg-slate-900 px-2.5 py-0.5 text-slate-300">
                PG: {health?.postgresql ?? "..."}
              </span>
              <span className="rounded-full border border-slate-700 bg-slate-900 px-2.5 py-0.5 text-slate-300">
                Doc-Intel: {health?.doc_intel?.status ?? "..."}
              </span>
            </div>
          </div>
          <button
            onClick={refreshAll}
            disabled={loading}
            className="inline-flex items-center gap-1.5 rounded-lg border border-slate-700 bg-slate-900 px-3 py-1.5 text-xs text-slate-200 hover:bg-slate-800"
          >
            <RefreshCw className={`h-3.5 w-3.5 ${loading ? "animate-spin" : ""}`} />
            Refresh
          </button>
        </header>

        <main className="flex-1 p-4 lg:p-6">
          {banner && (
            <div
              className={`mb-4 flex items-center justify-between rounded-lg border px-4 py-2.5 text-xs ${
                banner.type === "ok"
                  ? "border-emerald-500/40 bg-emerald-500/10 text-emerald-200"
                  : "border-rose-500/40 bg-rose-500/10 text-rose-200"
              }`}
            >
              <span>{banner.text}</span>
              <button onClick={() => setBanner(null)} className="ml-3 opacity-70 hover:opacity-100">
                Dismiss
              </button>
            </div>
          )}

          <DashboardModules
            activeTab={activeTab}
            user={user}
            overview={overview}
            documents={documents}
            vendors={vendors}
            contracts={contracts}
            purchaseOrders={purchaseOrders}
            invoices={invoices}
            payments={payments}
            findings={findings}
            reviewQueue={reviewQueue}
            agentRuns={agentRuns}
            reports={reports}
            models={models}
            settings={settings}
            auditLogs={auditLogs}
            externalAiEnv={externalAiEnv}
            onRefresh={refreshAll}
            onOpenDocument={(docId) => setViewerDocId(docId)}
            onOpenInvoice={(invId) => setWorkbenchInvId(invId)}
            setBanner={setBanner}
          />
        </main>
      </div>

      {viewerDocId && (
        <DocumentViewerModal
          documentId={viewerDocId}
          canReview={user.role === "admin" || user.role === "reviewer"}
          onClose={() => setViewerDocId(null)}
          onUpdated={refreshAll}
        />
      )}

      {workbenchInvId && (
        <ReconciliationWorkbench
          invoiceId={workbenchInvId}
          onClose={() => setWorkbenchInvId(null)}
          onOpenDocument={(docId) => setViewerDocId(docId)}
          onReconciled={refreshAll}
        />
      )}
    </div>
  );
}
