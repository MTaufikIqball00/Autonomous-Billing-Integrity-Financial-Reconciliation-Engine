export const API_BASE =
  process.env.NEXT_PUBLIC_API_BASE_URL || "http://127.0.0.1:14800/api/v1";

export interface UserSession {
  id: string;
  email: string;
  full_name: string;
  role: "admin" | "reviewer" | "analyst" | "auditor" | string;
  csrf_token?: string;
  must_rotate_password?: boolean;
}

export interface PaginationMeta {
  total: number;
  limit: number;
  offset: number;
}

export function buildPaginationQuery(limit: number, offset: number): string {
  const safeLimit = Math.max(1, Math.min(200, Math.floor(limit)));
  const safeOffset = Math.max(0, Math.floor(offset));
  return `limit=${safeLimit}&offset=${safeOffset}`;
}

export function isPotentialAnomaly(f: { classification_type?: string }): boolean {
  const ct = (f.classification_type || "").toLowerCase();
  return (
    ct === "potential_anomaly" ||
    ct === "probable_duplicate" ||
    ct === "unresolved_discrepancy"
  );
}

class ApiClient {
  // Never stores JWT in sessionStorage or localStorage; relies strictly on HttpOnly cookies
  private csrf: string | null = null;

  setCsrf(csrf?: string | null) {
    this.csrf = csrf ?? null;
  }

  getCsrf(): string | null {
    return this.csrf;
  }

  private headers(extra?: Record<string, string>): Record<string, string> {
    const h: Record<string, string> = { ...(extra || {}) };
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
