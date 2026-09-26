import { describe, expect, it } from "vitest";
import { resources } from "@/i18n";

function keys(obj: Record<string, unknown>, prefix = ""): string[] {
  return Object.entries(obj).flatMap(([k, v]) =>
    v && typeof v === "object"
      ? keys(v as Record<string, unknown>, `${prefix}${k}.`)
      : [`${prefix}${k}`],
  );
}

function placeholders(text: string): string[] {
  return [...text.matchAll(/\{\{\s*(\w+)\s*\}\}/g)].map((m) => m[1] ?? "").sort();
}

function lookup(obj: Record<string, unknown>, path: string): unknown {
  return path.split(".").reduce<unknown>((o, k) => (o as Record<string, unknown>)?.[k], obj);
}

describe("translations", () => {
  const en = resources.en ?? {};
  const zh = resources["zh-CN"] ?? {};

  it("has the same namespaces in every language", () => {
    expect(Object.keys(zh).sort()).toEqual(Object.keys(en).sort());
  });

  for (const ns of Object.keys(en)) {
    it(`zh-CN/${ns}.json has exactly the keys of en/${ns}.json`, () => {
      expect(keys(zh[ns] ?? {}).sort()).toEqual(keys(en[ns] ?? {}).sort());
    });

    it(`zh-CN/${ns}.json uses the same {{placeholders}} as English`, () => {
      for (const key of keys(en[ns] ?? {})) {
        const e = lookup(en[ns] ?? {}, key);
        const z = lookup(zh[ns] ?? {}, key);
        if (typeof e === "string" && typeof z === "string") {
          expect({ key, vars: placeholders(z) }).toEqual({ key, vars: placeholders(e) });
        }
      }
    });
  }

  it("keeps the required policy wording verbatim", () => {
    const common = en.common as Record<string, unknown>;
    expect(common.canvasNotice).toBe(
      "Personal access tokens are for your own use only; Canvas student tokens expire within 30 days.",
    );
    expect((common.disclosure as Record<string, string>).full).toBe(
      "When you ask your AI app about a course, it reads that course's materials from {{product}} and sends them to your AI provider under your own account. {{product}} itself stores nothing remotely. You're responsible for following each course's AI policy — and you can turn sharing off for any course.",
    );
  });
});
