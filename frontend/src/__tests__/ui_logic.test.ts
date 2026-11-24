import { describe, it, expect } from "vitest";
import { formatMoney, API_BASE } from "../lib/api";

describe("ABIFR Frontend Financial Formatting & API Config", () => {
  it("formats currency-tagged amounts without fabricating or altering precision", () => {
    expect(formatMoney("1250.50", "USD")).toBe("USD 1250.50");
    expect(formatMoney("12.345", "KWD")).toBe("KWD 12.345");
    expect(formatMoney(null, "USD")).toBe("—");
    expect(formatMoney(undefined, "EUR")).toBe("—");
  });

  it("defaults to isolated Rust backend port 14800", () => {
    expect(API_BASE).toContain(":14800/api/v1");
  });
});
