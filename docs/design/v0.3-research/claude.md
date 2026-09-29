# Research: the student's own Claude Code as PageLamp's Claude-subscription path (v0.3)

Research date: 2026-09-27. Scope: v0.3 "mode B" in `docs/design/v0.3-model-access.md` (the student's
unmodified Claude Code run headless), Anthropic policy on third-party apps using a Pro/Max
subscription, the written confirmation to ask for, plan limits, platform support, how to detect
Claude Code without touching its credentials, and whether a Claude Desktop `.mcpb` is still a
good v0.2 item.
Citations are `[Sn]` (list at the end; every source accessed 2026-09-27). Labels:
**[secondary]** = press or blog report, not Anthropic; **[community]** = GitHub issue or third-party
project; **[unverified]** = not confirmed from a primary source.

中文摘要：Anthropic 的 Claude Code 法律页在 2026-08-16 到 09-03 之间新增了一段："用户用自己的 Claude
订阅登录**未修改**的 Claude Code，产品可以运行它"，条件是开发者接受 Commercial Terms、不改二进制、不删减任何
登录方式、不代付不转售。这比 2 月份的版本宽松很多（2 月版写明订阅 OAuth 不得用于"任何其他产品，包括 Agent
SDK"）。剩下的灰色地带：本地 app 程序化调用 `claude -p` 算"运行 Claude Code"（允许），还是算"代用户路由订阅
凭据"（禁止）？定时后台运行算不算"普通个人使用"？Anthropic 服务端还有"第三方 harness 检测"，可能把调用改记
为额外付费用量。建议：照你的规定，没有书面确认就不上线；现在就通过法律页指定的"contact sales"渠道发确认请求；
代码先做好，默认关闭；同时把零风险路线（v0.1 的反向 MCP + 只含 skills 的插件 + Claude Desktop 本地定时任务
配方）做成 Claude 订阅用户的默认路线。`.mcpb` 还能装，但 Anthropic 目录已不再接收 MCPB，签名工具有 bug，
建议降级为小项。

---

## 1. Recommendation (short)

1. **Keep the owner's rule: no mode B in a release until Anthropic confirms in writing.** The policy
   is much friendlier than when the v0.3 draft's sources were gathered: since early September 2026
   Anthropic's legal page explicitly allows "an end user signing in to the unmodified Claude Code
   binary with their own Claude subscription", and allows products that run Claude Code under
   conditions [S1][S2]. Real gray areas remain (§4.3), enforcement "may" come "without prior
   notice" [S1], and Anthropic has already pulled one subscription change on the day it was due to
   take effect [S19]. **Send the confirmation request now** (§8); lead times are unknown.
2. **Shape (when it's a go):** PageLamp finds the student's **own installed** Claude Code (never
   bundled, never installed or updated by us), checks it (signature, version, `claude auth status`),
   and runs **one single-turn, tool-less, isolated** `claude -p` per generation. PageLamp's policy
   engine builds the whole prompt first, and the answer comes back as `--json-schema` structured
   output. No Agent SDK, no `--bare`, no credential access, no scheduled background runs unless
   Anthropic says they're fine. It is one backend behind the same `pagelamp-llm` trait as BYOK,
   exposed through `pagelamp-app` so Tauri and Swift both get it. Build it behind a cargo feature
   that is off in release builds until the go conditions in §7 hold.
3. **Make the zero-risk paths the default for Claude subscribers now:** the v0.1 reverse MCP, plus
   (a) a skills-only PageLamp plugin for Claude Code/Cowork ("plan my week", "explain week N") and
   (b) a copyable recipe for a **Claude Desktop local scheduled task** that calls PageLamp's MCP
   tools weekly. In both, the student runs Anthropic's own app, so the question of whether a
   third-party app may drive a subscription never comes up [S15][S37].
4. **`.mcpb`: still worth doing, but downgrade it** from a v0.2 headline item to a small early-v0.3
   task after signed releases and the updater. The directory no longer accepts MCPB submissions,
   privately distributed bundles only update by hand, `mcpb sign` currently produces bundles Claude
   Desktop rejects, and install bugs appeared in mid-2026 [S35][S36][S38].

---

## 2. What changed since 2025 (timeline)

| Date | Event | Source |
|---|---|---|
| ≤ 2026-02-01 | Claude Code legal page has no authentication section (snapshot of 2026-02-01). | [S2] |
| 2026-01-09 | Server-side block of third-party clients (e.g. OpenCode) that **spoofed** Claude Code to use subscription OAuth tokens ("This credential is only authorized for use with Claude Code…"). | [S28] [secondary] |
| 2026-02-19 | Legal page adds **"Authentication and credential use"**: OAuth "is intended exclusively for Claude Code and Claude.ai"; using Free/Pro/Max OAuth tokens "in any other product, tool, or service — including the Agent SDK — is not permitted"; developers must use API keys; no third-party "Claude.ai login" or routing through plan credentials "on behalf of their users"; "Advertised usage limits for Pro and Max plans assume ordinary, individual usage of Claude Code and the Agent SDK." | [S2] (Wayback 2026-02-19) |
| 2026-02-20 | Anthropic engineer (Thariq Shihipar): third-party harnesses using subscriptions "are prohibited by our Terms of Service". | [S27] [secondary] |
| 2026-04-04 | "Third-party harnesses" (OpenClaw first) stop drawing on plan limits and draw on **extra usage** (pay-as-you-go) instead; one-time credits offered. | [S26][S27] [secondary] |
| between 2026-03-21 and 2026-04-21 | Legal page **drops** the "including the Agent SDK — is not permitted" sentence; OAuth becomes "designed to support ordinary use of Claude Code and other native Anthropic applications". The ban on offering Claude.ai login and on routing through plan credentials "on behalf of their users" stays. | [S2] (Wayback 2026-03-21 vs 2026-04-21) |
| ~2026-05-03 | Anthropic confirms "a bug with the third-party harness detection": Claude Code scanned git status in the system prompt for words like "OpenClaw"/"hermes" and **re-billed** sessions; refunds given. So a **server-side harness classifier exists**. | [S29] [secondary] |
| 2026-05-13 | Anthropic announces a monthly **Agent SDK credit** from 2026-06-15 (Pro $20, Max 5x $100, Max 20x $200) covering the Agent SDK, `claude -p`, Claude Code GitHub Actions and "third-party apps built on the Agent SDK", taking this usage out of plan limits. Alex Albert (Anthropic): it covers "third-party apps built on the SDK (OpenClaw, Conductor, etc)". | [S19][S24] (X posts seen only as search snippets, [unverified] wording); [S25] [secondary] |
| 2026-06-15 | **Paused** on the day it was due: "For now, nothing has changed: Claude Agent SDK, `claude -p`, and third-party app usage still draw from your subscription's usage limits." Anthropic says it will announce any new plan before it takes effect. | [S19] |
| between 2026-08-16 and 2026-09-03 | Legal page adds **"Can customers offer Claude Code in their products?"** (conditions below) and: "Nor does it prevent an end user from signing in to the unmodified Claude Code binary with their own Claude subscription"; also "developers may not collect, store, or intermediate Claude.ai credentials or session tokens". | [S1][S2] (Wayback 2026-08-16 vs 2026-09-03) |
| 2026-08-27 / 09-02 / 09-22 | Claude Code v2.1.248 adds `--restricted`; v2.1.259 adds `--permission-prompts`; v2.1.280 makes **Opus 5.5 the default model on Pro** (Pro used to default to Sonnet). A community report says 2.1.280 dropped `apiKeySource` from the stream-json output. | [S10][S14][S11]; [S31] [community] |

Net effect for PageLamp: the February rule that made mode B a clear "no" is gone. The September
text describes almost exactly what the v0.3 draft proposes. What's unresolved is metering and
classification (plan limits vs a credit pool vs extra usage) and how the "on behalf of their
users" wording applies to a local app.

---

## 3. Findings: policy

### 3.1 Claude Code legal page, current text [S1]

- **Terms:** Free/Pro/Max users are under the Consumer Terms; Team/Enterprise/API users are under
  the Commercial Terms.
- **"Can customers offer Claude Code in their products?"** "Unless we've mutually agreed otherwise,
  preinstalling or running Claude Code in your products or services (e.g. in hosted sandboxes or
  other agent infrastructure) requires agreeing to our Commercial Terms of Service and complying with
  the conditions below":
  - "The Claude Code binary must not be modified." The customer "may not remove, disable, or restrict
    any authentication method built into it (including methods that permit signing in with a Claude
    account or the user's own API key)."
  - The customer "may not pay for, resell, or intermediate Claude usage on their end users' behalf."
    Each end user authenticates with their own API key, subscription credentials, or cloud
    credentials, billed to them.
- **Name and logo:** you may "accurately say, in plain text, that your product … runs Claude Code".
  You may **not** use the Claude Code/Anthropic names or logos in your product, feature or company
  name, in your logo, or in a way that suggests endorsement.
- **Acceptable use:** "Advertised usage limits for Pro and Max plans assume ordinary, individual
  usage of Claude Code and the Agent SDK."
- **Authentication and credential use:** OAuth is for purchasers of Free/Pro/Max/Team/Enterprise
  plans, "designed to support ordinary use of Claude Code and other native Anthropic applications".
  Developers of products "should use API key authentication". "Anthropic does not permit third-party
  developers to offer Claude.ai login into their own applications, or to route requests through
  Free, Pro, or Max plan credentials on behalf of their users." Developers "may not collect, store,
  or intermediate Claude.ai credentials or session tokens — sign-in to a Claude account must complete
  through Anthropic's own flow." **"Nor does it prevent an end user from signing in to the unmodified
  Claude Code binary with their own Claude subscription, including where a platform hosts Claude
  Code as described under Can customers offer Claude Code in their products?"** Enforcement "may"
  happen "without prior notice". Questions about permitted authentication methods go to **contact
  sales**.

### 3.2 Agent SDK [S3][S19]

- The SDK is "a library that runs the Claude Code binary", for Python/TypeScript. For other
  languages the docs say to "run the CLI as a subprocess with the `-p` flag and
  `--output-format json`", which is exactly the Rust route.
- The overview still carries this note: "Unless previously approved, Anthropic does not allow third
  party developers to offer claude.ai login or rate limits for their products, including agents
  built on the Claude Agent SDK." SDK use is governed by the Commercial Terms.
- → The draft's rejection of "Agent SDK with claude.ai login" still holds. The SDK would also need a
  Node/Python sidecar, which the draft rules out.

### 3.3 Consumer Terms, Commercial Terms, Usage Policy

- **Consumer Terms** (effective 2025-10-08) [S16]: users must be **18+** or the local age of
  consent, whichever is higher. No sharing of login or credentials. Prohibited: "Except when you are
  accessing our Services via an Anthropic API Key or where we otherwise explicitly permit it, to
  access the Services through automated or non-human means, whether through a bot, script, or
  otherwise." Anthropic explicitly documents `claude -p` for subscribers and says `claude -p` and
  third-party-app usage "draw from your subscription's usage limits" [S19], which reads as an
  explicit permission. The student is the party bound by these terms, so PageLamp must never nudge
  them outside what's permitted.
- **Commercial Terms** (effective 2025-06-17) [S17]: they take effect on first electronic consent
  or first access to the Services. No resale or competing products "except as expressly approved".
  The terms don't say how a developer who never calls the API "accepts" them. That is a question
  for Anthropic (§8).
- **Usage Policy** (effective 2025-09-15) [S18]: prohibits "Plagiarize or submit AI-assisted work
  without proper permission or attribution". This matches PageLamp's "never fetch or solve
  assignments" rule and its per-course policy engine. Consumer-facing AI agents must disclose that
  users are talking to AI.
- **Claude Code licence** [S23]: "© Anthropic PBC. All rights reserved. Use is subject to
  Anthropic's Commercial Terms." Redistributing (bundling) it inside a desktop installer is not
  clearly covered by the "preinstalling" clause, which is written for hosted sandboxes. → **Don't
  bundle Claude Code** (unlike the Codex plan).

### 3.4 Enforcement and metering behaviour to design for

- A server-side "third-party harness detection" exists and has misfired by matching keywords in
  context [S29]. How it classifies traffic is not public. A PageLamp run with a replaced system
  prompt could be classified differently from stock Claude Code: it might be billed as extra usage,
  or refused. **Must be tested (§12) and asked about (§8).**
- Stream events expose metering state that PageLamp can read **without tokens**:
  `rate_limit_event.rate_limit_info` with `status` (`allowed | allowed_warning | rejected`),
  `utilization`, `resetsAt`, and `errorCode: "credits_required"` when "a claude.ai subscription whose
  included usage is exhausted" can't continue until the user buys credits [S13].

### 3.5 Classification for PageLamp

| Clearly allowed (per [S1]) | Clearly not allowed | Gray: ask Anthropic |
|---|---|---|
| The student signs in to their own unmodified Claude Code through Claude Code's own flow, with their own Pro/Max plan | PageLamp offering a "Sign in with Claude" UI; asking students to paste `claude setup-token` output; reading, storing or relaying tokens | Whether a **local desktop app** driving `claude -p` counts as "running Claude Code in your products" (allowed) or as routing requests through plan credentials "on behalf of their users" (not allowed). The September text leans toward allowed. |
| A product running Claude Code if the developer accepted the Commercial Terms, the binary is unmodified, no auth method is restricted, and nobody pays for, resells or intermediates usage | Calling Anthropic's API with subscription OAuth outside Claude Code (spoofing); the Agent SDK with claude.ai login "unless previously approved" | Whether **scheduled/background** generation (e.g. every Monday) counts as "ordinary, individual usage" |
| Saying in plain text that PageLamp "runs Claude Code" | `--bare` or `CLAUDE_CODE_SIMPLE` (they skip OAuth, i.e. **disable** an auth method, and break the path anyway); modified or repackaged binaries; "Claude Code" in a PageLamp feature name or logo | Whether `--system-prompt` (replaced prompt), `--tools ""`, and `--restricted` (which ignores user settings, including an `apiKeyHelper`) are acceptable, i.e. not "restricting" an auth method, and how they are metered |
| | | How the developer "accepts" the Commercial Terms without API usage; whether branded builds (UTMCSSA) are covered; whether PageLamp may launch `claude auth login` for the student |

---

## 4. Findings: technical (headless Claude Code)

### 4.1 The flags that matter [S4][S5][S7]

| Need | Flag / env | Notes |
|---|---|---|
| Non-interactive | `-p` | Exit 0 on success. Failures inside the run (e.g. missing auth) are printed as the **result on stdout**. Invalid flags go to stderr. |
| Machine output | `--output-format json` or `stream-json` (+ `--verbose`, optionally `--include-partial-messages`) | The final `result` message carries `subtype` (`success`, `error_max_turns`, `error_during_execution`, `error_max_budget_usd`, `error_max_structured_output_retries`), `is_error`, `result`, `structured_output`, `total_cost_usd` (a client-side **estimate**), `usage`, `modelUsage`, `session_id`, `permission_denials`, `errors[]` [S13] |
| Structured output | `--json-schema '<schema>'` | Validated. Output goes in `structured_output`. An invalid schema has been a hard error since v2.1.205 (it used to be ignored silently). `format` is only an annotation [S4]. |
| Replace / append prompt | `--system-prompt(-file)` / `--append-system-prompt(-file)` | Replacing drops Claude Code's coding prompt (fewer tokens, cleaner output). How that is **metered or classified** is unknown (§3.4). |
| No tools | `--tools ""` | Removes all built-in tools. It doesn't affect MCP tools, so also add `--disallowedTools "mcp__*"` [S5] |
| No MCP | `--strict-mcp-config` (with no or only our `--mcp-config`) | "Only use MCP servers from `--mcp-config`, ignoring all other MCP configurations" [S5] |
| No memory / CLAUDE.md / claude.ai connectors | `CLAUDE_CODE_DISABLE_CLAUDE_MDS=1`, `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1`, `ENABLE_CLAUDEAI_MCP_SERVERS=false` | Documented env vars [S7] |
| Don't persist course text locally | `--no-session-persistence` | Otherwise transcripts are kept **in plaintext under `~/.claude/projects/` for 30 days** [S12] |
| Bounds | `--max-turns N`, `--max-budget-usd X` (print mode) | The budget uses the client-side cost estimate |
| No prompts | `--permission-mode dontAsk`, `--permission-prompts none` (v2.1.259+) | Unanswerable prompts are denied instead of hanging |
| Model | `--model sonnet\|haiku\|opus` (aliases), `--fallback-model` | **Always pass `--model`.** Pro's default became **Opus 5.5** in v2.1.280 [S10][S11]. A student's saved default could be Fable, which "can bill to usage credits" depending on plan [S11]. |
| Isolation presets | `--restricted` (v2.1.248+): removes command-running tools and WebFetch, confines file tools, "loads only managed settings and `--settings`", refuses `bypassPermissions`. `--safe-mode`: disables customizations; "authentication … work[s] normally". | The docs don't say `--restricted` skips OAuth (they do say it for `--bare`), so it probably keeps the subscription login **[unverified → spike]**. `--safe-mode` is a troubleshooting flag, so don't build on it. |
| **Avoid** | `--bare` / `CLAUDE_CODE_SIMPLE` | "In bare mode, Claude Code never reads OAuth credentials or the system keychain." And **"`--bare` … will become the default for `-p` in a future release."** [S4] That is the biggest technical risk to mode B. |
| Working directory | an empty PageLamp-owned dir | Without `--bare`, `-p` runs a project's `.claude/settings.json` hooks and `.mcp.json` servers "even in a folder you've never trusted" [S4] |
| Input | prompt on **stdin** | Stdin is capped at 10 MB [S4]. It also keeps course text out of `ps` argv. |
| Stop | SIGINT ends the turn; SIGTERM exits 143 with no result [S4] | Enforce a wall-clock timeout |
| Feature detection | `system/init` has `model`, `tools`, `mcp_servers`, `mcp_server_errors`, `capabilities[]` (v2.1.205+) [S4] | Prefer this over comparing version strings |

**Auth precedence** [S6]: cloud-provider env, then `ANTHROPIC_AUTH_TOKEN`, then `ANTHROPIC_API_KEY`
("In non-interactive mode (`-p`), the key is always used when present"), then `apiKeyHelper`, then
`CLAUDE_CODE_OAUTH_TOKEN`, then profiles, then subscription OAuth from `/login`. So an
`ANTHROPIC_API_KEY` in the child's environment silently switches billing to that API key. PageLamp
must not strip it: that would arguably "restrict" an auth method. Instead, PageLamp **shows the
effective method** from `claude auth status`, run with the same environment as the generation
runs. GUI-launched apps usually lack shell variables. If PageLamp probes a login shell to find
`claude`, it takes only the path from it, not the whole environment.

**Churn** [S10]: 50 releases between 2026-08-14 (2.1.233) and 2026-09-25 (2.1.283). Fields change:
`apiKeySource` is still in the SDK type docs [S13] but reportedly gone from CLI stream-json in 2.1.280
[S31] [community]. → Parse defensively, ignore unknown fields, set a minimum version (suggest
≥ 2.1.259), keep a "last tested" version, and run a manual smoke test each PageLamp release.

### 4.2 Plans and limits

- Claude Code is included in **Pro and Max** (and Team/Enterprise). **Free has no Claude Code**
  [S20][S22]. Prices: Pro US$20/month or $17/month billed annually; Max from $100 (5x) and $200
  (20x Pro usage) [S22].
- Limits are **shared** between Claude chat and Claude Code [S20]. There is a session limit that
  resets every five hours, plus a weekly limit across all models [S21]. Past the limit the user can
  wait or buy usage credits; "To maintain usage strictly within your Pro or Max Plan allocation:
  Decline the API credit option" [S20]. How a `-p` run behaves when extra usage is already enabled
  (does it bill without asking?) is **[unverified]**. `credits_required` suggests it stops [S13].
- Reported limit changes (Claude Code 5-hour limits doubled 2026-05-06; weekly limits +25%
  permanently from 2026-09-14) are **[secondary/unverified]**. Don't hard-code any numbers.
- **Economics:** today PageLamp runs spend the student's plan limits and compete with their own
  chat and coding. If the paused credit plan returns, they would come out of a $20/month Pro credit
  at API rates [S19]. Keep runs small: cheap model for summaries, capped `--max-budget-usd`,
  single-turn, and one plan per week by default.
- **Education accounts** (Claude for Education) are institution-managed and under commercial
  terms. Whether they include Claude Code, and whether headless use is allowed, is up to the
  institution and **[unverified]** [S42]. Org restrictions surface as `oauth_org_not_allowed` in `-p`.

### 4.3 Platforms and install locations [S8][S9]

- OS: macOS 13+, Windows 10 1809+, Ubuntu 20.04+, Debian 10+, Alpine 3.19+; x64 or ARM64; 4 GB RAM.
- Native installer (recommended): `~/.local/bin/claude` (symlink into `~/.local/share/claude/versions/`)
  on macOS/Linux, `%USERPROFILE%\.local\bin\claude.exe` on Windows. It auto-updates.
- Also: Homebrew cask `claude-code` / `claude-code@latest` (no auto-update); WinGet
  `Anthropic.ClaudeCode`; `npm install -g @anthropic-ai/claude-code`; apt/dnf/apk; a legacy
  `~/.claude/local/`. Homebrew's bin dirs (`/opt/homebrew/bin`, `/usr/local/bin`) are standard
  Homebrew prefixes, not from Anthropic's docs.
- Signatures: macOS "signed by 'Anthropic PBC' and notarized"; Windows "signed by 'Anthropic, PBC'";
  **Linux binaries are not individually code-signed** (use the manifest signature or package manager).
- Windows: Git for Windows is only needed for the Bash tool. With `--tools ""` it isn't needed.
- Credentials (for the never-touch list): macOS Keychain; Linux `~/.claude/.credentials.json`;
  Windows `%USERPROFILE%\.claude\.credentials.json`. `CLAUDE_CONFIG_DIR` changes both the file and
  the Keychain entry [S6].
- Local note: on the owner's Mac, `/usr/local/bin/claude` is an **x86_64** build (Rosetta). For the
  spike, use an arm64 native install (`~/.local/bin/claude`).

---

## 5. Detecting Claude Code without touching its credentials

Algorithm for `App::claude_code_status()` (≤ 5 s, cached ~60 s, never logged beyond the fields kept):

1. **Locate:** candidates in order: a user-chosen path (Settings), then `~/.local/bin/claude`
   (`%USERPROFILE%\.local\bin\claude.exe`), then `/opt/homebrew/bin/claude`, `/usr/local/bin/claude`,
   `~/.claude/local/claude`, then a PATH lookup from a login shell (`$SHELL -lc 'command -v claude'`,
   path only) and the Windows PATH. Prefer a native `.exe` over npm `.cmd` shims on Windows (running
   `.cmd` needs `cmd.exe` and careful quoting).
2. **Verify** (macOS/Windows): check the code signature (`codesign` "Anthropic PBC" /
   Authenticode "Anthropic, PBC") [S8]. Linux: show the resolved path and let the student confirm
   it. This guards against a planted `claude` on PATH.
3. **Version:** `claude --version`. Compare with a minimum (suggest ≥ 2.1.259) and a "tested up
   to" value.
4. **Login state:** `claude auth status` prints JSON and "Exits with code 0 if logged in, 1 if not"
   [S5]. Keep only `loggedIn`, `authMethod`, `apiProvider`, `subscriptionType` (field names are
   **[community]** [S43], not documented; `configDirectory` was added per the changelog [S10]).
   **Drop `email`, `orgId`, `orgName` on parse** and never log or store them. Known quirks:
   `subscriptionType` can be null [S43]; login can report success while the CLI stays
   unauthenticated (open issue, 2026-09-16) [S33].
5. **Never:** read `~/.claude/.credentials.json` or the Keychain item; call undocumented endpoints
   (e.g. `/api/oauth/usage`: a third-party app polling it got rate-limited [community]); read
   `oauthAccount` in `~/.claude.json`. (The existing doctor check in
   `crates/pagelamp-app/src/diagnostics.rs` parses `~/.claude.json` for `mcpServers.pagelamp`.
   That's fine, but keep it to that key. The file also holds account details.)
6. **Usage display:** only from run output: `rate_limit_event.utilization/resetsAt/status` and
   `total_cost_usd`, labelled "API-equivalent estimate, not what you're billed".

---

## 6. Recommended integration shape

### 6.1 Placement (both shells)

- `crates/pagelamp-llm` gets a `ClaudeCodeBackend` behind the same `LlmBackend` trait as BYOK.
  For v0.3 it is **single-turn only**: no tool loop, no MCP.
- `pagelamp-app` (FFI-friendly, plain serialisable types) adds:
  - `claude_code_status() -> ClaudeCodeStatus { installed, path, version, version_ok, signature:
    valid|unsigned_platform|invalid|unknown, logged_in, auth_kind: subscription|api_key|cloud|other,
    plan: Option<String>, problems: Vec<ClaudeCodeProblemCode> }`, with stable codes like
    `McpNoteCode` so the UI can localise. No email.
  - `generate(req, BackendChoice::ClaudeCode, on_event)`. Progress comes from stream-json events.
    The result is labelled "AI-generated (Claude, via your Claude Code)" for Canvas §2E.
- The **policy engine runs before the backend**. PageLamp builds the prompt from its DB, honouring
  `AiMaterialsState` (`withheld_by_policy` and `turned_off` courses send structure only). Claude Code
  gets no tools and no MCP, so it **cannot fetch** anything the engine didn't include. That is
  stronger than relying on MCP-side withholding. For the later tutor loop, `--mcp-config` points to
  `pagelamp mcp` only (read-only, the same policy code).
- Cargo feature `claude-code-backend`, **off in release builds** until §7 holds. The code, tests and
  UI can land earlier.

### 6.2 Invocation recipe (to confirm in the spike)

```
<verified claude path> -p
  --output-format stream-json --verbose
  --model sonnet --fallback-model haiku          # never omit; never fable
  --append-system-prompt-file <run>/system.md    # or --system-prompt-file, decided by spike + Anthropic
  --tools "" --disallowedTools "mcp__*" --strict-mcp-config
  --json-schema '<plan|weekly-explanation schema>'
  --max-turns 3 --max-budget-usd 0.50
  --no-session-persistence
  --permission-mode dontAsk --permission-prompts none
  [--restricted]                                  # only if the spike shows OAuth works AND Anthropic is fine with it
stdin: the prompt (policy-filtered course context + task)
cwd:   <data_dir>/claude-run/ (empty, 0700, PageLamp-owned)
env:   inherited unchanged + CLAUDE_CODE_DISABLE_CLAUDE_MDS=1, CLAUDE_CODE_DISABLE_AUTO_MEMORY=1,
       ENABLE_CLAUDEAI_MCP_SERVERS=false
never: --bare, CLAUDE_CODE_SIMPLE, setup-token, touching ANTHROPIC_* or credentials
```

Handling: one run at a time; wall-clock timeout (SIGINT, then SIGTERM); map `is_error`,
`subtype`, `api_retry.error` (`authentication_failed`, `oauth_org_not_allowed`, `rate_limit`,
`billing_error`, …) and `credits_required` to stable problem codes. Validate `structured_output`
locally against the same JSON Schema, with one repair retry, then fall back to "try again later".
Study-plan **dates stay deterministic** (the draft's scheduler).

### 6.3 UX rules

- Label: "Your Claude subscription (PageLamp runs your installed Claude Code)". That is a plain-text
  statement, which is allowed. **Don't** name a feature "Claude Code mode" or use Anthropic logos [S1].
- Before first use: runs use the **same limits as your Claude chat**. Your course text goes to
  Anthropic under your account; training depends on your Claude privacy setting (consumer
  retention is 5 years if training is allowed, 30 days if not) [S12]. If usage credits are on,
  going past your limit may cost money.
- Runs start **only when the student acts** (a "Generate" button). Background or scheduled runs are
  off until Anthropic confirms (§8 Q3). Until then, weekly automation goes through the Desktop
  local-scheduled-task recipe (6.4).
- If Claude Code is missing or logged out: link to the official install docs and tell the student
  to run `claude` in a Terminal to sign in. PageLamp does not trigger the login itself until
  Anthropic OKs it.
- Don't market this path to under-18s: the Consumer Terms require 18+ [S16].

### 6.4 Zero-permission complements (ship regardless)

- **Reverse MCP** (v0.1): unchanged.
- **Skills-only plugin** for Claude Code/Cowork, e.g. `/pagelamp:plan-week`,
  `/pagelamp:explain-week`, using the PageLamp MCP server the student already added. Plugins can be
  distributed from a GitHub marketplace or submitted to the directory (any paid plan can submit).
  A plugin's local MCP servers run in Claude Code and local Cowork, **not in chat** [S37].
- **Claude Desktop local scheduled task** (Code tab → Routines → Local; Desktop ≥ 1.1.5368). It
  runs on the student's machine with MCP from the config files, but only while the app is open and
  the computer is awake, with one catch-up run after wake [S15]. PageLamp shows a copyable prompt
  ("Every Monday 8:00: read my PageLamp courses and deadlines, draft this week's plan, save it
  with save_study_plan").

---

## 7. Go / no-go conditions for mode B

**Go, i.e. enable the feature in release builds, only when all of these hold:**

1. **Written confirmation from Anthropic** by email from a named employee, covering at least Q1–Q4
   of §8. Stored in the repo with personal data removed.
2. The owner has **accepted the Commercial Terms** in the way Anthropic confirms, with the date and
   terms version recorded.
3. The **spike (§12) passes** on macOS and Windows, and Linux if supported: OAuth works under the
   chosen flags; usage lands on **plan limits, not extra usage**, with the chosen system-prompt
   mode; `--json-schema` works with `--tools ""`; nothing is written to `~/.claude/projects`; user
   hooks, MCP servers and CLAUDE.md don't leak in; logged-out and limit-reached states produce clean
   errors.
4. At release time the legal page still contains the "unmodified Claude Code binary … own Claude
   subscription" permission, and `-p` still reads OAuth, i.e. `--bare` is not the default or an
   opt-out exists. Add this to the release checklist.
5. Policy tests pass: `withheld_by_policy` and `turned_off` courses never reach the prompt, checked
   by golden tests on the prompt builder.

**No-go, or pull it in the next update (there is no server, so no remote kill switch):**
Anthropic declines or doesn't answer; the legal page reverts to February-style wording; `-p` stops
reading OAuth with no opt-out; runs are shown to be billed as extra usage or classified as a
"third-party harness"; the credit plan returns in a form that makes weekly use cost students money
by default.

---

## 8. Written confirmation: what to ask, and from whom

**From whom.** The legal page names the channel: "For questions about permitted authentication
methods for your use case, please contact sales" [S1]
(https://www.anthropic.com/contact-sales). Ask for the reply **by email from an Anthropic employee**,
ideally commercial or legal, that quotes the page wording it relies on. Don't treat a support
chatbot answer or a social-media reply as confirmation. Optional second route: Anthropic's
education team (claude.com/solutions/education) **[unverified relevance]**.

**Questions (numbered so the reply can answer each one):**

1. PageLamp is a free, open-source (Apache-2.0), local-only desktop app with no server of ours. At
   the student's explicit request it would invoke the student's own, separately installed,
   **unmodified** Claude Code CLI (`claude -p`), signed in only through Claude Code's own login
   with the student's Pro/Max plan. PageLamp never reads, stores or transmits credentials and never
   pays for or resells usage. Is this permitted under "Can customers offer Claude Code in their
   products?" and the "Nor does it prevent an end user…" sentence, and **not** "route requests
   through … plan credentials on behalf of their users"?
2. Is it acceptable to pass `--system-prompt`/`--append-system-prompt`, `--tools ""`,
   `--strict-mcp-config`, `--no-session-persistence`, `--json-schema`, and possibly `--restricted`
   (which ignores user settings files)? None of these changes the binary. Does any of them count as
   "restricting an authentication method" or change how usage is metered or classified (plan limits
   vs extra usage vs a future Agent SDK credit)?
3. Is **scheduled or background** generation (e.g. one weekly plan while the student isn't
   watching) "ordinary, individual usage", or must every run be started by the student?
4. How should the developer "agree to the Commercial Terms" when we make no API calls? (A Claude
   Console organization? Something else?) Does this also cover **branded builds** of the same open
   source code (a student association's build), or does each distributor accept separately?
5. May PageLamp start `claude auth login` for the student (it opens Anthropic's own browser flow),
   or should it only tell them to run it in a terminal?
6. When `--bare` becomes the `-p` default, will there be a supported way to keep subscription
   OAuth in `-p` (e.g. a flag)?
7. Is the wording "Your Claude subscription (PageLamp runs your installed Claude Code)" acceptable
   under the naming rules?
8. Does anything change for students on institution-managed Claude for Education accounts?

**Draft (English, for the contact form or email):**

> Subject: Permission question — local open-source student app invoking the user's own unmodified
> Claude Code (`claude -p`) with their own subscription
>
> Hello, I maintain PageLamp (https://github.com/Euswbnix/pagelamp), a free Apache-2.0 desktop app
> that keeps a university student's own course materials in a local database. There is no PageLamp
> server; we don't resell or pay for usage and never touch Claude credentials. For our next release
> we'd like students with a Claude Pro/Max plan to optionally generate study plans and weekly
> explanations by having PageLamp run their own separately installed, unmodified Claude Code in
> non-interactive mode (`claude -p`), with no tools, and with the student signed in only through
> Claude Code's own login. Your Claude Code legal page (as of September 2026) describes running the
> unmodified binary with the end user's own subscription under the Commercial Terms. Before we ship,
> we'd like written confirmation on the numbered questions below: [Q1–Q8]. We're happy to adjust
> the design or join a short call. Thank you.

---

## 9. Claude Desktop `.mcpb`: still the right v0.2 item?

**Facts.**
- Claude Desktop installs `.mcpb` by double-click, drag-and-drop, or Settings → Extensions →
  Advanced settings → Install Extension… It shows a review screen and a settings UI from
  `user_config`. Sensitive values go to the Keychain / Credential Manager [S35][S36].
- **"Desktop extension listings in the directory are deprecated, and the directory no longer
  accepts MCPB submissions."** The directory route for local servers is now a plugin, and a
  plugin's local servers don't run in chat [S35][S37]. Privately distributed extensions "need to
  install updated .mcpb files manually" [S36].
- The spec is manifest 0.3 (0.4 adds `uv`). A `binary` server type exists, along with
  `platform_overrides` and `${__dirname}`/`${HOME}` substitution [S34]. The CLI's last release is
  v2.1.2 (2025-12-04). MCPB moved to the MCP project in Nov 2025 [S34][S39].
- Platforms: one Anthropic page says Claude Desktop runs on macOS and Windows [S35]; the support
  article says extensions work on macOS, Windows and Linux [S36]. A Linux beta has existed since
  2026-06-30 [S41] [secondary]. **This conflicts — treat Linux as unverified.**
- Quality signals: `mcpb sign` output is rejected by Claude Desktop ("Invalid comment length"; open
  since 2026-06-12). Silent install no-ops on macOS in June 2026 and a Windows bug where extension
  tools didn't reach Chat were closed in July/August, fix not verified [S38] [community]. Unsigned
  bundles install with a warning [community].

**Assessment.** The need is real: Claude Desktop chat is the most common Claude surface for
students, and manual JSON editing (with the "quit Claude Desktop first" step PageLamp already warns
about) is the weakest part of onboarding. But the benefits are smaller than when v0.2 was planned:
no directory, manual updates, broken signing. A bundle also needs either a platform-specific copy of
`pagelamp` inside the bundle, which risks the app and extension versions drifting apart over one
DB, or a manifest `command` pointing at the installed app's binary. Whether Claude Desktop accepts a
command outside `${__dirname}` is **[unverified]**.

**Recommendation.** Keep it, but make it small and put it after signed releases and the updater.
Spike first: (a) does Claude Desktop accept an absolute `command` pointing into
`/Applications/PageLamp.app` (or `%LOCALAPPDATA%`)? If yes, have the desktop app **generate the
`.mcpb` at runtime** from the same `McpLaunch`, as `mcp_config.rs` already anticipates. That avoids
bundling a binary and avoids version drift. (b) If not, bundle `pagelamp` per platform and define
DB compatibility (older readers must tolerate additive columns). Ship unsigned until mcpb#278 is
fixed. Keep the JSON snippet as a fallback.

---

## 10. Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Policy reversal, or enforcement "without prior notice" [S1] | Medium | High | Written confirmation; feature flag; fallbacks (reverse MCP, BYOK, Codex); release-time legal-page check |
| Harness classifier re-bills or refuses PageLamp runs [S29] | Medium | High (students charged) | Spike both prompt modes; ask Q2; detect `credits_required`; tell students to keep usage credits off |
| Credit plan returns: `-p` moves to a $20/month Pro pool [S19] | Medium | Medium | Cheap models, small single-turn runs, budget caps, visible usage |
| `--bare` becomes the `-p` default and OAuth stops working [S4] | Medium | High (path breaks) | Minimum/tested versions; clear "not logged in" handling; ask Q6 |
| CLI churn: fields and flags change [S10][S31] | High | Low–Medium | Defensive parsing, `capabilities`, smoke test per release |
| Student config leaks in (hooks, CLAUDE.md, MCP, Fable default) | Medium | Medium | Isolation flags + env, empty cwd, always `--model` |
| Course text persisted by Claude Code or used for training [S12] | Certain without flags | Medium | `--no-session-persistence`; disclosure; per-course policy engine |
| Minors using consumer Claude (18+ required) [S16] | Low–Medium | Medium | Don't target them; state the requirement |
| Academic-integrity misuse [S18] | Low | High | No assignment fetching; per-course AI policy; tutoring prompts |
| Planted `claude` binary on PATH | Low | High | Signature check, shown path, user confirmation |
| GUI apps can't find `claude` (PATH) | High | Low | Known paths + login-shell probe + manual picker |
| Subscription recognition bugs in Claude Code [S33] | Low | Low | Show Claude Code's own message; link Anthropic support |

---

## 11. Suggested edits to `docs/design/v0.3-model-access.md` (for the leader)

- Mode B status: change from "gray area" to "allowed by the Claude Code legal page since Sept 2026
  for the unmodified binary with the user's own subscription under the Commercial Terms; open
  questions: 'on behalf of users', automation, metering and classification. Written confirmation
  still required (owner rule)." Add the timeline in §2.
- Replace "`claude -p`, `--system-prompt`, `--output-format json`" with the §6.2 recipe: add
  `--json-schema` as the native-strict tier, `--no-session-persistence`, explicit `--model`, and
  isolation. **Don't bundle Claude Code** (unlike Codex).
- Rejected list: keep "Agent SDK with claude.ai login" (the SDK note is unchanged, and it needs a
  sidecar). Add "`--bare`/`CLAUDE_CODE_SIMPLE`" (it disables OAuth and would restrict an auth
  method) and "PageLamp-triggered `claude auth login`" (ask first).
- §6 open items: add `--restricted` + OAuth, `--json-schema` with no tools, how a replaced system
  prompt is metered, the dropped `apiKeySource`, the Opus 5.5 default on Pro, and `-p` behaviour
  when extra usage is enabled.
- Roadmap: `.mcpb` becomes a small task after the updater (§9). Add the skills-only plugin and the
  Desktop scheduled-task recipe as cheap v0.3 items.

---

## 12. Spike checklist (owner-run, about half a day, own subscription, no API key)

Run on a `git archive` snapshot or a scratch folder, never the shared tree (memory rule). Use an
**arm64** native Claude Code (`~/.local/bin/claude`), not the x86_64 `/usr/local/bin/claude`.

1. `claude --version`; `claude auth status | jq 'keys'` (field names only); `claude auth status --text`.
2. Run §6.2 with `--append-system-prompt-file`, then with `--system-prompt-file`. After each, check
   claude.ai Settings → Usage: plan usage should move and **no usage-credit charge** should appear.
   Record `total_cost_usd`, `modelUsage` and any `rate_limit_event`.
3. `--json-schema` with `--tools ""`: is `structured_output` present? Try an invalid-output case.
4. Add `--restricted`: does it still use the subscription? Does it still need anything from user
   settings?
5. Isolation: put a harmless hook in `~/.claude/settings.json`, an MCP server in `~/.claude.json`
   and a `CLAUDE.md` in `~`. Confirm none load (`system/init` → `mcp_servers` empty; hook didn't run).
6. `ls -lt ~/.claude/projects` before and after: no new transcript with `--no-session-persistence`.
7. Logged out without logging out: `CLAUDE_CONFIG_DIR=$(mktemp -d) claude -p …` (a different config
   dir reads a different Keychain entry [S6]). Record exit code and result text.
8. `ANTHROPIC_API_KEY=invalid` in the child env: confirm it takes precedence, and see what
   `auth status` reports (this shows why PageLamp displays the effective method).
9. Repeat 1–3 on Windows (native `claude.exe`, no Git Bash) and, if in scope, Linux.
10. Note latency and tokens for a realistic 5-course weekly prompt (policy-filtered).

---

## 13. Questions only the owner can answer

1. Who is the "customer" for the Commercial Terms: you personally or an entity? Which email and
   name go on the request to Anthropic?
2. If Anthropic hasn't replied by v0.3 feature freeze, do we ship v0.3 without mode B (BYOK +
   Codex + reverse MCP), or wait?
3. Should PageLamp ever run Claude in the background on a schedule, or only when the student clicks?
   This changes Q3 of §8 and the risk profile.
4. System prompt: accept Claude Code's default prompt with ours appended (more tokens, likely
   safer for metering), or replace it (cleaner)? Decide after spike step 2.
5. Minimum Claude Code version policy: require a recent version (e.g. ≥ 2.1.259) and tell students
   to update?
6. `.mcpb`: still wanted given no directory, manual updates and broken signing? Or keep snippets and
   add the skills-only plugin plus the Desktop routine recipe first?
7. Publish a PageLamp plugin via a GitHub marketplace, and later submit it to the directory?
8. Do target schools (UofT first) give students Claude for Education accounts? That affects
   whether mode B even applies to them.
9. Should the Anthropic confirmation explicitly name planned branded builds (UTMCSSA)?
10. Test machines: do you have Windows (and Linux) available for spike step 9?

---

## Sources (all accessed 2026-09-27)

- [S1] Claude Code docs, Legal and compliance — https://code.claude.com/docs/en/legal-and-compliance
- [S2] Wayback Machine snapshots of [S1]: https://web.archive.org/web/20260201064220/https://code.claude.com/docs/en/legal-and-compliance ,
  …/20260219142355/…, …/20260321004409/…, …/20260421200239/…, …/20260816100738/…, …/20260903205410/…, …/20260923195120/…
  (same path; compared for the phrases quoted in §2)
- [S3] Agent SDK overview — https://code.claude.com/docs/en/agent-sdk/overview
- [S4] Run Claude Code programmatically (headless) — https://code.claude.com/docs/en/headless
- [S5] CLI reference — https://code.claude.com/docs/en/cli-reference
- [S6] Authentication — https://code.claude.com/docs/en/authentication
- [S7] Environment variables — https://code.claude.com/docs/en/env-vars
- [S8] Setup — https://code.claude.com/docs/en/setup
- [S9] Troubleshoot installation — https://code.claude.com/docs/en/troubleshoot-install
- [S10] Changelog — https://code.claude.com/docs/en/changelog (2.1.205 = 2026-07-08, 2.1.248 = 08-27, 2.1.259 = 09-02, 2.1.280 = 09-22, 2.1.283 = 09-25)
- [S11] Model configuration — https://code.claude.com/docs/en/model-config
- [S12] Data usage — https://code.claude.com/docs/en/data-usage
- [S13] Agent SDK TypeScript reference (SDKResultMessage, SDKRateLimitEvent, ApiKeySource) — https://code.claude.com/docs/en/agent-sdk/typescript
- [S14] What's new, week 35 (restricted mode) — https://code.claude.com/docs/en/whats-new/2026-w35
- [S15] Schedule recurring tasks in Claude Code Desktop — https://code.claude.com/docs/en/desktop-scheduled-tasks
- [S16] Consumer Terms (effective 2025-10-08) — https://www.anthropic.com/legal/consumer-terms
- [S17] Commercial Terms (effective 2025-06-17) — https://www.anthropic.com/legal/commercial-terms
- [S18] Usage Policy (effective 2025-09-15) — https://www.anthropic.com/legal/aup
- [S19] Use the Claude Agent SDK with your Claude plan (update 2026-06-15: paused) — https://support.claude.com/en/articles/15036540-use-the-claude-agent-sdk-with-your-claude-plan
- [S20] Using Claude Code with your Pro or Max plan (updated 2026-08-19) — https://support.claude.com/en/articles/11145838-using-claude-code-with-your-pro-or-max-plan
- [S21] What is the Pro plan? — https://support.claude.com/en/articles/8325606-what-is-the-pro-plan
- [S22] Plans & pricing — https://claude.com/pricing
- [S23] Claude Code LICENSE.md — https://github.com/anthropics/claude-code/blob/main/LICENSE.md
- [S24] X posts, 2026-05-13 (seen only as search-result snippets; wording [unverified]): https://x.com/ClaudeDevs/status/2054610152817619388 ,
  https://x.com/alexalbert__/status/2054613082589298899 , https://x.com/lydiahallie/status/2054642875644932549
- [S25] VentureBeat, 2026-05-13 [secondary] — https://venturebeat.com/technology/anthropic-reinstates-openclaw-and-third-party-agent-usage-on-claude-subscriptions-with-a-catch
- [S26] TechCrunch, 2026-04-04 [secondary] — https://techcrunch.com/2026/04/04/anthropic-says-claude-code-subscribers-will-need-to-pay-extra-for-openclaw-support/
- [S27] The Register, 2026-04-06 (via search summary, incl. the 2026-02-20 quote) [secondary] — https://www.theregister.com/2026/04/06/anthropic_closes_door_on_subscription/
- [S28] VentureBeat, Jan 2026 (via search summary) [secondary] — https://venturebeat.com/technology/anthropic-cracks-down-on-unauthorized-claude-usage-by-third-party-harnesses
- [S29] MindStudio, 2026-05-03 [secondary] — https://www.mindstudio.ai/blog/anthropic-confirms-claude-code-scanning-git-commits-openclaw-hermes
- [S30] Zed blog, 2026-05-14 (updated 2026-06-16) [secondary] — https://zed.dev/blog/anthropic-subscription-changes
- [S31] mnemo issue #441, 2026-09-22 [community] — https://github.com/xyrlan/mnemo/issues/441
- [S32] agents-live issue #474, 2026-09-06 [community] — https://github.com/johnshew/agents-live/issues/474
- [S33] anthropics/claude-code #94856 (open, 2026-09-16) and #46847 — https://github.com/anthropics/claude-code/issues/94856 , https://github.com/anthropics/claude-code/issues/46847
- [S34] MCPB repo, MANIFEST.md, CLI.md, releases API — https://github.com/modelcontextprotocol/mcpb , https://github.com/modelcontextprotocol/mcpb/blob/main/MANIFEST.md
- [S35] Build a desktop extension with MCPB — https://claude.com/docs/connectors/building/mcpb
- [S36] Getting started with local MCP servers on Claude Desktop — https://support.claude.com/en/articles/10949351-getting-started-with-local-mcp-servers-on-claude-desktop
- [S37] Plugins overview; Decide what to include in your plugin — https://claude.com/docs/plugins/overview , https://claude.com/docs/connectors/building/what-to-build
- [S38] mcpb #278 (open) and claude-code #68240, #68484, #70397 [community] — https://github.com/modelcontextprotocol/mcpb/issues/278 , https://github.com/anthropics/claude-code/issues/68240 , https://github.com/anthropics/claude-code/issues/68484 , https://github.com/anthropics/claude-code/issues/70397
- [S39] MCP blog, adopting MCPB (2025-11-20/21) — https://blog.modelcontextprotocol.io/posts/2025-11-20-adopting-mcpb/
- [S40] MCP docs, connect local servers (2026-07-28 spec docs) — https://modelcontextprotocol.io/docs/2026-07-28/develop/connect-local-servers
- [S41] Claude Desktop on Linux (beta) — https://code.claude.com/docs/en/desktop-linux ; OMG! Ubuntu, 2026-07 [secondary] — https://www.omgubuntu.co.uk/2026/07/claude-desktop-linux-beta
- [S42] Use Claude for Education at your university (search snippet only) [unverified] — https://support.claude.com/en/articles/11139144-use-claude-for-education-at-your-university
- [S43] `claude auth status` field names from GitHub issue search results [community], e.g. https://github.com/Leonxlnx/tastecode/issues/1253
- [S44] claude-mem issue #1826 (2026-04-15) [community] — https://github.com/thedotmack/claude-mem/issues/1826
