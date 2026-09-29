import { describe, expect, it } from "vitest";
import { estimateAmount, formatTokens, formatUsd, parseUsd, usdInputValue } from "./money";

describe("money", () => {
  it("formats micro-USD as dollars", () => {
    expect(formatUsd(2_000_000, "en")).toBe("$2.00");
    expect(formatUsd(4_960_000, "en")).toBe("$4.96");
    expect(formatUsd(0, "en")).toBe("$0.00");
    expect(formatUsd(2_000_000, "zh-CN")).toBe("US$2.00");
  });

  it("never shows an estimate below its upper bound", () => {
    expect(estimateAmount(69_750, "en")).toEqual({ kind: "upTo", amount: "$0.07" });
    expect(estimateAmount(60_001, "en")).toEqual({ kind: "upTo", amount: "$0.07" });
    expect(estimateAmount(9_999, "en")).toEqual({ kind: "lessThan", amount: "$0.01" });
  });

  it("parses the budget field exactly, without floating point", () => {
    expect(parseUsd("5")).toBe(5_000_000);
    expect(parseUsd(" 2.50 ")).toBe(2_500_000);
    expect(parseUsd("$3.1")).toBe(3_100_000);
    expect(parseUsd("0.29")).toBe(290_000);
    expect(parseUsd("1,000")).toBe(1_000_000_000);
    for (const bad of ["", "-1", "abc", "1.234", "5e3"]) expect(parseUsd(bad)).toBeNull();
    expect(usdInputValue(5_000_000)).toBe("5.00");
  });

  it("shortens token counts", () => {
    expect(formatTokens(3_400_000, "en")).toBe("3.4M");
    expect(formatTokens(42_000, "en")).toBe("42K");
  });
});
