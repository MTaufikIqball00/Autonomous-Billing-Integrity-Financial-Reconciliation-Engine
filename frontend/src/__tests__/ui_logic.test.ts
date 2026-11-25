import fs from "node:fs";
import path from "node:path";
import { describe, it, expect } from "vitest";
import {
  formatMoney,
  API_BASE,
  buildPaginationQuery,
  isPotentialAnomaly,
} from "../lib/api";

describe("ABIFR Frontend Financial Formatting, Pagination & Cookie Security", () => {
  it("formats currency-tagged amounts without fabricating or altering precision", () => {
    expect(formatMoney("1250.50", "USD")).toBe("USD 1250.50");
    expect(formatMoney("12.345", "KWD")).toBe("KWD 12.345");
    expect(formatMoney(null, "USD")).toBe("—");
    expect(formatMoney(undefined, "EUR")).toBe("—");
  });

  it("defaults to isolated Rust backend port 14800", () => {
    expect(API_BASE).toContain(":14800/api/v1");
  });

  it("clamps pagination limit (1..200) and non-negative offset safely", () => {
    expect(buildPaginationQuery(25, 50)).toBe("limit=25&offset=50");
    expect(buildPaginationQuery(999, -10)).toBe("limit=200&offset=0");
    expect(buildPaginationQuery(0, 12.8)).toBe("limit=1&offset=12");
  });

  it("distinguishes potential anomalies from confirmed rule violations", () => {
    expect(isPotentialAnomaly({ classification_type: "potential_anomaly" })).toBe(true);
    expect(isPotentialAnomaly({ classification_type: "probable_duplicate" })).toBe(true);
    expect(isPotentialAnomaly({ classification_type: "unresolved_discrepancy" })).toBe(true);
    expect(isPotentialAnomaly({ classification_type: "confirmed_rule_violation" })).toBe(false);
    expect(isPotentialAnomaly({ classification_type: "exact_duplicate" })).toBe(false);
  });

  it("never stores session JWTs in sessionStorage or localStorage and enforces credentials + CSRF", () => {
    const apiSource = fs.readFileSync(
      path.resolve(__dirname, "../lib/api.ts"),
      "utf-8"
    );
    expect(apiSource).not.toMatch(/sessionStorage\.(setItem|getItem)/);
    expect(apiSource).not.toMatch(/localStorage\.(setItem|getItem)/);
    expect(apiSource).toContain('credentials: "include"');
    expect(apiSource).toContain("X-CSRF-Token");
  });
});
