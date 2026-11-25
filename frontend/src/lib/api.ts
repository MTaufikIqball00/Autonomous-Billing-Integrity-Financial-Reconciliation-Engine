import { handleDemoRequest } from "./demoData";

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
  private demoMode = false;

  setDemoMode(enabled: boolean) {
    this.demoMode = enabled;
  }

  isDemoMode(): boolean {
    return this.demoMode;
  }

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
    if (this.demoMode) {
      return handleDemoRequest<T>(path, init);
    }
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
    if (this.demoMode) {
      return Promise.reject(
        new Error(
          "[Demo Mode] File OCR & immutable storage require the local Rust + RapidOCR backend."
        )
      );
    }
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
    if (this.demoMode) {
      const content =
        format === "csv"
          ? [
              "finding_id,category,classification_type,severity,vendor,invoice_number,variance_amount,currency,rule_applied",
              "fnd-demo-1,CONTRACT_PRICE_MISMATCH,confirmed_rule_violation,high,[DEMO] Nusantara Cloud Infrastructure Ltd.,DEMO-INV-2026-081,500.00,USD,CONTRACT_UNIT_PRICE_CHECK",
              "fnd-demo-2,UNAUTHORIZED_CHARGE,confirmed_rule_violation,medium,[DEMO] Nusantara Cloud Infrastructure Ltd.,DEMO-INV-2026-081,55.00,USD,ALLOWED_ADDITIONAL_CHARGES_CHECK",
              "fnd-demo-3,EXTRACTION_UNCERTAIN,potential_anomaly,high,[DEMO] EuroData Security GmbH,,,EUR,PROMPT_INJECTION_QUARANTINE_GUARD",
            ].join("\n")
          : `[DEMO MODE - SYNTHETIC REPORT SNAPSHOT]\nReport ID: ${reportId}\nFormat requested: ${format.toUpperCase()}\nNotice: Binary XLSX/PDF generation is performed by the Rust backend (rust_xlsxwriter / PDF 1.4 generator).`;
      const ext = format === "csv" ? "csv" : "txt";
      const blob = new Blob([content], {
        type: format === "csv" ? "text/csv;charset=utf-8" : "text/plain;charset=utf-8",
      });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `abifr-demo-report-${reportId}.${ext}`;
      document.body.appendChild(a);
      a.click();
      a.remove();
      URL.revokeObjectURL(url);
      return;
    }

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
