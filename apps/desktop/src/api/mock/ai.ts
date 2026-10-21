// The AI setup part of the mock API (M1). It mirrors what the facade decides — backend state,
// the disclosure version, the estimate's upper bound and `would_block`, question (b) — so the
// screens can be built and tested before the Rust side exists. Keys are validated and dropped:
// only the last 4 characters are kept, as in the real facade.

import { isLoopbackUrl } from "@/lib/url";
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
  type ModelErrorKind,
  type ModelInfo,
  type ModelProviderRecord,
  materialSharing,
  type ProbeReport,
  type ProviderPreset,
  type UsageRow,
  type UsageSummary,
} from "../ai";
import type { PageLampApi } from "../client";
import { ApiError } from "../errors";
import { aiMaterialsState } from "../types";
import type { MockActivity } from "./activity";
import {
  CODING_PLAN_HOSTS,
  CODING_PLAN_KEY_PREFIXES,
  cloudViaLocalFacts,
  MOCK_MODELS,
  MOCK_PRESETS,
  MOCK_PRICES,
  mockUsage,
} from "./ai-fixtures";
import { createMockCodex } from "./codex";
import { liftedIncludes } from "./explain";
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
  | "modelRunsHere"
  | "modelDisclosure"
  | "acknowledgeModelDisclosure"
  | "acknowledgeUnpricedModel"
  | "setMonthlyBudget"
  | "estimateGeneration"
  | "usageSummary"
  | "removeAllAiData"
  | "codexStatus"
  | "installCodex"
  | "cancelCodexInstall"
  | "removeCodex"
  | "codexLogin"
  | "cancelCodexLogin"
  | "codexLogout"
  | "setCodexSource"
  | "setModeAWeeklyCap"
>;

export interface MockAiContext {
  scenario: MockScenario;
  now: () => Date;
  /** A Codex install is listed while it runs. */
  activity: MockActivity;
  /** Waits the mock's latency plus `extra` ms. */
  delay: (extra?: number) => Promise<void>;
  /** Delay between streamed events (install, sign-in) in ms. */
  stepMs: number;
  courses: () => MockCourse[];
  findCourse: (courseId: string) => MockCourse;
  /** The weekly note has nothing to write about: its estimate blocks, as the facade's does. */
  noteHasNothingToWrite: () => boolean;
  /** The courses a study plan covers; none: its estimate blocks, as the facade's does. */
  planCourses: (wanted: readonly string[]) => MockCourse[];
  /**
   * "Remove all AI data"'s part outside AI setup: the AI calendars not accepted and the
   * reminders' claims go (the proposals mock). Returns how many calendars went.
   */
  forgetAiRecords: () => number;
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
  if (!preset) throw new ApiError("not_found", `Unknown preset "${id}".`);
  return preset;
}

function isLoopback(url: URL): boolean {
  return isLoopbackUrl(url.href);
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
/** `lifted`: the included materials an explanation sends too (`liftedIncludes`). */
function workload(req: EstimateRequest, lifted: number): { input: number; output: number } {
  switch (req.feature) {
    case "study_plan":
      return { input: 6_000 + 1_500 * req.courses.length, output: 8_000 };
    case "weekly_explanation":
      return { input: 45_000 + 15_000 * lifted, output: 6_000 };
    case "weekly_note":
      return { input: 3_000, output: 1_500 };
    case "course_calendar":
      return { input: 15_000 * Math.max(1, req.courses.length), output: 4_000 };
  }
}

/** "Test" failed at the model: a report, not an error (like the facade). */
function failedProbe(error: ModelErrorKind): ProbeReport {
  return {
    ok: false,
    latency_ms: 0,
    structured_output_tier: null,
    thinking_always_on: false,
    error,
  };
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
  /** Providers whose API key the keychain won't let PageLamp delete ("ai-keychain-locked"). */
  const lockedKeys = new Set<string>();
  const features = new Map<AiFeature, ModelChoice | null>(FEATURES.map((f) => [f, null]));
  let budget: number | null = DEFAULT_BUDGET_MICRO_USD;
  const codex = createMockCodex({
    scenario,
    delay: ctx.delay,
    stepMs: ctx.stepMs,
    activity: ctx.activity,
  });
  let usage: [number, UsageRow][] = [...mockUsage(scenario), ...codex.usageRows()];

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
    case "ai-unpriced":
    // Monday's weekly note (beta.2): prepared with the student's own key.
    case "weekly-note-monday":
    // Calendar proposals (F3): an API key, so "Read the syllabus with AI" can run in the demo.
    case "proposals": {
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
    case "ai-keychain-locked":
      // Two keys; the keychain won't let PageLamp delete OpenAI's.
      record("openai", "openai", "https://api.openai.com/v1", openaiKey);
      acknowledged.set("provider:openai", presetOf("openai").data_policy.version);
      record("anthropic", "anthropic", "https://api.anthropic.com", "sk-ant-demo-0000Hq8e");
      acknowledged.set("provider:anthropic", presetOf("anthropic").data_policy.version);
      route("openai", "gpt-6-luna");
      lockedKeys.add("openai");
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
  // Mode A scenarios (M2): signed in and routed to the pin's models, unless signed out.
  const codexStart = codex.backendStatus(null, false);
  if (codexStart && scenario !== "codex-signed-out") {
    if (codex.initiallyAcknowledged) acknowledged.set("codex", codexStart.disclosure.version);
    for (const feature of FEATURES) {
      features.set(feature, {
        backend: { kind: "codex" },
        model: feature === "weekly_explanation" ? "gpt-6-sol" : "gpt-6-luna",
        effort: "lowest",
      });
    }
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
  /** Some feature is routed to the ChatGPT plan. */
  function codexChosen(): boolean {
    return [...features.values()].some((c) => c?.backend.kind === "codex");
  }
  function codexBackend(): AiBackendStatus {
    // Not offered: models, "Test", the Codex choice and its disclosure all refuse.
    codex.requireOffered();
    const found = codex.backendStatus(acknowledged.get("codex") ?? null, codexChosen());
    if (!found) {
      throw new ApiError("model", "Codex isn't installed.", { model_error: "runtime_missing" });
    }
    return found;
  }
  /** The facade's view of any backend: its status and its models. */
  function statusOf(backend: BackendRef): AiBackendStatus {
    return backend.kind === "codex" ? codexBackend() : backendStatus(providerOf(backend));
  }
  function modelsFor(backend: BackendRef): ModelInfo[] {
    if (backend.kind === "codex") {
      codexBackend();
      return codex.models();
    }
    return modelsOf(providerOf(backend));
  }
  function modelsOf(provider: ModelProviderRecord): ModelInfo[] {
    return MOCK_MODELS[provider.preset] ?? [];
  }
  function modelInfo(provider: ModelProviderRecord, model: string): ModelInfo | null {
    return modelsOf(provider).find((m) => m.id === model) ?? null;
  }
  /**
   * The provider's own note (the server's, for one on this computer), by address like the
   * facade's disclosure_for: a server here keeps what it's sent on this computer, for free; a
   * local app's server on another computer is up to whoever runs it.
   */
  function disclosureOf(provider: ModelProviderRecord): DisclosureFacts {
    const preset = presetOf(provider.preset);
    const base = preset.data_policy;
    if (provider.on_device) {
      return base.on_device
        ? base
        : { ...base, version: base.version + 7, cost: "free_on_device", on_device: true };
    }
    if (preset.local) {
      return {
        ...base,
        version: base.version + 9,
        training: { kind: "unknown" },
        retention: { kind: "provider_terms" },
        cost: "self_hosted",
        on_device: false,
      };
    }
    return base;
  }
  /**
   * Whether `model` runs on this computer, as a run decides it (the facade's run_destination):
   * LM Studio runs every model here; Ollama, the models it lists as local; any other server
   * here can't say, so it counts as the cloud; a provider off this computer is the cloud.
   */
  function runsHere(provider: ModelProviderRecord, model: string): boolean {
    if (!provider.on_device) return false;
    if (provider.preset === "lm_studio") return true;
    if (provider.preset === "ollama") return modelInfo(provider, model)?.on_device === true;
    return false;
  }
  /**
   * The note to read before using a model of `backend` where it runs, and the key its
   * acknowledgement is kept under (the facade's disclosure_and_key): a model a server here may
   * run in the cloud has a note of its own.
   */
  function noteOf(backend: BackendRef, here: boolean): { facts: DisclosureFacts; key: string } {
    if (backend.kind === "codex") return { facts: codexBackend().disclosure, key: "codex" };
    const provider = providerOf(backend);
    const base = disclosureOf(provider);
    return provider.on_device && !here
      ? { facts: cloudViaLocalFacts(base), key: `${backendKey(backend)}:cloud` }
      : { facts: base, key: backendKey(backend) };
  }
  /** Like the facade: a custom server on this computer may be added without a key. */
  function needsKey(preset: ProviderPreset, baseUrl: string): boolean {
    if (!preset.needs_key) return false;
    try {
      return !(preset.id === "custom" && isLoopback(new URL(baseUrl)));
    } catch {
      return true;
    }
  }
  /**
   * Like the facade's ai_status: the server's own note, and for a server here that may run a
   * model in the cloud (any but LM Studio) the cloud note too. Ready once each routed model's
   * note is read (placed as an estimate places it), or either note with none routed there.
   */
  function backendStatus(provider: ModelProviderRecord): AiBackendStatus {
    const backend: BackendRef = { kind: "provider", provider_id: provider.provider_id };
    const disclosure = disclosureOf(provider);
    const acked = acknowledged.get(backendKey(backend)) ?? null;
    const preset = presetOf(provider.preset);
    const hereNote = !provider.on_device || ["ollama", "lm_studio"].includes(provider.preset);
    const cloud =
      provider.on_device && provider.preset !== "lm_studio" ? cloudViaLocalFacts(disclosure) : null;
    const cloudAcked = cloud ? (acknowledged.get(`${backendKey(backend)}:cloud`) ?? null) : null;
    const hereOk = hereNote && acked === disclosure.version;
    const cloudOk = !!cloud && cloudAcked === cloud.version;
    let needsHere = false;
    let needsCloud = false;
    for (const choice of features.values()) {
      if (choice?.backend.kind !== "provider") continue;
      if (choice.backend.provider_id !== provider.provider_id) continue;
      // A routed model needs the cloud note only from a server that has one.
      if (!cloud || runsHere(provider, choice.model)) needsHere = true;
      else needsCloud = true;
    }
    const notesOk =
      needsHere || needsCloud
        ? (!needsHere || hereOk) && (!needsCloud || cloudOk)
        : hereOk || cloudOk;
    const problems: AiBackendStatus["problems"] = [];
    if (needsKey(preset, provider.base_url) && !provider.key_last4) problems.push("key_missing");
    if (
      (acked !== null && acked !== disclosure.version) ||
      (cloud && cloudAcked !== null && cloudAcked !== cloud.version)
    ) {
      problems.push("disclosure_changed");
    }
    const state = problems.includes("key_missing")
      ? "needs_setup"
      : notesOk
        ? "ready"
        : "needs_disclosure";
    return {
      backend,
      label: provider.label,
      kind: provider.on_device ? "local" : "api_key",
      state,
      problems,
      disclosure,
      disclosure_acknowledged: acked,
      cloud_disclosure: cloud,
      cloud_disclosure_acknowledged: cloudAcked,
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
      .filter(([ago, r]) => ago === 0 && r.cost_basis === "priced")
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
      // The facade's message is "<Vendor>: “<their sentence>”"; this sentence is made up.
      throw new ApiError(
        "blocked",
        "Demo Vendor: “This plan's keys may only be used in the vendor's own coding tools.”",
        { blocked: "coding_plan_key" },
      );
    }
    if (needsKey(preset, baseUrl) && !key?.trim()) {
      throw new ApiError("invalid", "Enter the API key.");
    }
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
        // Off except in the codex-* scenarios, as in every build until OpenAI confirms in
        // writing (CHATGPT_PLAN_OFFERED in the facade).
        chatgpt_plan_offered: codex.offered,
        // Priority order (design §7): the ChatGPT plan first, then keys and local models.
        backends: [
          codex.backendStatus(acknowledged.get("codex") ?? null, codexChosen()),
          ...providers.map(backendStatus),
        ].filter((b): b is AiBackendStatus => b !== null),
        providers,
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
      const key = preset.needs_key && apiKey?.trim() ? apiKey : null;
      validateKey(preset, url, preset.needs_key ? apiKey : null);
      // Like the facade: one provider per preset (per address for custom endpoints).
      const id = preset.id === "custom" ? `custom-${shortHash(url)}` : preset.id;
      if (providers.some((p) => p.provider_id === id)) {
        throw new ApiError("invalid", `${preset.label} is already set up.`);
      }
      const rec = record(preset.id, id, url, key);
      rec.created_at = now().toISOString();
      return structuredClone(rec);
    },

    updateModelProviderKey: async (providerId, apiKey) => {
      await ctx.delay(600);
      const provider = findProvider(providerId);
      const preset = presetOf(provider.preset);
      // Like the facade: a server that takes no key gets none (a custom server here may get one
      // later); a key may be left out only when adding a custom server here.
      if (!preset.needs_key) throw new ApiError("invalid", `${preset.label} doesn't use a key.`);
      if (!apiKey.trim()) throw new ApiError("invalid", "Enter the API key.");
      validateKey(presetOf(provider.preset), provider.base_url, apiKey);
      provider.key_last4 = last4(apiKey);
      return structuredClone(provider);
    },

    removeModelProvider: async (providerId) => {
      await ctx.delay();
      const provider = findProvider(providerId);
      // Like the facade: the key goes first; a key the keychain keeps changes nothing.
      if (provider.key_last4 && lockedKeys.has(providerId)) {
        throw new ApiError(
          "internal",
          `The API key of ${provider.label} couldn't be deleted from the keychain (the keychain is locked): it is still there, under \`llm:${providerId}\`. Nothing was removed; try again when the keychain can be reached.`,
        );
      }
      providers.splice(
        providers.findIndex((p) => p.provider_id === providerId),
        1,
      );
      acknowledged.delete(`provider:${providerId}`);
      acknowledged.delete(`provider:${providerId}:cloud`);
      for (const [feature, choice] of features) {
        if (choice?.backend.kind === "provider" && choice.backend.provider_id === providerId) {
          features.set(feature, null);
        }
      }
    },

    detectLocalServers: async (): Promise<LocalServer[]> => {
      await ctx.delay(300);
      // Like the facade: the provider already added with the same preset and address.
      const normal = (url: string) => url.replace(/\/+$/, "").replace("//localhost", "//127.0.0.1");
      const server = (
        preset: LocalServer["kind"],
        base_url: string,
        running: boolean,
      ): LocalServer => ({
        kind: preset,
        preset,
        base_url,
        running,
        provider_id:
          providers.find((p) => p.preset === preset && normal(p.base_url) === normal(base_url))
            ?.provider_id ?? null,
      });
      return [
        server("ollama", "http://127.0.0.1:11434", true),
        server("lm_studio", "http://127.0.0.1:1234/v1", false),
      ];
    },

    listModels: async (backend) => {
      await ctx.delay(400);
      if (backend.kind === "codex") return modelsFor(backend);
      const provider = providerOf(backend);
      if (scenario === "ai-errors") {
        throw new ApiError("model", "Couldn't reach llm.demo.test.", { model_error: "network" });
      }
      // Like the facade: only Ollama and LM Studio say a model runs here; any other server here
      // may run its models anywhere.
      const says = provider.preset === "ollama" || provider.preset === "lm_studio";
      return structuredClone(modelsOf(provider)).map((m) => {
        if (provider.on_device && !says) {
          return { ...m, on_device: false, runs_in_cloud: true, price_known: false };
        }
        // A local app's server on another computer: nothing runs here, priced by the catalog.
        if (!provider.on_device && presetOf(provider.preset).local) {
          return { ...m, on_device: false, runs_in_cloud: false, price_known: m.id in MOCK_PRICES };
        }
        return m;
      });
    },

    testModel: async (backend, model): Promise<ProbeReport> => {
      await ctx.delay(900);
      if (backend.kind === "codex") {
        const models = modelsFor(backend);
        const status = await codex.api.codexStatus();
        if (status.login.state === "signed_out") return failedProbe("not_signed_in");
        if (status.outdated_action !== "none") return failedProbe("runtime_outdated");
        if (!models.some((m) => m.id === model)) return failedProbe("model_not_found");
        return {
          ok: true,
          latency_ms: 3_100,
          structured_output_tier: "native_schema",
          thinking_always_on: false,
          error: null,
        };
      }
      // Like the facade: an unknown provider is NotFound; model errors are a failed probe.
      const provider = providerOf(backend);
      if (scenario === "ai-errors") return failedProbe("rate_limited");
      const info = modelInfo(provider, model);
      if (!info) return failedProbe("model_not_found");
      return {
        ok: true,
        latency_ms: info.on_device ? 2_300 : 900,
        structured_output_tier: provider.wire === "openai_chat" ? "json_object" : "native_schema",
        thinking_always_on: info.reasoning_always_on,
        error: null,
      };
    },

    setFeatureModel: async (feature, choice) => {
      await ctx.delay();
      if (choice) {
        if (!modelsFor(choice.backend).some((m) => m.id === choice.model)) {
          throw new ApiError("invalid", `No model "${choice.model}".`);
        }
      }
      features.set(feature, choice ? structuredClone(choice) : null);
    },

    acknowledgeAiDisclosure: async (backend, version) => {
      await ctx.delay();
      const current = statusOf(backend).disclosure.version;
      if (version !== current) {
        throw new ApiError("invalid", "The disclosure changed; read it again.");
      }
      acknowledged.set(backendKey(backend), version);
    },

    modelRunsHere: async (choice, ask) => {
      await ctx.delay();
      if (choice.backend.kind === "codex") return false;
      const provider = providerOf(choice.backend);
      // Like the facade: not asked, an Ollama model is unknown (only LM Studio says it runs here).
      if (!ask && provider.preset === "ollama") return false;
      return runsHere(provider, choice.model);
    },

    modelDisclosure: async (backend, here) => {
      await ctx.delay();
      const { facts, key } = noteOf(backend, here);
      return [facts, acknowledged.get(key) === facts.version];
    },

    acknowledgeModelDisclosure: async (backend, here, version) => {
      await ctx.delay();
      const { facts, key } = noteOf(backend, here);
      if (version !== facts.version) {
        throw new ApiError(
          "invalid",
          "What this service does with your data changed; please read it again.",
        );
      }
      acknowledged.set(key, version);
    },

    acknowledgeUnpricedModel: async (backend, model) => {
      await ctx.delay();
      // Like the facade: the plan has no price to acknowledge.
      if (backend.kind === "codex") {
        throw new ApiError("invalid", "The ChatGPT plan has no per-run price.");
      }
      statusOf(backend);
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
      // Like the facade (pinned in its tests/ai_api.rs): no model and the gate's blocks
      // (question (b) included) carry no amount and 0 tokens; the other blocks leave the
      // estimate complete, since only an acknowledgement or a cap stops the run.
      const gateBlocked = (reason: BlockReason): CostEstimate => ({
        micro_usd_upper: null,
        input_tokens: 0,
        max_output_tokens: 0,
        reasoning_allowance: 0,
        repair_possible: false,
        price_known: false,
        would_block: reason,
      });
      if (!choice) return gateBlocked("no_model_chosen");
      // The note's and the plan's context come next in the facade, before their other blocks.
      if (req.feature === "weekly_note" && ctx.noteHasNothingToWrite()) {
        return gateBlocked("nothing_to_write");
      }
      if (req.feature === "study_plan" && ctx.planCourses(req.courses).length === 0) {
        return gateBlocked("no_course_to_plan");
      }
      // A Codex routing stored while the plan was offered blocks instead of running.
      if (choice.backend.kind === "codex" && !codex.offered) {
        return gateBlocked("backend_disabled_in_this_build");
      }
      const info = modelsFor(choice.backend).find((m) => m.id === choice.model) ?? null;
      // Where the run would go, as the run decides it (a server here may run a model elsewhere).
      const onDevice =
        choice.backend.kind === "provider" && runsHere(providerOf(choice.backend), choice.model);
      const courses =
        req.feature === "weekly_explanation"
          ? [req.course]
          : req.feature === "course_calendar"
            ? req.courses
            : [];
      for (const course of courses) {
        const gate = courseGate(course, onDevice);
        if (gate) return gateBlocked(gate);
      }
      // An explanation's included materials count only where its run would send them.
      const lifted =
        req.feature === "weekly_explanation"
          ? liftedIncludes(
              ctx.findCourse(req.course),
              req.week ?? null,
              req.include ?? [],
              ctx.now(),
            ).length
          : 0;
      const { input, output } = workload(req, lifted);

      const status = statusOf(choice.backend);
      const reasoning = Math.max(REASONING[choice.effort], info?.reasoning_always_on ? 8_000 : 0);
      const record =
        choice.backend.kind === "provider" ? findProvider(choice.backend.provider_id) : null;
      const repair = record?.wire === "openai_chat";
      const price = MOCK_PRICES[choice.model];
      let upper: number | null = null;
      if (onDevice) upper = 0;
      else if (price && status.kind === "api_key") {
        upper = Math.ceil(
          ((input * price[0] + (output + reasoning) * price[1]) / 1_000_000) * (repair ? 2 : 1),
        );
      }
      const priceKnown = upper !== null;

      let block: BlockReason | null = null;
      // The note of where it runs: a model a server here may run in the cloud has its own.
      const note = status.kind === "codex" ? null : noteOf(choice.backend, onDevice);
      // This model's note only (the status asks for every routed model's), by its version alone,
      // like the facade: a sign-in or a key missing is the run's to say, not the note's.
      const read = note
        ? acknowledged.get(note.key) === note.facts.version
        : acknowledged.get("codex") === statusOf(choice.backend).disclosure.version;
      if (!read) block = "disclosure_not_acknowledged";
      // Mode A has no money budget; PageLamp's runs per week are capped instead.
      if (!block && status.kind === "codex" && codex.capReached()) {
        block = "weekly_run_cap_reached";
      }
      // An unpriced cloud model, also one a server here runs in the cloud.
      if (!block && status.kind !== "codex" && !onDevice && !priceKnown) {
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
        // The priced rows only: what the budget counts.
        total_micro_usd: rows
          .filter((r) => r.cost_basis === "priced")
          .reduce((sum, r) => sum + (r.micro_usd ?? 0), 0),
        budget: budgetStatus(),
        mode_a: codex.modeA(codexChosen()),
      });
    },

    removeAllAiData: async () => {
      await ctx.delay(300);
      // Like the facade: Codex's sign-in ends first; then the keys, and a provider whose key
      // the keychain keeps stays (with its key); everything else goes, then it says so.
      codex.forget();
      const kept = providers.filter((p) => p.key_last4 && lockedKeys.has(p.provider_id));
      const generations = ctx.forgetAiRecords();
      acknowledged.clear();
      unpricedAcks.clear();
      for (const feature of FEATURES) features.set(feature, null);
      budget = DEFAULT_BUDGET_MICRO_USD;
      const report = {
        providers_removed: providers.length - kept.length,
        generations_removed: generations,
        usage_rows_removed: usage.length,
        backup_removed: false,
      };
      providers.splice(0, providers.length, ...kept);
      usage = [];
      if (kept.length > 0) {
        const ids = kept.map((p) => p.provider_id);
        throw new ApiError(
          "internal",
          `Removed ${report.providers_removed} provider(s), ${report.usage_rows_removed} usage record(s) and ${report.generations_removed} generated text(s), but the keychain didn't let PageLamp delete the API key of ${ids.join(", ")}: it is still there (${ids.map((id) => `\`llm:${id}\``).join(", ")}), and so is that provider. Run this again when the keychain can be reached.`,
        );
      }
      return report;
    },

    ...codex.api,
    removeCodex: async () => {
      await codex.api.removeCodex();
      acknowledged.delete("codex");
      for (const [feature, choice] of features) {
        if (choice?.backend.kind === "codex") features.set(feature, null);
      }
    },
  };
}
