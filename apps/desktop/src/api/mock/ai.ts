// The AI setup part of the mock API (M1). It mirrors what the facade decides — backend state,
// the disclosure version, the estimate's upper bound and `would_block`, question (b) — so the
// screens can be built and tested before the Rust side exists. Keys are validated and dropped:
// only the last 4 characters are kept, as in the real facade.

import type { PageLampApi } from "../client";
import { ApiError } from "../errors";
import {
  type AiBackendStatus,
  type AiFeature,
  type AiStatus,
  type BackendRef,
  type BlockReason,
  backendKey,
  type CostEstimate,
  type CourseWithSharing,
  type DisclosureFacts,
  type Effort,
  type EstimateRequest,
  type LocalServer,
  type ModelChoice,
  type ModelInfo,
  type ModelProviderRecord,
  materialSharing,
  type ProbeReport,
  type ProviderPreset,
  type UsageRow,
  type UsageSummary,
} from "../provisional/ai";
import { aiMaterialsState } from "../types";
import {
  CODING_PLAN_HOSTS,
  CODING_PLAN_KEY_PREFIXES,
  MOCK_MODELS,
  MOCK_PRESETS,
  MOCK_PRICES,
  mockUsage,
  ollamaCloudFacts,
} from "./ai-fixtures";
import type { MockCourse, MockScenario } from "./fixtures";

type AiApi = Pick<
  PageLampApi,
  | "setCourseMaterialSharing"
  | "aiStatus"
  | "modelProviderPresets"
  | "addModelProvider"
  | "updateModelProviderKey"
  | "removeModelProvider"
  | "detectLocalServers"
  | "listModels"
  | "testModel"
  | "setFeatureModel"
  | "acknowledgeAiDisclosure"
  | "acknowledgeUnpricedModel"
  | "setMonthlyBudget"
  | "estimateGeneration"
  | "usageSummary"
  | "removeAllAiData"
>;

export interface MockAiContext {
  scenario: MockScenario;
  now: () => Date;
  /** Waits the mock's latency plus `extra` ms. */
  delay: (extra?: number) => Promise<void>;
  courses: () => MockCourse[];
  findCourse: (courseId: string) => MockCourse;
}

/** D18: US$5 soft cap, warn at 80%. */
export const DEFAULT_BUDGET_MICRO_USD = 5_000_000;
const WARN_AT_PERCENT = 80;
const FEATURES: AiFeature[] = [
  "study_plan",
  "weekly_explanation",
  "weekly_note",
  "course_calendar",
];

function presetOf(id: string): ProviderPreset {
  const preset = MOCK_PRESETS.find((p) => p.id === id);
  if (!preset) throw new ApiError("invalid", `Unknown preset "${id}".`);
  return preset;
}

function isLoopback(url: URL): boolean {
  return ["localhost", "127.0.0.1", "[::1]"].includes(url.hostname);
}

/** A short, stable id part for a custom endpoint (like the backend's `custom-<short-hash>`). */
function shortHash(text: string): string {
  let h = 0;
  for (const ch of text) h = (h * 31 + (ch.codePointAt(0) ?? 0)) >>> 0;
  return h.toString(36).slice(0, 6);
}

function last4(key: string): string {
  return key.trim().slice(-4);
}

/** Input tokens and output limits per feature (a rough stand-in for the backend's estimator). */
function workload(req: EstimateRequest): { input: number; output: number } {
  switch (req.feature) {
    case "study_plan":
      return { input: 6_000 + 1_500 * req.courses.length, output: 8_000 };
    case "weekly_explanation":
      return { input: 45_000, output: 6_000 };
    case "weekly_note":
      return { input: 3_000, output: 1_500 };
    case "course_calendar":
      return { input: 15_000 * Math.max(1, req.courses.length), output: 4_000 };
  }
}

const REASONING: Record<Effort, number> = { lowest: 0, low: 2_000, medium: 8_000, high: 24_000 };

export function createMockAi(ctx: MockAiContext): AiApi {
  const { scenario, now } = ctx;
  const created = (daysAgo: number) =>
    new Date(now().getTime() - daysAgo * 24 * 60 * 60 * 1000).toISOString();

  // ----- state ---------------------------------------------------------------------------------
  const providers: ModelProviderRecord[] = [];
  const acknowledged = new Map<string, number>();
  const unpricedAcks = new Set<string>();
  const features = new Map<AiFeature, ModelChoice | null>(FEATURES.map((f) => [f, null]));
  let budget: number | null = DEFAULT_BUDGET_MICRO_USD;
  let usage: [number, UsageRow][] = mockUsage(scenario);

  function record(presetId: string, providerId: string, baseUrl: string, key: string | null) {
    const preset = presetOf(presetId);
    const url = new URL(baseUrl);
    const rec: ModelProviderRecord = {
      provider_id: providerId,
      preset: preset.id,
      label: preset.id === "custom" ? url.host : preset.label,
      wire: preset.wire,
      base_url: baseUrl,
      on_device: isLoopback(url),
      key_last4: key ? last4(key) : null,
      created_at: created(20),
    };
    providers.push(rec);
    return rec;
  }
  function route(providerId: string, model: string, effort: Effort = "lowest", only?: AiFeature) {
    for (const feature of FEATURES) {
      if (!only || feature === only) {
        features.set(feature, {
          backend: { kind: "provider", provider_id: providerId },
          model,
          effort,
        });
      }
    }
  }
  const openaiKey = "sk-demo-000000000000007Qx2";
  switch (scenario) {
    case "ai-key":
    case "ai-budget":
    case "ai-unpriced": {
      record("openai", "openai", "https://api.openai.com/v1", openaiKey);
      acknowledged.set("provider:openai", presetOf("openai").data_policy.version);
      route("openai", "gpt-6-luna");
      route(
        "openai",
        scenario === "ai-unpriced" ? "gpt-6-preview-0929" : "gpt-5.4-mini",
        "low",
        "weekly_explanation",
      );
      break;
    }
    case "ai-local":
      record("ollama", "ollama", "http://127.0.0.1:11434", null);
      acknowledged.set("provider:ollama", presetOf("ollama").data_policy.version);
      route("ollama", "qwen3.5:9b");
      break;
    case "ai-disclosure-changed":
      record("anthropic", "anthropic", "https://api.anthropic.com", "sk-ant-demo-0000Hq8e");
      // Acknowledged an older version of the facts: the sheet has to be read again.
      acknowledged.set("provider:anthropic", presetOf("anthropic").data_policy.version - 1);
      route("anthropic", "claude-haiku-4-5");
      break;
    case "ai-errors": {
      const url = "https://llm.demo.test/v1";
      record("custom", `custom-${shortHash(url)}`, url, "demo-key-0000Zt3k");
      acknowledged.set(`provider:custom-${shortHash(url)}`, presetOf("custom").data_policy.version);
      route(`custom-${shortHash(url)}`, "demo-model-large");
      break;
    }
    default:
      break;
  }

  // ----- helpers -------------------------------------------------------------------------------
  function findProvider(providerId: string): ModelProviderRecord {
    const found = providers.find((p) => p.provider_id === providerId);
    if (!found) throw new ApiError("not_found", `No model provider "${providerId}".`);
    return found;
  }
  function providerOf(backend: BackendRef): ModelProviderRecord {
    if (backend.kind !== "provider") {
      throw new ApiError("blocked", "Not available in this build.", {
        blocked: "backend_disabled_in_this_build",
      });
    }
    return findProvider(backend.provider_id);
  }
  function modelsOf(provider: ModelProviderRecord): ModelInfo[] {
    return MOCK_MODELS[provider.preset] ?? [];
  }
  function modelInfo(provider: ModelProviderRecord, model: string): ModelInfo | null {
    return modelsOf(provider).find((m) => m.id === model) ?? null;
  }
  function disclosureOf(provider: ModelProviderRecord): DisclosureFacts {
    const base = presetOf(provider.preset).data_policy;
    // An Ollama provider routed to a cloud model discloses the cloud facts.
    const usesCloudModel = [...features.values()].some(
      (c) =>
        c?.backend.kind === "provider" &&
        c.backend.provider_id === provider.provider_id &&
        modelInfo(provider, c.model)?.on_device === false,
    );
    if (provider.preset === "ollama" && usesCloudModel) return ollamaCloudFacts(base);
    return base;
  }
  function backendStatus(provider: ModelProviderRecord): AiBackendStatus {
    const backend: BackendRef = { kind: "provider", provider_id: provider.provider_id };
    const disclosure = disclosureOf(provider);
    const acked = acknowledged.get(backendKey(backend)) ?? null;
    const preset = presetOf(provider.preset);
    const problems: AiBackendStatus["problems"] = [];
    if (preset.needs_key && !provider.key_last4) problems.push("key_missing");
    if (acked !== null && acked !== disclosure.version) problems.push("disclosure_changed");
    const state = problems.includes("key_missing")
      ? "needs_setup"
      : acked !== disclosure.version
        ? "needs_disclosure"
        : "ready";
    return {
      backend,
      label: provider.label,
      kind: provider.on_device ? "local" : "api_key",
      state,
      problems,
      provider,
      disclosure,
      disclosure_acknowledged: acked,
    };
  }
  function monthOffset(month: string | null): number {
    if (!month) return 0;
    const [y, m] = month.split("-").map(Number);
    if (!y || !m) throw new ApiError("invalid", "Expected a date like 2026-09-01.");
    const today = now();
    return (today.getFullYear() - y) * 12 + (today.getMonth() + 1 - m);
  }
  function spentThisMonth(): number {
    return usage
      .filter(([ago, r]) => ago === 0 && r.backend_kind === "api_key")
      .reduce((sum, [, r]) => sum + (r.micro_usd ?? 0), 0);
  }
  function budgetStatus() {
    return {
      monthly_micro_usd: budget,
      spent_micro_usd: spentThisMonth(),
      warn_at_percent: WARN_AT_PERCENT,
    };
  }
  function validateBaseUrl(preset: ProviderPreset, baseUrl: string | null): string {
    const value = preset.base_url_editable
      ? baseUrl?.trim() || preset.default_base_url
      : preset.default_base_url;
    if (!value) throw new ApiError("invalid", "Enter the endpoint's address.");
    let url: URL;
    try {
      url = new URL(value);
    } catch {
      throw new ApiError("invalid", "That is not a web address.");
    }
    if (url.protocol !== "https:" && !(url.protocol === "http:" && isLoopback(url))) {
      throw new ApiError("invalid", "Use an https:// address (http only on this computer).");
    }
    return value.replace(/\/+$/, "");
  }
  function validateKey(preset: ProviderPreset, baseUrl: string, key: string | null) {
    const target = baseUrl.replace(/^https?:\/\//, "");
    const codingPlan =
      CODING_PLAN_HOSTS.some((h) => target.startsWith(h)) ||
      (key !== null && CODING_PLAN_KEY_PREFIXES.some((p) => key.trim().startsWith(p)));
    if (codingPlan) {
      // The real message quotes the vendor's own terms; this one is made up.
      throw new ApiError(
        "blocked",
        "“This plan's keys may only be used in the vendor's own coding tools.”",
        { blocked: "coding_plan_key" },
      );
    }
    if (preset.needs_key && !key?.trim()) throw new ApiError("invalid", "Enter the API key.");
    if (key?.includes("bad")) {
      throw new ApiError("model", "The provider rejected this key.", {
        model_error: "auth_rejected",
      });
    }
    if (baseUrl.includes("offline")) {
      throw new ApiError("model", "Couldn't reach the provider.", { model_error: "network" });
    }
  }
  function courseGate(courseId: string, onDevice: boolean): BlockReason | null {
    const c = ctx.findCourse(courseId);
    if (c.course.hidden) return "course_hidden";
    const state = aiMaterialsState(c.course);
    if (state === "withheld_by_policy") return "course_policy_prohibited";
    if (state === "turned_off") return "course_ai_turned_off";
    if (!onDevice && materialSharing(c.course) === "not_allowed") {
      return "material_sharing_not_allowed";
    }
    return null;
  }

  // ----- the API -------------------------------------------------------------------------------
  return {
    setCourseMaterialSharing: async (courseId, answer) => {
      await ctx.delay();
      (ctx.findCourse(courseId).course as CourseWithSharing).material_sharing = answer;
    },

    aiStatus: async (): Promise<AiStatus> => {
      await ctx.delay();
      return structuredClone({
        backends: providers.map(backendStatus),
        features: FEATURES.map((feature) => ({ feature, choice: features.get(feature) ?? null })),
        budget: budgetStatus(),
      });
    },

    modelProviderPresets: async () => {
      await ctx.delay();
      return structuredClone(MOCK_PRESETS);
    },

    addModelProvider: async (presetId, baseUrl, apiKey) => {
      await ctx.delay(600);
      const preset = presetOf(presetId);
      const url = validateBaseUrl(preset, baseUrl);
      validateKey(preset, url, preset.needs_key ? apiKey : null);
      let id = preset.id === "custom" ? `custom-${shortHash(url)}` : preset.id;
      if (providers.some((p) => p.provider_id === id)) {
        let n = 2;
        while (providers.some((p) => p.provider_id === `${id}-${n}`)) n += 1;
        id = `${id}-${n}`;
      }
      const rec = record(preset.id, id, url, preset.needs_key ? apiKey : null);
      rec.created_at = now().toISOString();
      return structuredClone(rec);
    },

    updateModelProviderKey: async (providerId, apiKey) => {
      await ctx.delay(600);
      const provider = findProvider(providerId);
      validateKey(presetOf(provider.preset), provider.base_url, apiKey);
      provider.key_last4 = last4(apiKey);
      return structuredClone(provider);
    },

    removeModelProvider: async (providerId) => {
      await ctx.delay();
      findProvider(providerId);
      providers.splice(
        providers.findIndex((p) => p.provider_id === providerId),
        1,
      );
      acknowledged.delete(`provider:${providerId}`);
      for (const [feature, choice] of features) {
        if (choice?.backend.kind === "provider" && choice.backend.provider_id === providerId) {
          features.set(feature, null);
        }
      }
    },

    detectLocalServers: async (): Promise<LocalServer[]> => {
      await ctx.delay(300);
      return [
        { kind: "ollama", base_url: "http://127.0.0.1:11434", running: true },
        { kind: "lm_studio", base_url: "http://127.0.0.1:1234/v1", running: false },
      ];
    },

    listModels: async (backend) => {
      await ctx.delay(400);
      const provider = providerOf(backend);
      if (scenario === "ai-errors") {
        throw new ApiError("model", "Couldn't reach llm.demo.test.", { model_error: "network" });
      }
      return structuredClone(modelsOf(provider));
    },

    testModel: async (backend, model): Promise<ProbeReport> => {
      await ctx.delay(900);
      const provider = providerOf(backend);
      if (scenario === "ai-errors") {
        throw new ApiError("model", "Rate limited.", {
          model_error: "rate_limited",
          retry_after_secs: 20,
        });
      }
      const info = modelInfo(provider, model);
      if (!info)
        throw new ApiError("model", `No model "${model}".`, { model_error: "model_not_found" });
      return {
        ok: true,
        latency_ms: info.on_device ? 2_300 : 900,
        structured_output_tier:
          provider.wire === "chat_completions" ? "json_object" : "native_schema",
        thinking_always_on: info.reasoning_always_on,
        error: null,
      };
    },

    setFeatureModel: async (feature, choice) => {
      await ctx.delay();
      if (choice) {
        const provider = providerOf(choice.backend);
        if (!modelInfo(provider, choice.model)) {
          throw new ApiError("invalid", `No model "${choice.model}".`);
        }
      }
      features.set(feature, choice ? structuredClone(choice) : null);
    },

    acknowledgeAiDisclosure: async (backend, version) => {
      await ctx.delay();
      const current = disclosureOf(providerOf(backend)).version;
      if (version !== current) {
        throw new ApiError("invalid", "The disclosure changed; read it again.");
      }
      acknowledged.set(backendKey(backend), version);
    },

    acknowledgeUnpricedModel: async (backend, model) => {
      await ctx.delay();
      providerOf(backend);
      unpricedAcks.add(`${backendKey(backend)}/${model}`);
    },

    setMonthlyBudget: async (microUsd) => {
      await ctx.delay();
      if (microUsd !== null && (!Number.isInteger(microUsd) || microUsd < 0)) {
        throw new ApiError("invalid", "The budget must be a whole number of micro-dollars ≥ 0.");
      }
      budget = microUsd;
    },

    estimateGeneration: async (req): Promise<CostEstimate> => {
      await ctx.delay();
      const choice = features.get(req.feature) ?? null;
      const { input, output } = workload(req);
      const blank: CostEstimate = {
        micro_usd_upper: null,
        input_tokens: input,
        max_output_tokens: output,
        reasoning_allowance: 0,
        repair_possible: false,
        price_known: false,
        would_block: null,
      };
      if (!choice) return { ...blank, would_block: "no_model_chosen" };
      const provider = providerOf(choice.backend);
      const info = modelInfo(provider, choice.model);
      const onDevice = info?.on_device ?? false;
      const courses =
        req.feature === "weekly_explanation"
          ? [req.course]
          : req.feature === "course_calendar"
            ? req.courses
            : [];
      let block: BlockReason | null = null;
      for (const course of courses) block ??= courseGate(course, onDevice);
      const status = backendStatus(provider);
      if (!block && status.state !== "ready") block = "disclosure_not_acknowledged";

      const reasoning = Math.max(REASONING[choice.effort], info?.reasoning_always_on ? 8_000 : 0);
      const repair = provider.wire === "chat_completions";
      const price = MOCK_PRICES[choice.model];
      let upper: number | null = null;
      if (onDevice) upper = 0;
      else if (price && status.kind === "api_key") {
        upper = Math.ceil(
          ((input * price[0] + (output + reasoning) * price[1]) / 1_000_000) * (repair ? 2 : 1),
        );
      }
      const priceKnown = upper !== null;
      if (!block && status.kind === "api_key" && !priceKnown) {
        if (!unpricedAcks.has(`${backendKey(choice.backend)}/${choice.model}`)) {
          block = "price_unknown_not_acknowledged";
        }
      }
      if (!block && upper !== null && upper > 0 && budget !== null) {
        if (spentThisMonth() + upper > budget) block = "budget_reached";
      }
      return {
        micro_usd_upper: upper,
        input_tokens: input,
        max_output_tokens: output,
        reasoning_allowance: reasoning,
        repair_possible: repair,
        price_known: priceKnown,
        would_block: block,
      };
    },

    usageSummary: async (month): Promise<UsageSummary> => {
      await ctx.delay();
      const ago = monthOffset(month);
      const today = now();
      const first = new Date(today.getFullYear(), today.getMonth() - ago, 1);
      const iso = `${first.getFullYear()}-${String(first.getMonth() + 1).padStart(2, "0")}-01`;
      const rows = usage.filter(([a]) => a === ago).map(([, r]) => r);
      return structuredClone({
        month: iso,
        rows,
        total_micro_usd: rows.reduce((sum, r) => sum + (r.micro_usd ?? 0), 0),
        budget: budgetStatus(),
      });
    },

    removeAllAiData: async () => {
      await ctx.delay(300);
      providers.length = 0;
      acknowledged.clear();
      unpricedAcks.clear();
      for (const feature of FEATURES) features.set(feature, null);
      budget = DEFAULT_BUDGET_MICRO_USD;
      usage = [];
    },
  };
}
