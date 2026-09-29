#!/usr/bin/env node
// Refresh crates/pagelamp-llm/data/prices.json from the models.dev catalog (MIT; its licence is
// in data/models-dev.LICENSE). Run at release time, review the diff, commit. PageLamp never
// fetches prices at runtime (owner decision D20: no new network destination).
//
//   node crates/pagelamp-llm/scripts/refresh-prices.mjs            # fetch https://models.dev/api.json
//   node crates/pagelamp-llm/scripts/refresh-prices.mjs --from x.json
//
// Kept per model: prices as integer micro-USD per million tokens (base, plus the context tiers
// that cost more above a prompt size), the context and output limits, and what its effort
// setting allows (from `reasoning_options`). Only text models with a price, of the providers
// PageLamp has presets for.

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const out = join(here, "..", "data", "prices.json");

// models.dev provider id → PageLamp preset id.
const PROVIDERS = { openai: "openai", anthropic: "anthropic", google: "gemini", openrouter: "openrouter" };

async function catalog() {
  const from = process.argv.indexOf("--from");
  if (from !== -1) return JSON.parse(readFileSync(process.argv[from + 1], "utf8"));
  const response = await fetch("https://models.dev/api.json");
  if (!response.ok) throw new Error(`models.dev answered ${response.status}`);
  return response.json();
}

/** USD per million tokens → integer micro-USD per million tokens. */
const micro = (usd) => (typeof usd === "number" && usd >= 0 ? Math.round(usd * 1_000_000) : null);

/** Prices of one tier (output includes a separate reasoning price when there is one). */
function prices(cost) {
  const output = [micro(cost.output), micro(cost.reasoning)].filter((p) => p !== null);
  return {
    input: micro(cost.input),
    output: output.length ? Math.max(...output) : null,
    cache_read: micro(cost.cache_read),
    cache_write: micro(cost.cache_write),
  };
}

/** Context tiers: prices that apply when the prompt is longer than `above` tokens. */
function tiers(cost) {
  const found = (cost.tiers ?? [])
    .filter((t) => t.tier?.type === "context" && typeof t.tier.size === "number")
    .map((t) => ({ above: t.tier.size, ...prices({ ...cost, ...t }) }));
  if (cost.context_over_200k && !found.some((t) => t.above === 200000)) {
    found.push({ above: 200000, ...prices({ ...cost, ...cost.context_over_200k }) });
  }
  return found.sort((a, b) => a.above - b.above);
}

function effort(options) {
  const values = (options ?? []).filter((o) => o.type === "effort").flatMap((o) => o.values ?? []);
  const lowest = ["none", "minimal", "low"].find((v) => values.includes(v)) ?? null;
  return { effort: values.length > 0, lowest };
}

const data = await catalog();
const providers = {};
for (const [source, preset] of Object.entries(PROVIDERS)) {
  const models = {};
  for (const [id, model] of Object.entries(data[source]?.models ?? {}).sort(([a], [b]) => a.localeCompare(b))) {
    const outputs = model.modalities?.output ?? [];
    if (!outputs.includes("text") || outputs.some((o) => o !== "text")) continue;
    const cost = model.cost ?? {};
    const base = prices(cost);
    if (base.input === null || base.output === null) continue;
    const { effort: hasEffort, lowest } = effort(model.reasoning_options);
    const toggle = (model.reasoning_options ?? []).some((o) => o.type === "toggle");
    models[id] = {
      ...base,
      tiers: tiers(cost),
      context: model.limit?.context || null,
      max_output: model.limit?.output || null,
      structured_output: model.structured_output === true,
      effort: hasEffort,
      lowest_effort: lowest,
      // Reasons, and can't be told not to: no on/off toggle and no "none"/"minimal" effort.
      thinking_always_on: model.reasoning === true && !toggle && hasEffort && !["none", "minimal"].includes(lowest),
    };
  }
  providers[preset] = models;
}

const snapshot = {
  source: "https://models.dev/api.json (MIT licence: data/models-dev.LICENSE)",
  fetched: new Date().toISOString().slice(0, 10),
  unit: "micro-USD per million tokens",
  providers,
};
writeFileSync(out, `${JSON.stringify(snapshot, null, 1)}\n`);
const counts = Object.entries(providers).map(([p, m]) => `${p} ${Object.keys(m).length}`);
console.log(`wrote ${out}: ${counts.join(", ")}`);
