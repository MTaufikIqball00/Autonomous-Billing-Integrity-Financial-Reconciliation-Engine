"use client";

import React, { useState } from "react";
import { KeyRound, ShieldAlert, Sliders, UserPlus, Users } from "lucide-react";
import { api, PaginationMeta, UserSession } from "@/lib/api";
import { StatusBadge } from "../StatusBadge";
import { EmptyBox, PaginationBar } from "./SharedUi";

export function SettingsAdminModule({
  user,
  settings,
  users,
  auditLogs,
  auditPagination,
  onAuditPageChange,
  onRefresh,
  onPasswordRotated,
  setBanner,
}: {
  user: UserSession;
  settings: any[];
  users: any[];
  auditLogs: any[];
  auditPagination?: PaginationMeta;
  onAuditPageChange: (offset: number) => void;
  onRefresh: () => Promise<void>;
  onPasswordRotated: () => void;
  setBanner: (b: { type: "ok" | "err"; text: string } | null) => void;
}) {
  const isAdmin = user.role === "admin";

  // Self password rotation state
  const [currentSecret, setCurrentSecret] = useState("");
  const [nextSecret, setNextSecret] = useState("");
  const [rotating, setRotating] = useState(false);

  // Settings JSON editor state
  const [settingDrafts, setSettingDrafts] = useState<Record<string, string>>({});

  // Admin create user state
  const [newEmail, setNewEmail] = useState("");
  const [newName, setNewName] = useState("");
  const [newRole, setNewRole] = useState("analyst");
  const [newCredential, setNewCredential] = useState("");
  const [creatingUser, setCreatingUser] = useState(false);

  const handleRotatePassword = async (e: React.FormEvent) => {
    e.preventDefault();
    if (nextSecret.length < 12) {
      setBanner({ type: "err", text: "New passphrase must be at least 12 characters." });
      return;
    }
    setRotating(true);
    try {
      await api.request("/auth/change-password", {
        method: "POST",
        body: JSON.stringify({
          current_password: currentSecret,
          new_password: nextSecret,
        }),
      });
      setCurrentSecret("");
      setNextSecret("");
      setBanner({
        type: "ok",
        text: "Account credentials rotated successfully. Prior concurrent sessions were revoked.",
      });
      onPasswordRotated();
      await onRefresh();
    } catch (err: any) {
      setBanner({ type: "err", text: err.message });
    } finally {
      setRotating(false);
    }
  };

  const handleSaveSetting = async (key: string, originalValue: any) => {
    const raw = settingDrafts[key] ?? JSON.stringify(originalValue, null, 2);
    let parsed: any;
    try {
      parsed = JSON.parse(raw);
    } catch {
      setBanner({ type: "err", text: `Invalid JSON syntax for setting '${key}'.` });
      return;
    }
    try {
      await api.request(`/settings/${encodeURIComponent(key)}`, {
        method: "PATCH",
        body: JSON.stringify({ value: parsed }),
      });
      setBanner({ type: "ok", text: `System setting '${key}' updated and audited.` });
      await onRefresh();
    } catch (err: any) {
      setBanner({ type: "err", text: err.message });
    }
  };

  const handleCreateUser = async (e: React.FormEvent) => {
    e.preventDefault();
    if (newCredential.length < 12) {
      setBanner({ type: "err", text: "Initial passphrase must be at least 12 characters." });
      return;
    }
    setCreatingUser(true);
    try {
      await api.request("/users", {
        method: "POST",
        body: JSON.stringify({
          email: newEmail.trim(),
          full_name: newName.trim(),
          password: newCredential,
          role: newRole,
        }),
      });
      setNewEmail("");
      setNewName("");
      setNewCredential("");
      setBanner({ type: "ok", text: "New RBAC user provisioned (rotation required on first use)." });
      await onRefresh();
    } catch (err: any) {
      setBanner({ type: "err", text: err.message });
    } finally {
      setCreatingUser(false);
    }
  };

  const handleToggleUserStatus = async (u: any) => {
    try {
      await api.request(`/users/${u.id}/status`, {
        method: "PATCH",
        body: JSON.stringify({ is_active: !u.is_active }),
      });
      setBanner({
        type: "ok",
        text: `User ${u.email} ${!u.is_active ? "activated" : "deactivated (active sessions revoked)"}.`,
      });
      await onRefresh();
    } catch (err: any) {
      setBanner({ type: "err", text: err.message });
    }
  };

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-xl font-semibold text-slate-100">System Settings, RBAC & Audit Trail</h2>
        <p className="text-xs text-slate-400">
          Manage your security credentials, enterprise configuration, RBAC user accounts, and immutable audit logs.
        </p>
      </div>

      {/* 1. Account Credential Rotation */}
      <div className="rounded-xl border border-slate-800 bg-slate-900 p-5 space-y-4">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <KeyRound className="h-4 w-4 text-indigo-400" />
            <h3 className="text-sm font-semibold text-slate-100">Account Security & Credential Rotation</h3>
          </div>
          {user.must_rotate_password && (
            <span className="inline-flex items-center gap-1 rounded-full bg-amber-500/20 px-2.5 py-0.5 text-[11px] font-medium text-amber-300">
              <ShieldAlert className="h-3.5 w-3.5" /> Bootstrap credential rotation recommended
            </span>
          )}
        </div>

        <form onSubmit={handleRotatePassword} className="grid grid-cols-1 gap-3 md:grid-cols-3">
          <div>
            <label className="block text-[11px] text-slate-400">Current Credential</label>
            <input
              type="password"
              required
              value={currentSecret}
              onChange={(e) => setCurrentSecret(e.target.value)}
              className="mt-1 w-full rounded border border-slate-700 bg-slate-950 px-3 py-1.5 text-xs text-slate-100"
            />
          </div>
          <div>
            <label className="block text-[11px] text-slate-400">New Credential (min 12 chars)</label>
            <input
              type="password"
              required
              minLength={12}
              value={nextSecret}
              onChange={(e) => setNextSecret(e.target.value)}
              className="mt-1 w-full rounded border border-slate-700 bg-slate-950 px-3 py-1.5 text-xs text-slate-100"
            />
          </div>
          <div className="flex items-end">
            <button
              type="submit"
              disabled={rotating}
              className="rounded-lg bg-indigo-600 px-4 py-1.5 text-xs font-medium text-white hover:bg-indigo-500 disabled:opacity-50"
            >
              {rotating ? "Updating..." : "Update Credential"}
            </button>
          </div>
        </form>
      </div>

      {/* 2. System Settings */}
      <div className="rounded-xl border border-slate-800 bg-slate-900 p-5 space-y-4">
        <div className="flex items-center gap-2">
          <Sliders className="h-4 w-4 text-indigo-400" />
          <h3 className="text-sm font-semibold text-slate-100">System Policy & Tolerance Configuration</h3>
        </div>
        {settings.length === 0 ? (
          <EmptyBox text="No system settings records returned." />
        ) : (
          <div className="space-y-3">
            {settings.map((s) => {
              const draftText = settingDrafts[s.key] ?? JSON.stringify(s.value, null, 2);
              return (
                <div key={s.key} className="rounded-lg border border-slate-800 bg-slate-950 p-3.5 space-y-2">
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <div>
                      <span className="font-mono text-xs font-semibold text-indigo-300">{s.key}</span>
                      <p className="text-[11px] text-slate-400">{s.description}</p>
                    </div>
                    {isAdmin && (
                      <button
                        onClick={() => handleSaveSetting(s.key, s.value)}
                        className="rounded border border-indigo-500/40 bg-indigo-500/15 px-3 py-1 text-[11px] font-medium text-indigo-200 hover:bg-indigo-500/25"
                      >
                        Save JSON
                      </button>
                    )}
                  </div>
                  {isAdmin ? (
                    <textarea
                      rows={3}
                      value={draftText}
                      onChange={(e) => setSettingDrafts((prev) => ({ ...prev, [s.key]: e.target.value }))}
                      className="w-full rounded border border-slate-800 bg-slate-900 p-2 font-mono text-[11px] text-slate-200"
                    />
                  ) : (
                    <pre className="overflow-x-auto rounded bg-slate-900 p-2 font-mono text-[11px] text-slate-300">
                      {JSON.stringify(s.value, null, 2)}
                    </pre>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* 3. Admin User Management */}
      {isAdmin && (
        <div className="rounded-xl border border-slate-800 bg-slate-900 p-5 space-y-4">
          <div className="flex items-center gap-2">
            <Users className="h-4 w-4 text-indigo-400" />
            <h3 className="text-sm font-semibold text-slate-100">RBAC User Provisioning & Access Control</h3>
          </div>

          <form onSubmit={handleCreateUser} className="grid grid-cols-1 gap-3 rounded-lg border border-slate-800 bg-slate-950 p-3 md:grid-cols-5">
            <input
              type="email"
              required
              placeholder="Email address"
              value={newEmail}
              onChange={(e) => setNewEmail(e.target.value)}
              className="rounded border border-slate-700 bg-slate-900 px-2.5 py-1.5 text-xs text-slate-100"
            />
            <input
              type="text"
              required
              placeholder="Full name"
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              className="rounded border border-slate-700 bg-slate-900 px-2.5 py-1.5 text-xs text-slate-100"
            />
            <select
              value={newRole}
              onChange={(e) => setNewRole(e.target.value)}
              className="rounded border border-slate-700 bg-slate-900 px-2.5 py-1.5 text-xs text-slate-100"
            >
              <option value="analyst">analyst</option>
              <option value="reviewer">reviewer</option>
              <option value="auditor">auditor</option>
              <option value="admin">admin</option>
            </select>
            <input
              type="password"
              required
              minLength={12}
              aria-label="Initial user credential"
              value={newCredential}
              onChange={(e) => setNewCredential(e.target.value)}
              className="rounded border border-slate-700 bg-slate-900 px-2.5 py-1.5 text-xs text-slate-100"
            />
            <button
              type="submit"
              disabled={creatingUser}
              className="inline-flex items-center justify-center gap-1.5 rounded bg-indigo-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-indigo-500 disabled:opacity-50"
            >
              <UserPlus className="h-3.5 w-3.5" /> {creatingUser ? "Adding..." : "Create User"}
            </button>
          </form>

          {users.length === 0 ? (
            <EmptyBox text="No user records loaded." />
          ) : (
            <div className="overflow-x-auto rounded-lg border border-slate-800">
              <table className="w-full text-left text-xs">
                <thead className="bg-slate-950 text-slate-400">
                  <tr>
                    <th className="px-4 py-2">Email</th>
                    <th className="px-4 py-2">Full Name</th>
                    <th className="px-4 py-2">Role</th>
                    <th className="px-4 py-2">Status</th>
                    <th className="px-4 py-2">Rotation Flag</th>
                    <th className="px-4 py-2 text-right">Action</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-800">
                  {users.map((u) => (
                    <tr key={u.id} className="hover:bg-slate-800/30">
                      <td className="px-4 py-2 font-mono text-slate-100">{u.email}</td>
                      <td className="px-4 py-2 text-slate-300">{u.full_name}</td>
                      <td className="px-4 py-2 font-mono uppercase text-indigo-300">{u.role}</td>
                      <td className="px-4 py-2">
                        <StatusBadge value={u.is_active ? "active" : "rejected"} />
                      </td>
                      <td className="px-4 py-2 font-mono text-[11px] text-amber-300">
                        {u.must_rotate_password ? "Required" : "Rotated"}
                      </td>
                      <td className="px-4 py-2 text-right">
                        <button
                          disabled={u.id === user.id}
                          onClick={() => handleToggleUserStatus(u)}
                          className="rounded border border-slate-700 px-2.5 py-1 text-[11px] text-slate-200 hover:bg-slate-800 disabled:opacity-40"
                        >
                          {u.is_active ? "Deactivate" : "Reactivate"}
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      )}

      {/* 4. Append-Only Audit Log */}
      <div className="rounded-xl border border-slate-800 bg-slate-900 overflow-hidden">
        <div className="border-b border-slate-800 px-5 py-3">
          <h3 className="text-sm font-semibold text-slate-100">Append-Only Audit Log</h3>
          <p className="text-[11px] text-slate-400">
            Cryptographically ordered record of authentication, document, review, and configuration events.
          </p>
        </div>
        {auditLogs.length === 0 ? (
          <div className="p-8">
            <EmptyBox text="No audit logs available or insufficient audit:read permission." />
          </div>
        ) : (
          <>
            <div className="overflow-x-auto">
              <table className="w-full text-left text-xs">
                <thead className="bg-slate-950 text-slate-400">
                  <tr>
                    <th className="px-4 py-2.5">Timestamp</th>
                    <th className="px-4 py-2.5">Actor</th>
                    <th className="px-4 py-2.5">Action</th>
                    <th className="px-4 py-2.5">Resource</th>
                    <th className="px-4 py-2.5">Details</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-800">
                  {auditLogs.map((log) => (
                    <tr key={log.id} className="hover:bg-slate-800/30">
                      <td className="px-4 py-2 font-mono text-[11px] text-slate-400">
                        {new Date(log.created_at).toLocaleString()}
                      </td>
                      <td className="px-4 py-2 text-slate-200">
                        {log.actor_email || "system"}{" "}
                        <span className="font-mono text-[10px] text-indigo-300">({log.actor_role || "sys"})</span>
                      </td>
                      <td className="px-4 py-2 font-mono text-emerald-300">{log.action}</td>
                      <td className="px-4 py-2 font-mono text-[11px] text-slate-300">
                        {log.resource_type} {log.resource_id ? `(${String(log.resource_id).slice(0, 8)}…)` : ""}
                      </td>
                      <td className="max-w-xs truncate px-4 py-2 font-mono text-[11px] text-slate-400">
                        {JSON.stringify(log.details)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <PaginationBar pagination={auditPagination} onPageChange={onAuditPageChange} />
          </>
        )}
      </div>
    </div>
  );
}
