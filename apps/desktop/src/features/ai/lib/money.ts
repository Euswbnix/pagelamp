// Money in the AI screens: the facade stores and sends integer micro-USD (1 USD = 1 000 000),
// never floating-point dollars. Only formatting and parsing the budget field touch dollars.

export const MICRO_PER_USD = 1_000_000;
const MICRO_PER_CENT = 10_000;

function currency(locale: string) {
  return new Intl.NumberFormat(locale, {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });
}

/** "$2.00" (en) / "US$2.00" (zh-CN), rounded to the nearest cent. */
export function formatUsd(micro: number, locale: string): string {
  return currency(locale).format(Math.round(micro / MICRO_PER_CENT) / 100);
}

/**
 * How to show an upper-bound estimate: under a cent it's "less than $0.01"; otherwise the bound
 * rounded UP to the cent, so the shown amount is never below the real bound.
 */
export function estimateAmount(
  micro: number,
  locale: string,
): { kind: "lessThan" | "upTo"; amount: string } {
  if (micro < MICRO_PER_CENT)
    return { kind: "lessThan", amount: formatUsd(MICRO_PER_CENT, locale) };
  const cents = Math.ceil(micro / MICRO_PER_CENT);
  return { kind: "upTo", amount: currency(locale).format(cents / 100) };
}

/** The budget field: "5", "2.50", "$3", "1,000" → micro-USD; anything else → null. */
export function parseUsd(text: string): number | null {
  const cleaned = text
    .trim()
    .replace(/^(US)?\$/i, "")
    .replace(/,/g, "")
    .trim();
  if (!/^\d+(\.\d{0,2})?$/.test(cleaned)) return null;
  const [whole = "0", fraction = ""] = cleaned.split(".");
  return Number(whole) * MICRO_PER_USD + Number(fraction.padEnd(2, "0")) * MICRO_PER_CENT;
}

/** The budget field's text for a stored value: "5.00". */
export function usdInputValue(micro: number): string {
  return (Math.round(micro / MICRO_PER_CENT) / 100).toFixed(2);
}

/** 3 400 000 → "3.4M", 42 000 → "42K" (token counts). */
export function formatTokens(count: number, locale: string): string {
  return new Intl.NumberFormat(locale, { notation: "compact", maximumFractionDigits: 1 }).format(
    count,
  );
}
