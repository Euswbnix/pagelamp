import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { renderRoute } from "@/test/render";
import { recentMonths } from "./UsageSection";

function monthName(monthsAgo: number) {
  const now = new Date();
  return new Intl.DateTimeFormat("en", { year: "numeric", month: "long" }).format(
    new Date(now.getFullYear(), now.getMonth() - monthsAgo, 1),
  );
}

async function usageSection() {
  const heading = await screen.findByRole("heading", { level: 2, name: "AI usage" });
  const region = heading.closest("section");
  if (!region) throw new Error("no AI usage section");
  return region;
}

describe("Settings → AI usage", () => {
  it("lists this month's runs, tokens and cost, with the budget", async () => {
    renderRoute("/settings", { scenario: "ai-key" });
    const section = await usageSection();
    const table = await within(section).findByRole("table", {
      name: `AI usage in ${monthName(0)}`,
    });
    const plans = within(table).getByRole("row", { name: /gpt-6-luna/ });
    expect(within(plans).getByText("Study plans")).toBeInTheDocument();
    expect(within(plans).getByText("6")).toBeInTheDocument();
    expect(within(plans).getByText("45K in, 14.4K out")).toBeInTheDocument();
    expect(within(plans).getByText("$0.40")).toBeInTheDocument();
    // An estimated cost is marked, and so is the total that includes it.
    expect(within(table).getByRole("row", { name: /gpt-5\.4-mini/ })).toHaveTextContent("≈ $1.60");
    expect(within(table).getByRole("row", { name: /Total/ })).toHaveTextContent("≈ $2.00");
    expect(within(section).getByText("$2.00 of $5.00 used this month")).toBeInTheDocument();
  });

  it("shows an earlier month", async () => {
    const { user } = renderRoute("/settings", { scenario: "ai-key" });
    const section = await usageSection();
    await user.click(within(section).getByRole("combobox", { name: "Month" }));
    await user.click(await screen.findByRole("option", { name: monthName(1) }));
    const table = await within(section).findByRole("table", {
      name: `AI usage in ${monthName(1)}`,
    });
    expect(within(table).getByRole("row", { name: /Total/ })).toHaveTextContent("$3.10");
    expect(within(section).queryByText(/used this month/)).toBeNull();
  });

  it("counts local runs as free and unpriced ones in tokens only", async () => {
    renderRoute("/settings", { scenario: "ai-local" });
    const section = await usageSection();
    const table = await within(section).findByRole("table");
    expect(within(table).getAllByText("Free")).toHaveLength(2);
    expect(within(table).getByRole("row", { name: /gpt-oss:120b-cloud/ })).toHaveTextContent(
      "No price",
    );
  });

  it("says when a month has no runs", async () => {
    renderRoute("/settings");
    const section = await usageSection();
    expect(await within(section).findByText(`No AI runs in ${monthName(0)}.`)).toBeInTheDocument();
  });

  it("offers the 13 months the ledger keeps", () => {
    const months = recentMonths("2026-01-15");
    expect(months).toHaveLength(13);
    expect(months.slice(0, 2)).toEqual(["2026-01-01", "2025-12-01"]);
    expect(months.at(-1)).toBe("2025-01-01");
  });
});
