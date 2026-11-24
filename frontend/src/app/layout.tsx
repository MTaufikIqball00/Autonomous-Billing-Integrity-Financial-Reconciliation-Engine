import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "ABIFR — Agentic Billing Integrity & Financial Reconciliation",
  description: "Evidence-driven enterprise financial reconciliation, contract auditing, and multi-agent billing integrity platform.",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className="dark">
      <body className="min-h-screen bg-slate-950 text-slate-100">{children}</body>
    </html>
  );
}
