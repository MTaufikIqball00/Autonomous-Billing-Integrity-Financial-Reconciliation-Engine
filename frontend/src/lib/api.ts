export const API_BASE =
  process.env.NEXT_PUBLIC_API_BASE_URL || "http://127.0.0.1:14800/api/v1";

export interface UserSession {
  id: string;
  email: string;
  full_name: string;
  role: "admin" | "reviewer" | "analyst" | "auditor" | string;
  csrf_token?: string;
}

class ApiClient {
  private token: string | null = null;
  private csrf: string | null = null;

  setAuth(token: string | null, csrf?: string | null) {
    this.token = token;
    this.csrf = csrf ?? null;
    if (typeof window !== "undefined") {
      if (token) {
        sessionStorage.setItem("abifr_token", token);
        if (csrf) sessionStorage.setItem("abifr_csrf", csrf);
      } else {
        sessionStorage.removeItem("abifr_token");
        sessionStorage.removeItem("abifr_csrf");
      }
    }
  }

  hydrate() {
    if (typeof window !== "undefined") {
      this.token = sessionStorage.getItem("abifr_token");
      this.csrf = sessionStorage.getItem("abifr_csrf");
    }
  }

  getToken(): string | null {
    return this.token;
  }

  private headers(extra?: Record<string, string>): Record<string, string> {
    const h: Record<string, string> = { ...(extra || {}) };
    if (this.token) h["Authorization"] = `Bearer ${this.token}`;
    if (this.csrf) h["X-CSRF-Token"] = this.csrf;
    return h;
  }

  async request<T = any>(path: string, init: RequestInit = {}): Promise<T> {
    const isJson = init.body && !(init.body instanceof FormData);
    const res = await fetch(`${API_BASE}${path}`, {
      ...init,
      credentials: "include",
      headers: this.headers({
        ...(isJson ? { "Content-Type": "application/json" } : {}),
        ...(init.headers as Record<string, string> | undefined),
      }),
    });

    if (!res.ok) {
      let msg = `HTTP ${res.status}`;
      try {
        const err = await res.json();
        msg = err.message || err.error || msg;
      } catch {
        // ignore parse failure
      }
      throw new Error(msg);
    }
    return res.json();
  }

  uploadDocument(
    file: File,
    onProgress?: (pct: number) => void,
    vendorId?: string
  ): Promise<any> {
    return new Promise((resolve, reject) => {
      const form = new FormData();
      form.append("file", file);
      if (vendorId) form.append("vendor_id", vendorId);

      const xhr = new XMLHttpRequest();
      xhr.open("POST", `${API_BASE}/documents/upload`);
      xhr.withCredentials = true;
      if (this.token) xhr.setRequestHeader("Authorization", `Bearer ${this.token}`);
      if (this.csrf) xhr.setRequestHeader("X-CSRF-Token", this.csrf);

      xhr.upload.onprogress = (ev) => {
        if (ev.lengthComputable && onProgress) {
          onProgress(Math.round((ev.loaded / ev.total) * 100));
        }
      };
      xhr.onload = () => {
        try {
          const data = JSON.parse(xhr.responseText);
          if (xhr.status >= 200 && xhr.status < 300) {
            resolve(data);
          } else {
            reject(new Error(data.message || `Upload failed (${xhr.status})`));
          }
        } catch {
          reject(new Error(`Upload failed (${xhr.status})`));
        }
      };
      xhr.onerror = () => reject(new Error("Network error uploading document."));
      xhr.send(form);
    });
  }

  async downloadReport(reportId: string, format: "csv" | "xlsx" | "pdf") {
    const res = await fetch(`${API_BASE}/reports/${reportId}/export?format=${format}`, {
      credentials: "include",
      headers: this.headers(),
    });
    if (!res.ok) {
      throw new Error(`Export failed (${res.status})`);
    }
    const blob = await res.blob();
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `abifr-report-${reportId}.${format}`;
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
  }
}

export const api = new ApiClient();

export function formatMoney(val: string | number | null | undefined, currency?: string | null): string {
  if (val === null || val === undefined || val === "") return "—";
  const str = String(val);
  return currency ? `${currency} ${str}` : str;
}
