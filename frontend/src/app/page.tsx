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
  KeyRound,
  Sparkles,
} from "lucide-react";
import { api, buildPaginationQuery, PaginationMeta, UserSession } from "@/lib/api";
import { DEMO_USER } from "@/lib/demoData";
import { DashboardModules, FindingsFilters } from "@/components/DashboardModules";
import { DocumentViewerModal } from "@/components/DocumentViewerModal";
import { ReconciliationWorkbench } from "@/components/ReconciliationWorkbench";

const PAGE_SIZE = 25;

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
  const [isDemo, setIsDemo] = useState(false);
  const [authChecked, setAuthChecked] = useState(false);
  const [email, setEmail] = useState("admin@abifr.local");
  const [password, setPassword] = useState("");
  const [loginError, setLoginError] = useState<string | null>(null);
  const [loggingIn, setLoggingIn] = useState(false);
  const [rotateBannerDismissed, setRotateBannerDismissed] = useState(false);

  const [activeTab, setActiveTab] = useState("overview");
  const [mobileMenu, setMobileMenu] = useState(false);
  const [loading, setLoading] = useState(false);
  const [banner, setBanner] = useState<{ type: "ok" | "err"; text: string } | null>(null);
  const [health, setHealth] = useState<any>(null);

  // Per-tab pagination offsets & metadata
  const [offsets, setOffsets] = useState<Record<string, number>>({});
  const [paginationMap, setPaginationMap] = useState<Record<string, PaginationMeta | undefined>>({});
  const [findingsFilters, setFindingsFilters] = useState<FindingsFilters>({
    category: "",
    severity: "",
    status: "",
    classification_type: "",
  });

  // Live DB / Demo state
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
  const [users, setUsers] = useState<any[]>([]);
  const [auditLogs, setAuditLogs] = useState<any[]>([]);

  // Active modals
  const [viewerDocId, setViewerDocId] = useState<string | null>(null);
  const [workbenchInvId, setWorkbenchInvId] = useState<string | null>(null);

  const setTabPagination = (key: string, pg?: PaginationMeta) => {
    setPaginationMap((prev) => ({ ...prev, [key]: pg }));
  };

  const fetchTabData = useCallback(async () => {
    if (!user) return;
    setLoading(true);
    try {
      const [h, ov] = await Promise.all([
        api.request("/health").catch(() => null),
        api.request("/dashboard/overview").catch(() => null),
      ]);
      if (h) setHealth(h);
      if (ov) setOverview(ov);

      const offset = offsets[activeTab] ?? 0;

      switch (activeTab) {
        case "overview": {
          const fnds = await api.request(`/findings?${buildPaginationQuery(10, 0)}`);
          setFindings(fnds.findings || []);
          break;
        }
        case "documents": {
          const [docs, vends] = await Promise.all([
            api.request(`/documents?${buildPaginationQuery(PAGE_SIZE, offset)}`),
            api.request(`/vendors?${buildPaginationQuery(100, 0)}`),
          ]);
          setDocuments(docs.documents || []);
          setTabPagination("documents", docs.pagination);
          setVendors(vends.vendors || []);
          break;
        }
        case "vendors": {
          const vends = await api.request(`/vendors?${buildPaginationQuery(PAGE_SIZE, offset)}`);
          setVendors(vends.vendors || []);
          setTabPagination("vendors", vends.pagination);
          break;
        }
        case "contracts": {
          const [ctrs, vends] = await Promise.all([
            api.request(`/contracts?${buildPaginationQuery(PAGE_SIZE, offset)}`),
            api.request(`/vendors?${buildPaginationQuery(100, 0)}`),
          ]);
          setContracts(ctrs.contracts || []);
          setTabPagination("contracts", ctrs.pagination);
          setVendors(vends.vendors || []);
          break;
        }
        case "purchase_orders": {
          const [pos, vends, ctrs] = await Promise.all([
            api.request(`/purchase-orders?${buildPaginationQuery(PAGE_SIZE, offset)}`),
            api.request(`/vendors?${buildPaginationQuery(100, 0)}`),
            api.request(`/contracts?${buildPaginationQuery(100, 0)}`),
          ]);
          setPurchaseOrders(pos.purchase_orders || []);
          setTabPagination("purchase_orders", pos.pagination);
          setVendors(vends.vendors || []);
          setContracts(ctrs.contracts || []);
          break;
        }
        case "invoices": {
          const [invs, vends] = await Promise.all([
            api.request(`/invoices?${buildPaginationQuery(PAGE_SIZE, offset)}`),
            api.request(`/vendors?${buildPaginationQuery(100, 0)}`),
          ]);
          setInvoices(invs.invoices || []);
          setTabPagination("invoices", invs.pagination);
          setVendors(vends.vendors || []);
          break;
        }
        case "payments": {
          const [pays, invs, vends] = await Promise.all([
            api.request(`/payments?${buildPaginationQuery(PAGE_SIZE, offset)}`),
            api.request(`/invoices?${buildPaginationQuery(100, 0)}`),
            api.request(`/vendors?${buildPaginationQuery(100, 0)}`),
          ]);
          setPayments(pays.payments || []);
          setTabPagination("payments", pays.pagination);
          setInvoices(invs.invoices || []);
          setVendors(vends.vendors || []);
          break;
        }
        case "agents": {
          const ag = await api.request(`/agents/runs?${buildPaginationQuery(PAGE_SIZE, offset)}`);
          setAgentRuns(ag.agent_runs || []);
          setTabPagination("agents", ag.pagination);
          break;
        }
        case "anomalies": {
          const [fnds, docs] = await Promise.all([
            api.request(`/findings?${buildPaginationQuery(100, 0)}`),
            api.request(`/documents?${buildPaginationQuery(100, 0)}`),
          ]);
          setFindings(fnds.findings || []);
          setDocuments(docs.documents || []);
          break;
        }
        case "findings": {
          const params = new URLSearchParams({
            limit: String(PAGE_SIZE),
            offset: String(offset),
          });
          if (findingsFilters.category) params.set("category", findingsFilters.category);
          if (findingsFilters.severity) params.set("severity", findingsFilters.severity);
          if (findingsFilters.status) params.set("status", findingsFilters.status);
          if (findingsFilters.classification_type) {
            params.set("classification_type", findingsFilters.classification_type);
          }
          const fnds = await api.request(`/findings?${params.toString()}`);
          setFindings(fnds.findings || []);
          setTabPagination("findings", fnds.pagination);
          break;
        }
        case "review_queue": {
          const rq = await api.request("/review-queue");
          setReviewQueue(rq);
          break;
        }
        case "reports": {
          const rpts = await api.request(`/reports?${buildPaginationQuery(PAGE_SIZE, offset)}`);
          setReports(rpts.reports || []);
          setTabPagination("reports", rpts.pagination);
          break;
        }
        case "models": {
          const mdls = await api.request("/models");
          setModels(mdls.models || []);
          setExternalAiEnv(Boolean(mdls.enable_external_ai_env));
          break;
        }
        case "settings": {
          const [stgs, logsRes, usersRes] = await Promise.all([
            api.request("/settings"),
            api.request(`/audit-logs?${buildPaginationQuery(PAGE_SIZE, offset)}`).catch(() => ({
              audit_logs: [],
              pagination: undefined,
            })),
            user.role === "admin"
              ? api.request("/users").catch(() => ({ users: [] }))
              : Promise.resolve({ users: [] }),
          ]);
          setSettings(stgs.settings || []);
          setAuditLogs(logsRes.audit_logs || []);
          setTabPagination("settings", logsRes.pagination);
          setUsers(usersRes.users || []);
          break;
        }
      }
    } catch (e: any) {
      setBanner({ type: "err", text: e.message });
    } finally {
      setLoading(false);
    }
  }, [user, activeTab, offsets, findingsFilters]);

  useEffect(() => {
    api
      .request<UserSession>("/auth/me")
      .then((u) => {
        api.setCsrf(u.csrf_token);
        setUser(u);
      })
      .catch(() => {
        api.setCsrf(null);
        setUser(null);
      })
      .finally(() => setAuthChecked(true));
  }, []);

  useEffect(() => {
    if (user) {
      fetchTabData();
    }
  }, [user, fetchTabData]);

  const handleLogin = async (e: React.FormEvent) => {
    e.preventDefault();
    setLoggingIn(true);
    setLoginError(null);
    try {
      api.setDemoMode(false);
      setIsDemo(false);
      const res = await api.request("/auth/login", {
        method: "POST",
        body: JSON.stringify({ email, password }),
      });
      api.setCsrf(res.csrf_token);
      setRotateBannerDismissed(false);
      setUser(res.user);
      setPassword("");
    } catch (err: any) {
      setLoginError(err.message);
    } finally {
      setLoggingIn(false);
    }
  };

  const handleEnterDemo = () => {
    setLoginError(null);
    setBanner(null);
    api.setDemoMode(true);
    api.setCsrf(DEMO_USER.csrf_token);
    setIsDemo(true);
    setActiveTab("overview");
    setUser({ ...DEMO_USER });
  };

  const handleLogout = async () => {
    await api.request("/auth/logout", { method: "POST" }).catch(() => {});
    api.setDemoMode(false);
    api.setCsrf(null);
    setIsDemo(false);
    setUser(null);
    setBanner(null);
  };

  const handlePageChange = (tabKey: string, nextOffset: number) => {
    setOffsets((prev) => ({ ...prev, [tabKey]: Math.max(0, nextOffset) }));
  };

  const handleFindingsFilterChange = (next: FindingsFilters) => {
    setFindingsFilters(next);
    setOffsets((prev) => ({ ...prev, findings: 0 }));
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

          <div className="mt-4 border-t border-slate-800 pt-4">
            <button
              type="button"
              onClick={handleEnterDemo}
              className="flex w-full items-center justify-center gap-2 rounded-lg border border-amber-500/40 bg-amber-500/10 py-2.5 text-xs font-semibold text-amber-200 hover:bg-amber-500/20"
            >
              <Sparkles className="h-4 w-4 text-amber-300" />
              Launch Interactive Demo (Synthetic Data)
            </button>
            <p className="mt-1.5 text-center text-[11px] text-slate-500">
              Preview all 14 enterprise modules directly in your browser without a live backend.
            </p>
          </div>

          <div className="mt-4 rounded-lg border border-slate-800 bg-slate-950 p-3 text-[11px] text-slate-400">
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
            <LogOut className="h-3.5 w-3.5" /> {isDemo ? "Exit Demo" : "Sign Out"}
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
              {isDemo ? (
                <span className="inline-flex items-center gap-1.5 rounded-full border border-amber-500/40 bg-amber-500/15 px-2.5 py-0.5 text-amber-200">
                  <Sparkles className="h-3 w-3 text-amber-300" />
                  DEMO MODE • Synthetic Data
                </span>
              ) : (
                <>
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
                </>
              )}
            </div>
          </div>
          <button
            onClick={fetchTabData}
            disabled={loading}
            className="inline-flex items-center gap-1.5 rounded-lg border border-slate-700 bg-slate-900 px-3 py-1.5 text-xs text-slate-200 hover:bg-slate-800"
          >
            <RefreshCw className={`h-3.5 w-3.5 ${loading ? "animate-spin" : ""}`} />
            Refresh
          </button>
        </header>

        <main className="flex-1 p-4 lg:p-6">
          {isDemo && (
            <div className="mb-4 flex flex-wrap items-center justify-between gap-3 rounded-lg border border-amber-500/40 bg-amber-500/10 px-4 py-2.5 text-xs text-amber-100">
              <div className="flex items-center gap-2">
                <Sparkles className="h-4 w-4 text-amber-300 shrink-0" />
                <span>
                  <strong>[DEMO MODE — SYNTHETIC DATA]</strong> Interactive portfolio preview running in-browser without a connected Rust/PostgreSQL backend.
                </span>
              </div>
              <div className="flex items-center gap-2">
                <label className="text-[11px] text-amber-200/80" htmlFor="demo-role">
                  Preview Role:
                </label>
                <select
                  id="demo-role"
                  value={user.role}
                  onChange={(e) => setUser((u) => (u ? { ...u, role: e.target.value } : u))}
                  className="rounded border border-amber-500/40 bg-slate-950 px-2 py-0.5 font-mono text-[11px] text-amber-200"
                >
                  <option value="admin">admin</option>
                  <option value="reviewer">reviewer</option>
                  <option value="analyst">analyst</option>
                  <option value="auditor">auditor</option>
                </select>
                <button
                  onClick={handleLogout}
                  className="rounded border border-amber-400/50 bg-amber-500/20 px-2.5 py-0.5 font-medium text-amber-100 hover:bg-amber-500/30"
                >
                  Exit Demo
                </button>
              </div>
            </div>
          )}

          {user.must_rotate_password && !rotateBannerDismissed && !isDemo && (
            <div className="mb-4 flex flex-wrap items-center justify-between gap-3 rounded-lg border border-amber-500/40 bg-amber-500/10 px-4 py-2.5 text-xs text-amber-200">
              <div className="flex items-center gap-2">
                <KeyRound className="h-4 w-4 text-amber-300 shrink-0" />
                <span>
                  This account still uses a bootstrap credential. Rotate your password before production use.
                </span>
              </div>
              <div className="flex items-center gap-2">
                <button
                  onClick={() => setActiveTab("settings")}
                  className="rounded border border-amber-400/50 bg-amber-500/20 px-2.5 py-1 font-medium text-amber-100 hover:bg-amber-500/30"
                >
                  Rotate Now
                </button>
                <button
                  onClick={() => setRotateBannerDismissed(true)}
                  className="opacity-70 hover:opacity-100"
                >
                  Dismiss
                </button>
              </div>
            </div>
          )}

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
            documentsPagination={paginationMap.documents}
            vendors={vendors}
            vendorsPagination={paginationMap.vendors}
            contracts={contracts}
            contractsPagination={paginationMap.contracts}
            purchaseOrders={purchaseOrders}
            purchaseOrdersPagination={paginationMap.purchase_orders}
            invoices={invoices}
            invoicesPagination={paginationMap.invoices}
            payments={payments}
            paymentsPagination={paginationMap.payments}
            findings={findings}
            findingsPagination={paginationMap.findings}
            findingsFilters={findingsFilters}
            onFindingsFilterChange={handleFindingsFilterChange}
            reviewQueue={reviewQueue}
            agentRuns={agentRuns}
            agentRunsPagination={paginationMap.agents}
            reports={reports}
            reportsPagination={paginationMap.reports}
            models={models}
            externalAiEnv={externalAiEnv}
            settings={settings}
            users={users}
            auditLogs={auditLogs}
            auditPagination={paginationMap.settings}
            onPageChange={handlePageChange}
            onRefresh={fetchTabData}
            onPasswordRotated={() =>
              setUser((u) => (u ? { ...u, must_rotate_password: false } : u))
            }
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
          canWrite={
            user.role === "admin" || user.role === "reviewer" || user.role === "analyst"
          }
          onClose={() => setViewerDocId(null)}
          onUpdated={fetchTabData}
        />
      )}

      {workbenchInvId && (
        <ReconciliationWorkbench
          invoiceId={workbenchInvId}
          onClose={() => setWorkbenchInvId(null)}
          onOpenDocument={(docId) => setViewerDocId(docId)}
          onReconciled={fetchTabData}
        />
      )}
    </div>
  );
}
