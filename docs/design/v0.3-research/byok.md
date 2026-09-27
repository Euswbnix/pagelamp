# Research: BYOK API providers and local models for PageLamp v0.3

Date: 2026-09-27 · Scope: v0.3 access mode C (BYOK API key) and mode D (local model) from
`docs/design/v0.3-model-access.md`. Subscription modes A/B (Codex, Claude Code) are covered elsewhere.
They only matter here because the provider abstraction must be able to host them too.

How each claim is marked: **[V]** = read in a primary source (vendor docs, terms, repo, registry) on
2026-09-27. **[S]** = secondary source only (blog, forum, search summary). **[U]** = not verified,
or an inference of mine. The numbers in brackets point to the source list in §11. Every source was
accessed on 2026-09-27. Prices and model IDs change every month, so check them again before you
hard-code anything.

---

## 0. Summary and recommendation

1. **Write our own thin layer, `crates/pagelamp-llm`** (reqwest 0.13 + serde_json + a small SSE parser +
   tokio-util `CancellationToken`). This is the draft's decision, and it still holds. The Rust options
   have not changed: OpenAI and Anthropic still have no official Rust SDK [20][69]. The crates that do
   exist are maintained by one person each (genai, async-openai) or warn about breaking changes (rig).
   Our layer needs about 4 wire drivers. It depends on nothing new except possibly `sse-stream` or
   `eventsource-stream`, and a hand-written SSE parser is about 150 lines.
2. **Ship first (v0.3.0):** OpenAI **Responses**, Anthropic **Messages** (native), **Ollama native**
   (`/api/chat`), and a **generic OpenAI-compatible Chat Completions** driver. That last one also
   serves OpenRouter, LM Studio, llama.cpp `llama-server` and Gemini's OpenAI-compatible endpoint
   as data-only presets. Add Gemini native, the Tier-2 vendors and an "Open Responses" dialect later,
   and only when there is a concrete need.
3. **The abstraction works at the feature level.** It is a closed `enum Backend` (no `dyn`, no plugins)
   behind one internal trait-like surface. Its IR is built for single-shot generation with
   structured output, and it keeps a slot for opaque reasoning state so follow-up chat and the tutor
   can be added later. Subscription CLIs (Codex `exec`, `claude -p`) become further `Backend`
   variants of the same surface. §6 has the full shape.
4. **The policy gate is enforced by the type system.** The llm crate accepts course text only as a
   `GatedContext`, which only the core policy module can build from `AiMaterialsState`. That keeps
   the engine in front of every model call, local ones included. Whether "prohibited" also blocks
   local models is an owner question (§10).
5. **Some decisions in the 2026-09-25 draft need changing** (§9). Examples: Ollama's 4k default
   context makes the native Ollama driver mandatory. The draft's frontier cost estimate is now
   4–9× too low. MiniMax plan keys cannot be detected by base URL. UniFFI 0.32 cannot cancel Rust
   futures from Swift, so the facade needs an explicit `cancel_generation(id)`.

---

## 1. Repo constraints that shape this

- **Facade contract.** Everything goes through `pagelamp-app` with plain serialisable types, so that Tauri
  and Swift both get it. Tauri streams through `tauri::ipc::Channel`. Swift streams through a foreign
  callback trait: `#[uniffi::export(foreign)] trait SyncObserver` exists on `feat/macos-shell`
  in `crates/pagelamp-ffi/src/lib.rs`, pinned to `uniffi = "=0.32.2"`.
- **Secrets.** They live in `pagelamp-core::secrets`: keychain service `dev.pagelamp`, one account per
  id, and the env override `PAGELAMP_SECRET_<ID>`. `SecretBackend`/`MemorySecrets` exist for tests.
  The lockfile has `keyring 4.2.0` and `keyring-core 1.0.0`.
- **HTTP.** `reqwest 0.13.5` (rustls) is already used by canvas and local. The canvas transport already
  has a retry/backoff/throttle pattern (`RetryPolicy`, `is_throttled`, `classify_failure`) that we
  can copy.
- **Licences.** `deny.toml` allows Apache/MIT/BSD/ISC/Zlib/MPL-2.0 and forbids copyleft and
  source-available licences.
- **`pagelamp mcp` stays network-free and lightweight** (ARCHITECTURE §3 rule 1, §4). Model calls
  therefore live in the app and CLI only, never in the MCP server.
- **PRIVACY.md currently says "PageLamp never contacts any other server".** v0.3 must rewrite this,
  and any runtime catalog fetch (models.dev) would be yet another new destination.

---

## 2. What changed since 2025

| Area | Change | Source |
|---|---|---|
| OpenAI | The Assistants API was **shut down on 2026-08-26**. Responses is its replacement. | [12] [S], [1] [V] |
| OpenAI | Chat Completions is still "supported", but "**Starting with GPT-5.4, Chat Completions does not support tool calling with `reasoning_effort` values other than `none`**". Reasoning items and encrypted reasoning exist only in Responses. | [1] [V] |
| OpenAI | New families: GPT-6 (astra/sol/luna, 1.05M context) and GPT-5.6. The **`gpt-5`/`gpt-5-mini`/`gpt-5-nano` 2025-08-07 snapshots shut down on 2026-12-11**. | [2][10][11] [V] |
| OpenAI | Responses accepts non-PDF files (docx, pptx, xlsx, csv…; text only). Chat Completions accepts PDFs only. | [3] [V] |
| OpenAI | **Enforced spend limits** have 429 codes: `organization_spend_limit_exceeded`, `project_spend_limit_exceeded`, `credit_balance_exhausted`. | [9] [V] |
| Multi-vendor | The **Open Responses** spec is an open, Responses-based interop standard. Listed implementers include OpenAI, OpenRouter, Ollama, LM Studio, vLLM, Hugging Face, AWS and NVIDIA, and it has acceptance tests. | [34] [V]; launch date [S] early 2026 |
| Anthropic | Current lineup: Fable 5.1 ($10/$50), **Opus 5.5 ($4/$20)**, **Sonnet 5 ($2/$10)**, **Haiku 4.5 ($1/$5)**. The newer models have 1M context. | [14] [V] |
| Anthropic | **Structured outputs are GA** as `output_config.format` (the old `output_format` is deprecated) and `strict: true` tools, with no beta header. | [15] [V] |
| Anthropic | **Thinking is always on** for Opus 5.5 and Fable (`disabled` returns 400). `thinking.type:"enabled"` is rejected from 4.7 on (use adaptive + `output_config.effort`). **Prefill is rejected** from 4.6 on. **Forced `tool_choice` is rejected** on Opus 5.5. **Non-default `temperature`/`top_p`/`top_k` return 400** from 4.7 on. | [20][21] [V] |
| Anthropic | Replayed thinking blocks are bound to an unchanged prefix. **Accounts created on or after 2026-08-31 get a 400** on a mismatch unless `prefix_mismatch_behavior:"drop_block"` is set (beta header). | [20] [V] |
| Anthropic | Top-level automatic `cache_control`. The minimum cacheable prompt is 512 tokens on the newest models. Cache reads cost 0.05× on Opus 5.5. | [16] [V] |
| Anthropic | Tiers are now Start/Build/Scale with monthly caps ($500/$1k/$200k). A cap 429 comes **without `retry-after`**, with `error_code: enforced_spend_limit_reached`. | [19] [V] |
| Anthropic | **Haiku 4.5 retirement "not sooner than 2026-10-15"**. No Haiku 5 is listed. | [21] [V] |
| Google | The **Interactions API went GA in June 2026** and is now the recommended native API. `generateContent` is "legacy" but supported. Interactions **stores data by default** (paid 55 days, free 1 day). | [28] [V] |
| Google | New Gemini 3.x Flash and Flash-Lite models. Thinking **cannot be disabled** on the 3.x Flash models. **Mandatory prepaid billing for new AI Studio users since 2026-03-23** and hard per-tier spend caps. | [24][26][29] [V] |
| Local | **Gemma 4** (Apache-2.0; E2B/E4B/12B/26B-A4B/31B), **Qwen3.5 small** (Apache-2.0; 0.8–9B, released 2026-03), **gpt-oss-20b** (Apache-2.0, fits in 16 GB). | [53][54][55] [V], [56] [S] |
| Local | **Ollama "cloud models" are served through `localhost:11434`** (the `-cloud` suffix), so "local" is no longer guaranteed. They can be detected with `remote_host` in `/api/tags` and disabled with `OLLAMA_NO_CLOUD=1`. | [44][45][46] [V], [49] [S] |
| Local | Ollama and LM Studio both expose `/v1/responses`. LM Studio also exposes an Anthropic-compatible Messages endpoint. LM Studio has been **free for work use since 2025-07-08** (still proprietary). | [42][50][51] [V] |
| Coding plans | Z.ai's terms explicitly forbid "directly invoking model APIs from your own applications". Alibaba intl forbids "application backends". Kimi Code treats User-Agent tampering as a violation. | [38][39][40] [V] |
| UniFFI | Cancelling a Swift `Task` still **does not cancel the Rust future**. PR #3007 was still open on 2026-09-24. | [63][64] [V] |

---

## 3. Findings per provider

### 3.1 OpenAI

- **Responses vs Chat Completions.** OpenAI's guide says "While Chat Completions remains supported,
  Responses is recommended for all new projects" [1][V]. Responses-only features that matter to us:
  reasoning items with encrypted reasoning for stateless or ZDR use, `text.format` structured
  outputs, and non-PDF file inputs. On Chat Completions, tool calling from GPT-5.4 on requires
  `reasoning_effort: none` [1][V]. One secondary source claims GPT-6 Astra cannot call tools on Chat
  Completions at all [S, benchlm]. I could not confirm that in the docs, so treat it as [U].
  **Conclusion:** our OpenAI driver targets Responses. We keep a Chat Completions driver for everyone
  else.
- **Storage.** "Response objects are saved for 30 days by default." Disable this with `store: false` [7][V].
  For stateless multi-turn, replay the returned reasoning items; each carries `encrypted_content` by
  default [1][V]. API data is not used for training unless you opt in, and abuse-monitoring logs are
  kept for 30 days [6][V]. `previous_response_id` still bills all prior input [7][V], so it saves no
  money. **Always send `store: false`.**
- **Structured outputs.** Use `text: {format: {type: "json_schema", strict: true, schema}}` on Responses and
  `response_format` on Chat Completions. `additionalProperties: false` is required, and refusals come
  back as a separate `refusal` field [4][V]. Schema limits: 100 object properties, 5 nesting levels,
  15,000 characters of names/enums, 500 enum values [S; the official page did not render these
  numbers].
- **File inputs.** 50 MB per file and 50 MB per request. For PDFs, vision models get "both text and page
  images" (more tokens). File URLs are Responses-only. Chat Completions takes PDFs via file_id or
  base64 [3][V].
- **Streaming.** Event types include `response.created`, `response.output_text.delta`,
  `response.completed` and `error` [13][V].
- **Background and cancel.** `POST /v1/responses/{id}/cancel` is idempotent. Background mode keeps data
  on disk for about 10 minutes even under ZDR [8][V]. We don't need background mode.
- **Rate limits and errors.** Headers: `x-ratelimit-{limit,remaining,reset}-{requests,tokens}`,
  `*-project-tokens` and `Retry-After` [5][V]. 429 `slow_down`, quota and spend codes as listed in §2,
  503 "model overloaded", 403 unsupported region [9][V]. **Branch on `error.code`: quota and spend
  429s must not be retried.**
- **Prices per 1M tokens, standard tier** [2][V]: gpt-6-luna $0.10/$0.50 (the cheapest featured model
  [11]), gpt-5.6-luna $0.20/$1.20, gpt-5.4-nano $0.20/$1.25, gpt-5.4-mini $0.75/$4.50, gpt-6-sol
  $2/$10, gpt-6-astra $10/$50. Batch/Flex are about 50% off.
- **Hard spend cap for students.** Use a project spend limit (enforced, 429) or prepaid credits with
  auto-recharge off [9][V].
- **Age.** 13+, with parent or guardian permission under 18 [68][S].
- **BYOK in third-party apps.** OpenAI's terms don't mention it. It is widely practised, but I found
  no official statement [S].

### 3.2 Anthropic

- **Native Messages is required.** The OpenAI-compatible layer "is not considered a long-term or
  production-ready solution". It ignores `response_format` and `strict`, ignores `file` parts, doesn't
  support caching, and accepts `thinking` but doesn't return the thought process [18][V].
- **Models and constraints** are in §2 [14][20][21][V]. They have direct consequences for our driver:
  - Never send `temperature`/`top_p`/`top_k` to 4.7+ models.
  - Never prefill.
  - Never use the "forced tool as JSON" trick. Use `output_config.format` instead.
  - Opus 5.5 cannot turn thinking off. Use effort `low` for cheap features.
  - Haiku 4.5 still uses extended thinking (`budget_tokens`), or none.
  - The Models API returns `capabilities`, `max_input_tokens` and `max_tokens`, which the probe can use [14][V].
- **Structured outputs** (GA) [15][V]. Supported: `enum`, `const`, `anyOf`, `allOf`, `$ref`/`$defs`,
  `minItems` 0/1 only, and the formats date, date-time, uri and so on. Not supported: recursion,
  `minimum`/`maximum`, `minLength`/`maxLength`. Changing the format invalidates the prompt cache.
- **Prompt caching** [16][V]. Up to 4 breakpoints, or automatic via a top-level `cache_control`.
  5-minute writes cost 1.25× and 1-hour writes 2×. Reads cost 0.1×, 0.05× on Opus 5.5. Minimum size
  is 512–4,096 tokens depending on the model (4,096 for Haiku 4.5). Usage fields:
  `cache_creation_input_tokens`, `cache_read_input_tokens`, `input_tokens` (after the last
  breakpoint). Cached reads don't count toward ITPM on most models [19][V].
- **PDFs** [17][V]. 32 MB per request and 600 pages (100 below 1M context). Each page is text plus image:
  "1,500–3,000 tokens per page" of text, plus image tokens. Files API `file_id` is supported.
- **Errors** [20][V]. 400/401/402 `billing_error`/403/404/409/413/429/500/504/529 `overloaded_error`. The
  SDKs retry twice and honour `retry-after`. Errors can arrive mid-stream after a 200. The
  `request-id` header should go into our diagnostic report.
- **Data** [22][V]. Retained data is "never used for model training without your express permission".
  Covered Models (Fable/Mythos) require 30-day retention. I did not re-fetch the standard commercial
  retention period [U].
- **Age for API/Commercial Terms.** Search results mix up the consumer terms (18+) and the Commercial
  Terms. Not verified [U].

### 3.3 Google Gemini: worth it?

- **Endpoint.** The OpenAI-compatible endpoint `…/v1beta/openai/` is "still in beta". It supports
  `response_format` JSON schema and maps `reasoning_effort` onto thinking. Gemini-specific features go
  through `extra_body` [23][V].
- **Native API.** The native API is now the **Interactions API** (GA June 2026). It **stores by default**
  (paid 55 days, free 1 day). `store=false` exists but disables `previous_interaction_id` [28][V].
- **Thinking.** It can't be disabled on the 3.x Flash models. Thought signatures must be replayed exactly
  in stateless multi-turn use [26][V].
- **Prices per 1M tokens** [24][V]: gemini-3.1-flash-lite $0.25/$1.50, gemini-3.5-flash-lite $0.30/$2.50,
  gemini-3.8-flash $0.75/$3.75 (a promotional price "through Dec 31, 2026", which the page says
  doubles in 2027), gemini-2.5-flash-lite $0.10/$0.40. **Most models have a free tier.**
- **Terms** [25][V]:
  - "You must be **18** years of age or older to use the APIs."
  - For Unpaid Services, "human reviewers may read, annotate, and process your API input and output",
    and "Do not submit sensitive, confidential, or personal information".
  - Paid Services don't use prompts to improve products.
- **Billing** [29][V]. Prepaid for new users since 2026-03-23. Hard tier caps (Tier 1 $250). Project spend
  caps enforce with about 10 minutes of latency. Running out of credit returns 402.
- **Verdict:** worth it **as a preset on the generic Chat Completions driver** (almost zero extra code). It
  is also the only real zero-cost cloud option for students. The catch: the free tier means Google
  may train on course materials and have humans read them, and it is 18+. Show that prominently.
  This is an owner decision (§10). Don't build a native Interactions driver for v0.3.

### 3.4 Generic OpenAI-compatible (OpenRouter, DeepSeek, others) and Open Responses

- **OpenRouter** [30][31][32][V]:
  - Endpoint and usage: `https://openrouter.ai/api/v1`, Chat Completions schema, `response_format`
    json_schema. `usage.cost` is returned directly, which is handy for cost display.
  - Attribution headers: `HTTP-Referer` and `X-OpenRouter-Title`/`X-Title`.
  - Errors: 402 for insufficient credits, 429 for rate limits.
  - Privacy: prompts are not stored by default, but metadata is. Routing controls include
    `provider.data_collection: "deny"`, `zdr: true` and `require_parameters: true` (only providers
    that support every parameter, e.g. `response_format`).
  - OpenRouter also has a stateless `/api/v1/responses` beta that rejects `store:true` [33][S].
  - Recommended preset defaults: `require_parameters: true`, and `data_collection: "deny"`, pending the
    owner decision in §10.
- **DeepSeek.**
  - Only `json_object`, no json_schema. "May occasionally return empty content" [35][V].
  - The data is stored in the PRC [36][S].
  - That makes it a Tier-2 preset at most, with its data location shown.
- **Open Responses** [34][V]. A multi-vendor spec based on Responses. Ollama and LM Studio already serve
  `/v1/responses` [42][50][V]. **Watch it, but don't depend on it yet.** Chat Completions is still the
  widest common denominator (llama-server documents only `/v1/chat/completions` [52][V], and DeepSeek
  documents Chat Completions). Revisit after v0.3.0. If most compatible endpoints pass the Open
  Responses acceptance tests, one Responses dialect could replace the Chat Completions dialect for
  Tier 2.

### 3.5 "Coding Plan" keys: terms and detection

| Vendor | Terms (quoted) | Endpoint / key signal | Status |
|---|---|---|---|
| Z.ai GLM Coding Plan | Use only in "officially supported tools". "You shall not use the GLM Coding Plan quota for general-purpose API access … including … directly invoking model APIs from your own applications". No resale, proxying or sharing. Penalties escalate to a ban [37][38]. | `api.z.ai/api/coding/paas/v4` [S] | [V] terms |
| Alibaba Model Studio (intl) Coding Plan | "Do not use the plan's API key for automated scripts, application backends, or other non-interactive scenarios." Violations → suspension or key revocation [39]. | `coding-intl.dashscope.aliyuncs.com/v1` and `/apps/anthropic`. Keys `sk-sp-…` [39] | [V] |
| Kimi Code (Moonshot) | For "Kimi Code CLI / supported coding agents". "Tampering with the client identifier (User-Agent) is considered a violation" [40]. | `api.kimi.ai/coding/v1` (overseas), `api.kimi.com/coding/v1` (China) [40] | [V] |
| MiniMax Token Plan (ex-Coding Plan) | I found no restriction text. Third-party pages say sk-cp keys work in "any OpenAI-compatible tool" [41][S]. | Keys `sk-cp-…`, same host `api.minimax.io` [S] | [U] |
| Volcengine / BytePlus coding plans | Per the 2026-09-25 draft. Not re-verified. | per draft | [U] |

**Rule:** a data-driven blocklist in the provider profiles. Block by **base URL host+path prefix**
(Z.ai, Alibaba, Kimi, BytePlus/Volcengine coding paths) **and by key prefix** (`sk-sp-`, and warn on
`sk-cp-` until the MiniMax terms are checked). Show the vendor's own sentence as the reason. Never try
to work around User-Agent checks: we send an honest `User-Agent: PageLamp/<ver>`.

### 3.6 Local runtimes and models

- **Ollama** (MIT, very active [70]).
  - Endpoints: OpenAI-compatible `/v1/chat/completions`, `/v1/responses`, `/v1/models`. Limitations:
    no `tool_choice`, no image URLs, no stateful Responses [42][V].
  - The native `/api/chat` has `format` (JSON schema), `think` (bool or level), `options.num_ctx`,
    `options.num_predict` and `keep_alive`. Usage comes back as `prompt_eval_count`/`eval_count` plus
    durations [48][V].
  - **Default context is 4k when VRAM is under 24 GiB** [47][V]. The OpenAI-compatible page doesn't
    document a way to raise it [42]. **Course-material prompts would be silently cut off, so use the
    native API with `num_ctx`.**
  - Structured outputs: "Ollama's Cloud currently does not support structured outputs". The docs
    recommend temperature 0 and including the schema in the prompt [43][V].
  - Cloud models go through the local daemon [44][45][V]. `/api/tags` has `remote_model`/`remote_host`
    [46][V]. **PageLamp must treat a model as on-device only if it is on a loopback URL AND has no
    `remote_host` AND no `-cloud` suffix.**
- **LM Studio** (the app is proprietary but free for work; the `lms` CLI is MIT [51][70]).
  - Endpoints: `/v1/models`, `/v1/responses`, `/v1/chat/completions`, `/v1/embeddings`,
    `/v1/completions` on port 1234, plus an Anthropic-compatible Messages endpoint. Structured output
    and tool use are listed [50][V].
  - Use it through the generic Chat Completions driver, as a preset.
- **llama.cpp `llama-server`** (MIT).
  - Endpoints and features: `/v1/chat/completions`, `response_format` with schema, tool calling with
    `--jinja`, `reasoning_content`, `--api-key` [52][V].
  - A generic preset covers it. This is for power users, so it isn't advertised.
- **Models for a student laptop** (Q4 sizes from the Ollama library):
  - Qwen3.5 4b is 3.4 GB and 9b is 6.6 GB, with 256K context and vision [54][V]. Apache-2.0 [S].
  - Gemma 4 comes in E2B/E4B (128K context), 12B (256K), 26B-A4B MoE and 31B, Apache-2.0 [53][V].
  - gpt-oss-20b "runs within 16GB" and is Apache-2.0 [56][S].
  - Suggested defaults [U, my judgement]:
    - 8 GB RAM: Gemma 4 E4B or Qwen3.5 4b.
    - 16 GB RAM: Qwen3.5 9b or Gemma 4 12B.
    - 16 GB+ Apple Silicon or a GPU: gpt-oss-20b.
    - 32 GB+: Gemma 4 26B-A4B.
- **Quality for our features.** The Qwen3.5 small models reach Intelligence Index 27 (4B) and 32 (9B).
  They are verbose with reasoning (230M+ output tokens on the index) and hallucinate 80–82% on the
  AA-Omniscience knowledge test [55][V]. That means:
  - Local explanations must be **grounded in the retrieved course chunks with citations**, with
    thinking turned off (for speed and to limit verbosity).
  - Local models are fine for summaries and for breaking a study plan into tasks, where the dates come
    from the deterministic scheduler.
  - Weekly explanations work but are slow (a CPU-only 8B model runs at about 8–15 tok/s [S]).
  - Label local output as lower quality.
  - Don't bundle a runtime or model. Detect a running Ollama (11434) or LM Studio (1234) on loopback
    instead.

---

## 4. Rust options

| Option | Licence (crates.io) | Latest / updated | Fit |
|---|---|---|---|
| **Own layer**: reqwest 0.13 + serde_json + SSE parser | — (already MIT/Apache deps) | — | Full control over the quirks in §2/§3. No new transitive tree. Fits `AppError`/keychain/FFI. **Recommended.** |
| `async-openai` | MIT | 0.42.0, 2026-09-09. Uses reqwest ^0.13. Has BYOT and retry [57][59] | OpenAI-typed only. 257 commits by the owner vs single digits for everyone else [70]: bus factor 1. Useful as a **type reference**. |
| `genai` (rust-genai) | MIT OR Apache-2.0 | 0.6.5, 2026-09-23. **0.7 beta brings breaking changes** (`Client::new()?`) [57][58] | Broadest set of adapters. It preserves signatures and thinking blocks across tool turns (0.7 beta). 907 of the top commits are by one maintainer [70]. **Plan B or reference only**, pinned. |
| `rig-core` | MIT | 0.42.0, 2026-08-17 [57] | An agent/RAG framework. The README says future updates "**will** contain **breaking changes**" [60]. Too heavy. |
| `llm` (graniet), `openai-api-rs`, `misanthropic`, `clust`… | MIT / MIT OR Apache | assorted [57] | Smaller, less active, or single-vendor. No. |
| `ollama-rs` | "non-standard" on crates.io [57] | 0.3.6 | Licence field unclear: **fails our deny check until reviewed**. Not needed. |
| SSE: `eventsource-stream` 0.2.3 (2022, nom 7), `sse-stream` 0.3.0 (2026-09, Apache/MIT), `reqwest-eventsource` 0.6 | MIT/Apache | [57] | **`reqwest-eventsource` pins reqwest ^0.12, which would duplicate reqwest.** Use `sse-stream` or hand-roll it. |
| `tokio-util` (`CancellationToken`) | MIT | already in the lockfile | Yes. |
| `jsonschema` (local validation of model JSON) | MIT | 0.58.1 [57] | Yes, or hand-validate our few schemas. `schemars` 1.2 already generates them. |

---

## 5. Cross-cutting design

**Key storage.**
- Reuse `core::secrets` with account `llm:<provider_id>` (for example `llm:openai`, or
  `llm:custom-<short-hash>`), which gives the env override `PAGELAMP_SECRET_LLM_OPENAI` for free.
  keyring 4's default features are fine. Its README only warns apps against linking the `cli` feature
  [62][V].
- Keys never go into the DB, logs, the frontend beyond the input field, MCP, or reports. The UI shows
  only the last 4 characters.
- Validate a key with a free call (`GET /v1/models` on every vendor that has it).
- `remove_model_provider` deletes the keychain entry.
- Require HTTPS for any non-loopback base URL. Build the LLM reqwest client with
  `redirect::Policy::none()` so an Authorization header can never follow a redirect.

**Streaming.**
- The driver turns SSE into IR `StreamEvent`s.
- The facade forwards them through a Tauri `Channel` and through a Swift foreign-trait
  `GenerationObserver` (the same pattern as `SyncObserver`). The CLI prints deltas.
- Chat Completions needs `stream_options: {include_usage: true}` to get usage.
- Anthropic sends `ping` events. Use a generous time-to-first-byte (reasoning models can think for
  minutes; Anthropic recommends streaming for anything long [20][V]) and a separate idle timeout
  between events.

**Cancellation.**
- The facade keeps `generation_id → CancellationToken` and exposes `cancel_generation(id)`. The
  driver uses `select!` on the token, and dropping the stream closes the connection.
- **This must be an explicit API.** UniFFI 0.32's Swift side doesn't propagate Task cancellation [63][64][V],
  and a Tauri `invoke` can't be cancelled from JS either.
- Providers generally stop billing when the client cancels [65][S]. The final usage event is lost,
  though [66][S], so estimate the output tokens (streamed characters ÷ ~4) and mark the cost "≈".

**Token and cost display.**
- A local ledger table stores only token counts and the estimated cost, never content: `ts,
  provider_id, model, feature, input_uncached, cache_read, cache_write, output, reasoning,
  est_cost_micro_usd, estimated, outcome`.
- Prices come from a **models.dev snapshot vendored at release time** (MIT; the repo is now
  `anomalyco/models.dev` [61][V]; fields include cache prices, limits, `tool_call`,
  `structured_output`, `reasoning`) plus our own overlay.
- OpenRouter returns `usage.cost` directly. Local models show tokens and seconds at $0.
- An unknown price shows tokens only.

**Budgets.**
- A local soft cap with a pre-flight estimate (characters ÷ 4) blocks new generations over the cap,
  with an override.
- The settings page recommends the provider's own hard caps: an OpenAI project spend limit or
  prepaid credits [9], an Anthropic spend limit (which returns 400) [19], a Gemini project cap or
  prepaid credits [29], OpenRouter credits (which return 402) [31].
- Estimated monthly cost for the draft workload (2.3M input + 0.15M output tokens, no caching; add
  roughly 1.5× on output where thinking is always on):

  | Model | $/month |
  |---|---|
  | gpt-6-luna | 0.30 |
  | gemini-2.5-flash-lite | 0.29 |
  | gpt-5.4-nano | 0.65 |
  | gemini-3.1-flash-lite | 0.80 |
  | gpt-5.4-mini | 2.40 |
  | gemini-3.8-flash | 2.29 |
  | claude-haiku-4-5 | 3.05 |
  | claude-sonnet-5 | 6.10 |
  | claude-opus-5-5 | 12.20 |
  | gpt-6-astra | 30.50 |

  (My arithmetic from [2][14][24].) Per-feature routing to small models is therefore essential.

**Retries** (reuse the canvas `RetryPolicy` idea). Retry only **before the first streamed byte**:
- 429 with `retry-after`: honour it, capped at 60 s.
- 500/502/503/504/529 and connection errors: exponential backoff with jitter, 3 tries.

Never retry:
- 400/401/402/403/404/413.
- OpenAI quota/spend 429 codes.
- Anthropic 429 with `enforced_spend_limit_reached` (no `retry-after`).

For a mid-stream error, retry automatically only if no text has been emitted yet. Otherwise show
"Try again", so the student isn't silently billed twice.

**Error taxonomy.** This is the facade's equivalent of `SourceErrorKind`: `auth | billing_or_quota |
rate_limited{retry_after} | overloaded | invalid_request | model_not_found | context_too_long | refused
| content_filtered | network | timeout | cancelled | blocked_key{reason} | unsupported{capability}`. The
UI branches on the kind, never on message strings.

---

## 6. Recommended provider abstraction

```rust
// crates/pagelamp-llm — internal; the facade exposes only plain Serialize+JsonSchema types.

pub enum Wire { OpenAiResponses, OpenAiChat, AnthropicMessages, OllamaNative /* later: GeminiNative, OpenResponses */ }

/// Data, not code: shipped as embedded TOML + brand overlay; user-added "custom" profiles too.
pub struct ProviderProfile {
    pub id: String, pub display_name: String, pub wire: Wire,
    pub base_url: Url, pub auth: Auth /* Bearer | XApiKey | None */, pub extra_headers: Vec<(String, String)>,
    pub quirks: Quirks,            // e.g. no_temperature, no_prefill, no_forced_tool, json_object_only,
                                   // needs_include_usage, max_schema_depth, thinking_cannot_disable, reasoning_field
    pub data_policy: DataPolicy,   // training, retention, location, min_age, on_device — shown in UI (Canvas §2E)
    pub key_hints: Vec<String>,    // display-only prefix hints; never used to pick the vendor
    pub blocked: Option<BlockRule>,// coding-plan hosts/paths/key prefixes + vendor sentence
}

pub struct GenerateRequest {
    pub model: String,
    pub instructions: String,
    pub context: GatedContext,          // ONLY constructible by pagelamp-core's policy module (AiMaterialsState)
    pub turns: Vec<Turn>,               // v0.3 features: one user turn; follow-up/tutor later
    pub output: OutputSpec,             // Text | Json { name, schema /* LCD subset */, }
    pub tools: Vec<ToolSpec>,           // empty in v0.3.0
    pub effort: Effort,                 // Lowest | Low | Medium | High → mapped per wire/quirk
    pub max_output_tokens: u32,
    pub cache: CacheHint,               // stable-prefix boundary (Anthropic cache_control; OpenAI prompt caching)
}
pub struct Turn { pub role: Role, pub parts: Vec<Part>, pub provider_state: Vec<ProviderState> }
pub struct ProviderState { pub backend: String, pub model: String, pub raw: serde_json::Value } // opaque reasoning, replayed verbatim, dropped if model changes

pub enum StreamEvent { TextDelta(String), Usage(Usage), Notice(NoticeCode) }
pub struct Usage { pub input_uncached: u64, pub cache_read: u64, pub cache_write: u64, pub output: u64, pub reasoning: Option<u64>, pub estimated: bool }
pub enum StopReason { Complete, MaxTokens, Refusal(String), ContentFilter, Cancelled }
pub struct Outcome { pub text: String, pub json: Option<serde_json::Value>, pub stop: StopReason, pub usage: Usage,
                     pub provider_state: Vec<ProviderState>, pub request_id: Option<String>, pub model_reported: String }

/// Closed set, enum dispatch: no dyn/async-trait, no plugin surface, exhaustive matches in tests.
pub enum Backend { Http(HttpDriver /* profile + Wire */), CodexExec(CodexCli), ClaudeCodeHeadless(ClaudeCli) }
impl Backend {
    pub async fn list_models(&self, cancel: &CancellationToken) -> Result<Vec<ModelInfo>, LlmError>;
    pub async fn probe(&self, model: &str, cancel: &CancellationToken) -> Result<ProbeReport, LlmError>; // json_schema? thinking toggle? tools? context?
    pub async fn generate(&self, req: GenerateRequest, on_event: &(dyn Fn(StreamEvent) + Send + Sync),
                          cancel: CancellationToken) -> Result<Outcome, LlmError>;
}
```

The facade additions in `pagelamp-app` are thin wrappers for Tauri and UniFFI:

```rust
pub fn model_provider_presets(&self) -> Vec<ProviderPreset>;                 // incl. data-policy lines, blocked flags
pub async fn add_model_provider(&self, preset: &str, base_url: Option<&str>, api_key: Option<&str>) -> Result<ModelProviderRecord>;
pub async fn update_model_provider_key(&self, id: &str, api_key: &str) -> Result<ModelProviderRecord>;
pub fn remove_model_provider(&self, id: &str) -> Result<()>;                // deletes keychain entry
pub async fn list_provider_models(&self, id: &str) -> Result<Vec<ModelInfo>>; // live list + catalog merge + on_device flag
pub async fn test_model_provider(&self, id: &str, model: &str) -> Result<ProbeReport>;
pub fn set_feature_model(&self, feature: AiFeature, id: &str, model: &str, effort: Effort) -> Result<()>;
pub fn usage_summary(&self, month: Option<NaiveDate>) -> Result<UsageSummary>;
pub fn set_monthly_budget(&self, usd: Option<f64>) -> Result<()>;
pub async fn generate_study_plan(&self, req: StudyPlanRequest, generation_id: &str, on_event: impl Fn(GenEvent) + Send + Sync) -> Result<GenerationResult>;
pub async fn explain_week(&self, course: &str, week: Option<u32>, generation_id: &str, on_event: impl Fn(GenEvent) + Send + Sync) -> Result<GenerationResult>;
pub fn cancel_generation(&self, generation_id: &str);                      // explicit (UniFFI/Tauri can't cancel futures)
```

Rules built into this shape:

1. Structured output falls back through tiers: native strict schema → `json_object` → prompt-only. It
   always uses the lowest-common-denominator schema (no numeric or string bounds, no recursion, every
   field required, `additionalProperties:false`, depth 5 or less), validates locally, and retries a
   repair once.
2. The effort mapping is per wire. "Lowest" means `none`/`minimal` where the model allows it. Where
   thinking can't be turned off (Opus 5.5, Gemini 3.x Flash), it maps to the lowest level and the
   UI says so.
3. Temperature is sent only where the quirk flags allow it.
4. OpenAI and Gemini native get `store:false`.
5. The MCP server never links `pagelamp-llm`.

---

## 7. Which providers ship first

| Phase | Ship | Why |
|---|---|---|
| v0.3.0 | **OpenAI (Responses)**, **Anthropic (Messages)** | These are the students' own vendors, and the only way to get strict schemas, caching and reasoning right. |
| v0.3.0 | **Ollama (native)**, plus **LM Studio** as a loopback preset on the generic driver | The on-device privacy option. Ollama needs `num_ctx` control and cloud-model detection. |
| v0.3.0 | **Generic OpenAI-compatible (Chat Completions)** with presets **OpenRouter** and **Gemini (OpenAI-compatible)** | One driver, many vendors. OpenRouter gives students one key for many models. Gemini is the only free cloud tier (with the warnings in §3.3). |
| v0.3.x | Tier-2 presets on the generic driver (DeepSeek, Alibaba intl, Moonshot, Z.ai and MiniMax pay-as-you-go), each with a data-location line | Low priority, per the user's decision. |
| later | Gemini native (Interactions, `store:false`), an Open Responses dialect, tool loops for the tutor | Only when follow-up chat or the tutor needs multi-turn tools with opaque state. |
| never | Coding-plan keys and endpoints, reverse-engineered web APIs, any relay | Terms (§3.5) and user decisions. |

Effort (my estimate [U]): backend (4 drivers, profiles, SSE, retries, ledger, probe, facade) about 3–4
person-weeks. Frontend settings, usage and generation UI about 2 weeks. Swift bindings about 1 week.
That excludes the tutor loop.

---

## 8. Risks

- **Parameter and model churn is monthly.** Recent examples: 400s for temperature, prefill, forced tools
  and thinking=disabled on Claude; Chat Completions tool restrictions on GPT-5.4+; the gpt-5 snapshots
  shutting down on 2026-12-11; Haiku 4.5 retirement possibly in October. Mitigations: data-driven
  quirks, a runtime probe, recorded-fixture contract tests per wire, and never hard-coding default
  model IDs.
- **Cost surprises.** Thinking is always on for some models, native PDF input costs about 1.5–3k tokens
  per page plus images, and frontier prices are high. Mitigations: per-feature routing, a lowest-effort
  default, local extraction by default, a ledger with a cap, and provider hard caps.
- **Privacy traps.**
  - Ollama `-cloud` models on localhost.
  - OpenAI `store` defaults to true (30 days).
  - Gemini Interactions stores for 55 days.
  - The Gemini free tier allows training and human review.
  - DeepSeek stores data in the PRC.
  - OpenRouter's upstream provider varies unless `data_collection:"deny"`/`zdr` is set.
- **Terms.** Coding-plan keys; the MiniMax terms are unverified; OpenAI's position on BYOK is unstated;
  age limits (Gemini 18+; Anthropic unverified).
- **Engineering.** No UniFFI cancellation (we need the explicit API). Anthropic thinking-block binding
  breaks non-append-only history. Mid-stream errors after a 200. Lost usage on cancel.
- **Local quality.** Hallucination, a silent 4k-context cut-off, and slow CPUs. Mitigations: grounding,
  citations, `num_ctx`, thinking off, and honest labels.
- **Academic integrity.** Prompts and the policy gate reduce misuse but cannot stop a student asking for
  graded work. Rule 4 still holds: assignment instructions are never stored.

---

## 9. Corrections to `docs/design/v0.3-model-access.md` (2026-09-25)

1. "file inputs … are Responses-only": **only partly right**. Chat Completions accepts PDFs (file_id or
   base64). Only non-PDF files and file URLs are Responses-only [3].
2. Anthropic's OpenAI-compatible layer: `thinking` is **accepted** but the thought process isn't returned.
   The rest is confirmed [18].
3. "Refuse Coding Plan keys: detect by base URL": **MiniMax plan keys (`sk-cp-`) use the normal host**.
   Add key-prefix rules (`sk-sp-` for Alibaba [39]) and the Kimi overseas host `api.kimi.ai/coding` [40].
4. The cost estimate is out of date. For the same workload: frontier $12 (Opus 5.5) to $30 (GPT-6 Astra)
   per month. "Mid" (Haiku 4.5) is about $3 uncached. Cheapest is about $0.3 (gpt-6-luna,
   gemini-2.5-flash-lite).
5. Gemini: the OpenAI-compatible endpoint is still "beta" [23]. The native API is now **Interactions**
   (stores by default) [28], not `generateContent`.
6. Ollama native: move it from **optional to required**. Reasons: the 4k default context [47],
   `num_ctx` only in the native API [48], and cloud-model detection [46].
7. models.dev moved to `anomalyco/models.dev` [61]. Fetching it at runtime adds a network destination
   that PRIVACY.md doesn't allow, so vendor it per release instead.
8. Add: explicit `cancel_generation` (UniFFI gap [63]), `store:false` everywhere, and error-code-aware
   429 handling [9][19].

---

## 10. Questions only the owner can answer

1. **Gemini free tier.** Allow it, knowing Google may train on and humans may read course materials and
   that it's 18+? Options: offer it with an explicit acknowledgement, require the paid tier, or leave
   Gemini out.
2. **Age.** Gemini is 18+ [25], OpenAI 13+ with parental permission [S], Anthropic unverified. Some
   first-year students are 17. Is showing a per-provider eligibility line enough, or should we gate?
3. **Does `ai_policy = prohibited` also block local models?** My view: yes. A course rule against GenAI
   covers local models too, and your decision was that the policy engine sits in front of every model
   call.
4. **Default monthly budget cap.** Should there be one (e.g. US$5)? Should it be mandatory? Should
   hitting it block, or just warn?
5. **OpenRouter defaults.** Set `data_collection:"deny"` and/or `zdr:true` by default (fewer models,
   more privacy)? Send attribution headers (`X-OpenRouter-Title: PageLamp`)?
6. **DeepSeek.** Offer it as a preset at all (data in the PRC, and our audience is outside China), or
   only through OpenRouter?
7. **Catalog and prices.** Vendor a snapshot per release (my recommendation), or fetch at runtime
   (needs a PRIVACY.md change and one more destination)?
8. **Native PDF mode** in v0.3, as an opt-in high-fidelity mode, or local extraction only?
9. **Default model suggestions.** Hard-code a per-provider "cheap" pick in each release, or always ask
   the student to choose from their live model list?
10. **LM Studio** (proprietary, free) as a first-class preset next to Ollama, or just documented?
11. **Live API tests in CI.** They need real keys as CI secrets and small spend. Is that acceptable
    under the secrets rules, or fixtures only?
12. **Ownership.** Confirm the split: `pagelamp-llm` and the facade to the backend developer, the
    settings, usage and generation UI to the frontend developer, and the Swift observer and cancel
    wiring to the leader.

---

## 11. Sources (all accessed 2026-09-27)

1. OpenAI, Migrate to the Responses API — https://developers.openai.com/api/docs/guides/migrate-to-responses
2. OpenAI, Pricing — https://developers.openai.com/api/docs/pricing
3. OpenAI, File inputs (PDF) — https://developers.openai.com/api/docs/guides/pdf-files
4. OpenAI, Structured outputs — https://developers.openai.com/api/docs/guides/structured-outputs (limits: https://platform.openai.com/docs/guides/structured-outputs via search [S])
5. OpenAI, Rate limits — https://developers.openai.com/api/docs/guides/rate-limits
6. OpenAI, Your data — https://developers.openai.com/api/docs/guides/your-data
7. OpenAI, Conversation state — https://developers.openai.com/api/docs/guides/conversation-state
8. OpenAI, Background mode — https://developers.openai.com/api/docs/guides/background
9. OpenAI, Error codes — https://developers.openai.com/api/docs/guides/error-codes
10. OpenAI, Deprecations — https://developers.openai.com/api/docs/deprecations
11. OpenAI, Models — https://developers.openai.com/api/docs/models
12. OpenAI Community, Assistants API sunset — https://community.openai.com/t/assistants-api-beta-deprecation-august-26-2026-sunset/1354666 [S]
13. OpenAI, Streaming responses — https://developers.openai.com/api/docs/guides/streaming-responses
14. Anthropic, Models overview — https://platform.claude.com/docs/en/docs/about-claude/models/overview
15. Anthropic, Structured outputs — https://platform.claude.com/docs/en/docs/build-with-claude/structured-outputs
16. Anthropic, Prompt caching — https://platform.claude.com/docs/en/build-with-claude/prompt-caching
17. Anthropic, PDF support — https://platform.claude.com/docs/en/build-with-claude/pdf-support
18. Anthropic, OpenAI SDK compatibility — https://platform.claude.com/docs/en/api/openai-sdk
19. Anthropic, Rate limits — https://platform.claude.com/docs/en/api/rate-limits
20. Anthropic, Errors (incl. SDK list, validation errors) — https://platform.claude.com/docs/en/api/errors
21. Anthropic, Model deprecations — https://platform.claude.com/docs/en/about-claude/model-deprecations
22. Anthropic, API and data retention — https://platform.claude.com/docs/en/manage-claude/api-and-data-retention
23. Google, Gemini OpenAI compatibility — https://ai.google.dev/gemini-api/docs/openai
24. Google, Gemini pricing — https://ai.google.dev/gemini-api/docs/pricing
25. Google, Gemini API terms — https://ai.google.dev/gemini-api/terms
26. Google, Thinking — https://ai.google.dev/gemini-api/docs/thinking
27. Google, Structured output — https://ai.google.dev/gemini-api/docs/structured-output
28. Google, Interactions API overview — https://ai.google.dev/gemini-api/docs/interactions-overview
29. Google, Billing — https://ai.google.dev/gemini-api/docs/billing
30. OpenRouter, Data collection — https://openrouter.ai/docs/guides/privacy/data-collection
31. OpenRouter, API overview — https://openrouter.ai/docs/api/reference/overview
32. OpenRouter, Provider selection — https://openrouter.ai/docs/guides/routing/provider-selection
33. OpenRouter, Responses API beta — https://openrouter.ai/docs/api_reference/responses/overview [S via search]
34. Open Responses — https://www.openresponses.org/ (spec: https://www.openresponses.org/specification)
35. DeepSeek, JSON output — https://api-docs.deepseek.com/guides/json_mode
36. DeepSeek, Privacy policy — https://cdn.deepseek.com/policies/en-US/deepseek-privacy-policy.html [S via search]
37. Z.ai, Coding Plan usage policy — https://docs.z.ai/devpack/usage-policy
38. Z.ai, Subscription terms §4 — https://docs.z.ai/legal-agreement/subscription-terms
39. Alibaba Cloud, Model Studio Coding Plan — https://www.alibabacloud.com/help/en/model-studio/coding-plan
40. Moonshot, Kimi Code docs — https://www.kimi.com/code/docs/en/
41. MiniMax, Token Plan docs — https://platform.minimax.io/docs/token-plan/openclaw (no restriction text found); key prefix via search [S]
42. Ollama, OpenAI compatibility — https://docs.ollama.com/api/openai-compatibility
43. Ollama, Structured outputs — https://docs.ollama.com/capabilities/structured-outputs
44. Ollama, Cloud — https://docs.ollama.com/cloud
45. Ollama, FAQ — https://docs.ollama.com/faq
46. Ollama, List models (/api/tags) — https://docs.ollama.com/api/tags
47. Ollama, Context length — https://docs.ollama.com/context-length
48. Ollama, Chat API — https://docs.ollama.com/api/chat
49. Humla, "Is Ollama local?" — https://humla.team/blog/is-ollama-local [S]
50. LM Studio, OpenAI compatibility — https://lmstudio.ai/docs/developer/openai-compat
51. LM Studio, Free for work — https://lmstudio.ai/blog/free-for-work
52. llama.cpp server README — https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md
53. Google, Gemma 4 model card — https://ai.google.dev/gemma/docs/core/model_card_4
54. Ollama library, qwen3.5 — https://ollama.com/library/qwen3.5
55. Artificial Analysis, Qwen3.5 small models (2026-03-05) — https://artificialanalysis.ai/articles/qwen3-5-small-models
56. OpenAI, Introducing gpt-oss — https://openai.com/index/introducing-gpt-oss/ and https://huggingface.co/openai/gpt-oss-20b [S via search]
57. crates.io API — https://crates.io/api/v1/crates/{async-openai,genai,rig-core,llm,openai-api-rs,misanthropic,clust,ollama-rs,reqwest-eventsource,eventsource-stream,sse-stream,keyring,tokio-util,jsonschema} (+ /dependencies)
58. rust-genai — https://github.com/jeremychone/rust-genai
59. async-openai — https://github.com/64bit/async-openai
60. rig — https://github.com/0xPlaygrounds/rig
61. models.dev — https://github.com/anomalyco/models.dev (redirected from sst/models.dev)
62. keyring-rs — https://github.com/open-source-cooperative/keyring-rs
63. UniFFI PR #3007 (Swift task cancellation, open) — https://github.com/mozilla/uniffi-rs/pull/3007
64. UniFFI issue #2771 — https://github.com/mozilla/uniffi-rs/issues/2771
65. OpenRouter help, cancelling streams — https://openrouter.zendesk.com/hc/en-us/articles/51691588409883 [S]
66. openai-openapi issue #539 (usage lost on abort) — https://github.com/openai/openai-openapi/issues/539 [S]
67. U of T AI Kitchen (students: Copilot Chat only; no student API access) — https://ai.utoronto.ca/ai-kitchen/
68. OpenAI Terms of Use (age) — https://openai.com/policies/row-terms-of-use/ [S via search]
69. openai-go issue #628 (no official Rust SDK) — https://github.com/openai/openai-go/issues/628 [S]
70. GitHub REST API repo/contributor metadata — https://api.github.com/repos/{ollama/ollama,ggml-org/llama.cpp,lmstudio-ai/lms,jeremychone/rust-genai,64bit/async-openai,0xPlaygrounds/rig,anomalyco/models.dev}
