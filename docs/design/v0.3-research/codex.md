# Research: official OpenAI Codex as PageLamp's ChatGPT-subscription path (v0.3)

Researcher: subagent, 2026-09-27. Every external fact has a source and access date (all accessed 2026-09-27 unless noted).
**[unverified]** marks things I could not confirm from a primary source. The repo was read, not changed. I ran the local
arm64 `codex --version` / `codex exec --help` only (0.144.1). No Codex credentials were read.

---

## 0. TL;DR (recommendation)

1. **Use `codex exec` as the only production path in v0.3.0.** It is labelled **Stable**. Run it from Rust
   (`tokio::process`) behind the `pagelamp-app` facade, so Tauri and the Swift shell both get it. Flags:
   `--json --output-schema <file> -o <file> --ephemeral --skip-git-repo-check --strict-config`. Send the prompt on
   **stdin** (`codex exec -`), never in argv. Use read-only sandbox, approval `never`, all tools off. Point
   `CODEX_HOME` at a **PageLamp-owned folder**.
2. **Don't bundle Codex in the installer.** Download the pinned official GitHub release when the student turns the
   option on, then check its SHA-256 (and the OpenAI code signature on macOS). The unpacked binary is ≈250 MB, a
   new stable release ships every week, and the Linux package includes an LGPL `bwrap` helper. The terms also have
   a "distribute" clause. Downloading at runtime avoids all four problems.
3. **Let Codex handle sign-in entirely.** PageLamp spawns `codex login` (browser) or `codex login --device-auth`, and
   uses `codex login status` / `codex logout`. Store credentials in the OS keychain (`cli_auth_credentials_store =
   "auto"`). PageLamp **never** reads `auth.json` or the keychain item. It never uses app-server `chatgptAuthTokens`
   or `codex login --with-access-token`.
4. **The policy engine stays in front.** PageLamp builds the prompt in Rust from `AiMaterialsState`-filtered text.
   v0.3.0 connects **no MCP servers** inside Codex runs. A later tutor mode can connect PageLamp's own MCP server
   read-only, since that server already enforces the per-course policy.
5. **`codex app-server`: prototype only for now.** It is still **Experimental** and "not supported for production
   workloads". Put exec and app-server behind one `CodexBackend` trait. Switch when OpenAI marks app-server Stable.
   Two signs point that way: the Python SDK is "stable" and runs on app-server, and `codex exec` itself now runs an
   in-process app-server.
6. **Terms risk is medium-low, not zero.** OpenAI's docs invite embedding the harness and list `codex exec` for
   Plus and higher. But nothing explicitly says a *third-party consumer app* may drive a user's ChatGPT-signed-in
   Codex. **Ask OpenAI for written confirmation.** It is cheap, but unlike Anthropic's case it should not block.
7. **Free/Go students are not covered by any doc.** The pricing page lists Free/Go Codex only "in the desktop app,
   subject to rollout". The owner should test Free and Go accounts themselves (checklist in §11).

---

## Observed 2026-09-28 (addendum)

The leader ran these on the owner's Mac with the owner's permission. No credentials and no course
content were read or recorded.

1. **A pinned Codex can stop working when OpenAI changes the account's default model.** The
   standalone Codex CLI 0.144.1 (`~/.local/bin/codex`) failed every run with HTTP 400: "The
   'gpt-6-astra' model requires a newer version of Codex. Please upgrade to the latest app or CLI
   and try again." The account's default model had moved on. The Codex CLI bundled in the ChatGPT
   desktop app (0.158.0-alpha.2.1) worked. For mode A (a pinned Codex downloaded on demand) this
   means a pin can break without any retirement notice. Design response (design §2.3, plan M2):
   - always pass an explicit `-m` from a short supported list in `codex-pin.toml`, never the
     account default (whether that always avoids the error is **[unverified]**);
   - map this 400 to its own `ModelErrorKind` (`RuntimeOutdated`) with a student-facing message:
     "PageLamp needs a newer Codex; updating…" when the pin already names a newer version, else
     "Update PageLamp";
   - refresh the pin on a schedule (at least monthly) and out of cycle on this error;
   - a default-model canary in the owner's A7 test (checklist item 12 in §11).
2. **MCP output size.** With PageLamp's MCP server added to Codex, one question ("where is each of
   my courses this week?", 13 courses) made Codex call `sync_status`, `list_courses` and
   `course_overview` ×7, and used about 113k input tokens (89k cached) and 1.3k output tokens.
   Tool outputs are re-read on every turn, so PageLamp keeps `course_overview` and `list_courses`
   compact, and any budget for a run that attaches the MCP server must count tool-output tokens
   (design §6).
3. **`codex exec` waits on an open stdin.** Spawned without a prompt on stdin and with stdin left
   open, it prints "Reading additional input from stdin..." and waits. PageLamp closes stdin after
   writing the prompt, and gives every other Codex command a null stdin (`</dev/null`).

## Checked 2026-09-29 (addendum): the 0.158.0 release assets

The backend downloaded the 0.158.0 single-binary assets (with the owner's permission; deleted
afterwards) to settle the signature rule of the managed runtime (plan M2, `codex-pin.toml`):
- **macOS arm64 and x64:** signed "Developer ID Application: OpenAI OpCo, LLC (2DC432GLL2)", with
  the hardened runtime and a secure timestamp. `codesign --verify --strict` passes with the
  requirement `anchor apple generic and certificate leaf[subject.OU] = "2DC432GLL2"`, and fails
  with any other team. This matches the ChatGPT.app bundled Codex the leader checked.
- **Windows x64 and arm64 (`codex.exe`):** Authenticode-signed; the signer is "OpenAI OpCo, LLC",
  issued by "Microsoft ID Verified CS AOC CA 03" (chain: Microsoft ID Verified Code Signing PCA
  2021 → Microsoft Identity Verification Root Certificate Authority 2020). This answers the §6
  **[unverified]** "Authenticode signing" for this version: signed. Whether Smart App Control
  accepts it is still A7's check.
- **Linux:** unsigned binaries; the pinned SHA-256 is the check (sigstore bundles exist for the
  `codex-package-*` archives only).
- Sizes: download 68–80 MB; unpacked 240 MB (macOS arm64), 258 MB (macOS x64), 324 MB (Windows
  x64). A pure-Rust zstd decoder (`ruzstd`) unpacks them identically to `unzstd`.
- **Config keys (0.158.0 `codex-rs/core/config.schema.json`).** The root, `[features]`, `[tools]`
  and the other tables reject unknown keys, so `--strict-config` fails on any typo. Differences
  from design §2.3's list: there is no `tools.view_image` (it is `features.view_image`);
  `features.code_mode` takes a plain boolean; `[tools]` holds only `update_plan`,
  `experimental_request_user_input` and `web_search` (tables with `enabled`). 0.158.0 has more
  tools PageLamp turns off: `unified_exec`, `js_repl`, `browser_use`, `computer_use`,
  `image_generation`, `plugins`, `collab`, `codex_hooks` / `plugin_hooks`,
  `standalone_web_search`. The generated config (`pagelamp-llm` `codex/home.rs`) was checked
  key by key against that schema.
- **Sign-in output (0.158.0 source).** Browser: stderr "Starting local login server on
  http://localhost:1455. If your browser did not open, navigate to this URL to authenticate:"
  then the URL. Device code: stdout "1. Open this link…" + URL, "2. Enter this one-time code
  (expires in 15 minutes)" + code (ANSI-coloured). `login status` (stderr): "Logged in using
  ChatGPT", "Logged in using an API key - <masked>", and other "Logged in using …" modes
  (access token, personal access token, Bedrock, workload identity).
- The official installer uses `codex-package-<target>.tar.gz` (`bin/codex`,
  `bin/codex-code-mode-host`, `codex-path/rg`, and on Linux `codex-resources/bwrap`); PageLamp
  uses the bare binary, as tools are off. Whether `-s read-only` needs `bwrap` on Linux is checked
  by the leader's contract test (plan M2).

---

## 1. Current state: versions and release cadence

| Fact | Value | Source |
|---|---|---|
| Latest stable | **0.157.1**, published 2026-09-26T01:02Z (tag `rust-v0.157.1`) | GitHub API `repos/openai/codex/releases/latest` |
| Previous stables | 0.157.0 (09-25, added GPT-6 Sol/Luna), 0.156.1 (09-23), 0.156.0 (09-22), 0.155.1 (09-18), 0.155.0 (09-17), 0.154.0 (09-09) | same, releases list |
| Pre-releases | Several alphas per day (e.g. 0.159.0-alpha.4 … alpha.9 on 09-26/27) | https://github.com/openai/codex/releases |
| Cadence | 7–15 stable releases/month (2026: Jan 14, Feb 13, Mar 10, Apr 8, May 7, Jun 11, Jul 11, Aug 8, Sep 15). Still 0.x, no 1.0 | GitHub API, counted |
| End of 2025 | 0.77.0 (2025-12-21). So ~80 minor versions in 9 months | GitHub API |
| License | **Apache-2.0**. NOTICE: "Copyright 2025 OpenAI", includes Ratatui (MIT) | https://github.com/openai/codex (LICENSE, NOTICE) |
| Locally installed here | 0.144.1 (arm64, `~/.local/bin/codex`), signed "Developer ID Application: OpenAI OpCo, LLC (2DC432GLL2)", `minos 11.0`, 248 MB | local `codesign`/`otool` |

Docs have moved: `developers.openai.com/codex/*` now 308-redirects to `learn.chatgpt.com/docs/*` (observed
2026-09-27). Markdown versions: append `.md`. Index: https://learn.chatgpt.com/llms.txt

Maturity labels come from https://learn.chatgpt.com/docs/feature-maturity. **Stable** means "Safe for production use;
removals typically go through a deprecation process". **Experimental** means "Unstable and OpenAI may remove or
change it."

## 2. `codex exec` (non-interactive)

Sources: https://learn.chatgpt.com/docs/non-interactive-mode ,
https://learn.chatgpt.com/docs/developer-commands?surface=cli , local `codex exec --help` (0.144.1), source at tag
`rust-v0.157.1` (`codex-rs/exec/src/*`).

- **Maturity: Stable** (alias `codex e`). `codex app-server` is **Experimental**.
- **Output.** Default: progress goes to stderr, only the final message to stdout. `--json` makes stdout JSONL with
  events `thread.started`, `turn.started`, `turn.completed` (with `usage`: input / cached / cache_write / output /
  reasoning tokens), `turn.failed`, `item.started|updated|completed`, `error`. Item types: `agent_message`,
  `reasoning`, `command_execution`, `file_change`, `mcp_tool_call`, `collab_tool_call`, `web_search`, `todo_list`,
  `error` (from `exec_events.rs`).
- **Structured output.** `--output-schema <file>` (JSON Schema for the final response). `-o/--output-last-message
  <file>` writes the final message. `exec resume` also accepts `--output-schema` (release note #23123).
- **Errors are plain text in exec.** `ThreadErrorEvent { message: String }` has no structured code. By contrast,
  app-server errors carry `codexErrorInfo` = `UsageLimitExceeded`, `Unauthorized`, `ContextWindowExceeded`, …
  (https://learn.chatgpt.com/docs/app-server#errors).
- **Sandbox and approvals.** `-s read-only|workspace-write|danger-full-access`. exec defaults to a read-only sandbox.
  `-a on-request|never`, and exec forces `never` in headless mode (source). `--full-auto` is deprecated.
  `approval_policy="untrusted"` has been unsupported since 0.149.0 (https://help.openai.com/en/articles/11369540).
  In exec, command and file-change approval requests are **rejected** and MCP elicitations are **auto-cancelled**
  (source, `handle_server_request`).
- **Working directory and isolation.**
  - `-C/--cd`, `--add-dir`, `--skip-git-repo-check` (a Git repo is otherwise required).
  - `--ephemeral`: no rollout files.
  - `--ignore-user-config`: skips `$CODEX_HOME/config.toml`. Per the help text, "auth still uses CODEX_HOME".
  - `--ignore-rules`, `--strict-config` (error on unknown keys), `-c key=value` overrides. The overrides are a
    separate CLI layer, so they still apply with `--ignore-user-config` (source, `config/src/loader`).
  - Managed or cloud requirements still apply.
- **Prompt input.** A positional prompt, or stdin when it is omitted or `-` is used. Piped stdin plus a prompt
  argument is appended as `<stdin>`.
- **Auth in exec.** Reuses the saved CLI login. `CODEX_API_KEY` switches to API billing for that run. PageLamp must
  **scrub the environment** so the subscription path cannot silently turn into API billing.
- **Relevant config keys** (https://learn.chatgpt.com/docs/config-file/config-reference):
  - Tools and features: `features.shell_tool` ("stable; on by default"), `web_search = "disabled"` (default is
    `"cached"`), `tools.view_image`, `features.apps`, `features.multi_agent`, `features.code_mode.enabled`,
    `features.hooks`, `features.memories` (off by default).
  - Instructions: `developer_instructions`, and `model_instructions_file` (a "Replacement for built-in
    instructions").
  - Housekeeping: `project_doc_max_bytes`, `history.persistence = "none"`, `check_for_update_on_startup`,
    `analytics.enabled`, `feedback.enabled`, `cli_auth_credentials_store`, `[windows] sandbox`.
- **Global AGENTS.md leak.** Codex always reads `$CODEX_HOME/AGENTS.md` (or `AGENTS.override.md`) as global guidance
  (https://learn.chatgpt.com/docs/agent-configuration/agents-md). With the student's default `~/.codex`, their
  personal coding instructions would be injected into PageLamp prompts. This is the main reason for a **dedicated
  CODEX_HOME**.

### MCP client support (Codex calling PageLamp's MCP server)
Source: https://learn.chatgpt.com/docs/extend/mcp and the config reference.
- STDIO and Streamable HTTP servers are supported. Codex reads the MCP **`instructions`** field. The first 512
  characters should stand on their own, which helps PageLamp's "course text is data" rule.
- Per-server keys: `command`, `args`, `env`, `env_vars`, `cwd`, `startup_timeout_sec` (default 10), `tool_timeout_sec`
  (60), `enabled`, `required` (exec exits if the server fails), `enabled_tools` / `disabled_tools`,
  `default_tools_approval_mode = auto|prompt|writes|approve`, `tools.<t>.approval_mode`, `tools.<t>.output_token_limit`.
- All of these can be passed as `-c mcp_servers.pagelamp.*` for one run, with no user config.
- Because exec cancels elicitations, PageLamp tools must never need approval. Allow-list only the read tools, mark
  them `readOnlyHint`, and leave out `save_study_plan` (PageLamp saves plans from the structured output instead).
- The v0.1 "Connect your AI app" snippet (`[mcp_servers.pagelamp]` in `~/.codex/config.toml`, shared with the ChatGPT
  desktop app) is still correct per the same page.

## 3. `codex app-server` (stability)

Source: https://learn.chatgpt.com/docs/app-server , https://learn.chatgpt.com/docs/mcp-server
- **Experimental**. The docs say "The app-server command and WebSocket transport are experimental and aren't
  supported for production workloads." It uses JSON-RPC over stdio (JSONL).
- An `experimentalApi` capability gates the unstable fields. `generate-ts` / `generate-json-schema` produce
  version-specific schemas.
- Useful features exec lacks:
  - Account methods: `account/read` (auth mode + `planType`, plus email), `account/rateLimits/read` (used %, window,
    `resetsAt`), `account/usage/read`.
  - Structured error codes.
  - Per-turn `outputSchema`, `turn/interrupt`, and threads for follow-up questions.
- Auth modes: `apiKey`, `chatgpt` (browser; Codex owns tokens), `chatgptDeviceCode`, and **experimental
  `chatgptAuthTokens`** ("for host apps that already own the user's ChatGPT auth lifecycle"). PageLamp must not use
  `chatgptAuthTokens`.
- `clientInfo.name` identifies the integration in OpenAI's Compliance Logs. The docs ask enterprise integrations to
  contact OpenAI to be added to a known-clients list.
- **`codex mcp-server` was removed in 0.154.0 (2026-09-09).** Release note #42993: "The deprecated `codex mcp-server`
  entry point is no longer available". App-server is its replacement.
- Signs of convergence: the Python SDK (`pip install openai-codex`) "controls the local Codex app-server" and "is
  available as a stable release" with a "pinned Codex CLI runtime" (https://learn.chatgpt.com/docs/codex-sdk).
  At 0.157.1, `codex exec` is implemented on `InProcessAppServerClient` (source).

## 4. "Sign in with ChatGPT": the flow and where credentials live

Source: https://learn.chatgpt.com/docs/auth ; source `codex-rs/login/src/auth/storage.rs`, `cli/src/login.rs`.
- **Browser flow.** `codex login` opens the browser. The browser "returns your credentials to Codex" through a
  localhost callback (default port **1455**).
- **Device code.** `codex login --device-auth` (beta). The student must first enable device-code login in their
  ChatGPT security settings. Codex falls back to the browser flow if it is not enabled.
- **API key.** `printenv OPENAI_API_KEY | codex login --with-api-key`.
- **Check and sign out.** `codex login status` exits 0 when signed in and prints "Logged in using ChatGPT", or
  "Logged in using an API key - <masked>" on **stderr**. PageLamp should branch on the exit code and the
  ChatGPT/API-key phrase, and never log that output. `codex logout` revokes and clears.
- **Storage.** `cli_auth_credentials_store = file | keyring | auto | ephemeral`. The **default is `file`**:
  `$CODEX_HOME/auth.json`, mode 0600, which the docs call "a plaintext file". The keychain service is **"Codex
  Auth"**, keyed by SHA-256 of the canonical `CODEX_HOME`, so a dedicated `CODEX_HOME` gets its own entry. The CLI
  and IDE extension share cached logins. Tokens refresh automatically.
- **Enterprise controls.** Admins can force `forced_login_method`, `forced_chatgpt_workspace_id`, and the credential
  store through managed requirements. A student in a managed Edu workspace may therefore be blocked or logged out.
- **What PageLamp must never do:**
  - read `auth.json` or the "Codex Auth" keychain item;
  - include `codex-home/` in diagnostic reports, logs, or exports (add a test);
  - use `--with-access-token`, `CODEX_ACCESS_TOKEN`, or app-server `chatgptAuthTokens`;
  - copy `auth.json` between machines. The docs describe this for headless machines, but PageLamp must never
    automate it.

## 5. Plans that include Codex, and their limits

Sources: https://learn.chatgpt.com/docs/pricing , https://help.openai.com/en/articles/11369540 ,
https://help.openai.com/en/articles/20001147 , https://developers.openai.com/community/students

- **Feature matrix.** "Codex SDK, `codex exec`, and scriptable workflows" and "Codex CLI" are listed for **Plus, Pro,
  Business, Enterprise/Education, API key**. Free and Go are not in the matrix.
- **Free ($0) and Go ($8/mo).** The pricing cards say only "GPT-6 Luna at Standard speed in the desktop app, subject
  to rollout". The Help Center says "Codex is included across ChatGPT plans, including Free and Go" and lists
  "Codex CLI" among the clients. Secondary sources say Free/Go get no CLI (morphllm.com/codex-pricing).
  → **CLI/exec on Free/Go is [unverified]. Test it.**
- **Plus ($20).** CLI, IDE, web, and iOS; GPT-6 Sol and Luna; extra usage with ChatGPT credits.
- **Pro.** $100 (5x Plus) or $200 (20x Plus).
- **Business.** $20/user/month billed annually ($25 monthly), minimum 2 users.
- **Edu/Enterprise.** Per-seat limits like Plus unless flexible pricing. Admins control RBAC, local vs. cloud Codex,
  and **Compliance API logs that cover local CLI usage**. If a student signs in with a university Edu workspace,
  prompts built from course material become visible to that workspace's admins. PageLamp must disclose this.
- **Rough local messages per 5 hours on Plus** (estimates, not fixed): GPT-6 Luna 350–3,000; GPT-6 Sol 15–150;
  GPT-6 Astra 5–45; GPT-5.6 Luna 250–2,000. Weekly limits may also apply. Usage is token/credit-based. Every MCP
  server "adds more context … and uses more of your limit". Hitting a limit mid-turn lets the turn finish.
- **Model churn.** GPT-5.4 and 5.4 mini left ChatGPT-sign-in Codex on 2026-08-31. **GPT-5.5 retires 2026-10-14** on
  all plans (https://learn.chatgpt.com/docs/models). 0.157.0 was needed to add GPT-6 Sol/Luna, which was even
  hotfixed into 0.156.0 (#47405). PageLamp must never hard-code one model and must move its Codex pin forward
  regularly.
- **Codex for Students.** Verified university students in the **US and Canada** get **$100 = 2,500 ChatGPT credits**
  (not API credits), valid 12 months. It needs a Free, Go, Plus, or Pro plan, and verification goes through SheerID.
  The credits "extend Codex usage beyond the limits included in your plan". Whether credits unlock the CLI for a
  Free-plan student is **[unverified]**. UofT students are eligible if they live in Canada.
- **Training.** Plus and Pro conversations "may be used to improve models unless you turn off training". Business,
  Enterprise, and Edu: not by default. This belongs in PageLamp's Canvas §2E disclosure.

## 6. Platform support

From the release assets of `rust-v0.157.1` (GitHub API) and https://learn.chatgpt.com/docs/sandboxing ,
https://learn.chatgpt.com/docs/agent-approvals-security , https://learn.chatgpt.com/docs/windows/windows-sandbox

| Target | Asset | zst / tar.gz | Notes |
|---|---|---|---|
| macOS arm64 | `codex-aarch64-apple-darwin.*` | 64 / 90 MB | Signed by OpenAI Developer ID (seen locally on 0.144.1), minos 11.0; Seatbelt sandbox |
| macOS x64 | `codex-x86_64-apple-darwin.*` | 70 / 98 MB | same |
| Windows x64 / arm64 | `codex-{x86_64,aarch64}-pc-windows-msvc.exe.*` | 75 / 69 MB zst | Native sandbox: `elevated` (admin setup) or `unelevated` fallback. Authenticode signing **[unverified]** |
| Linux x64 / arm64 | `codex-{x86_64,aarch64}-unknown-linux-musl.*` | 73 / 68 MB zst | Static musl. Needs `bwrap` on PATH, otherwise a bundled helper (needs unprivileged user namespaces; Ubuntu 24.04 AppArmor caveat) |

- Also published: `codex-package-*` tarballs (122–148 MB), npm tgz, PyPI wheels, `install.sh`/`install.ps1`,
  `codex-package_SHA256SUMS`, and sigstore bundles for Linux. Every GitHub asset has a `digest: sha256:…`.
- WSL1 has been unsupported since 0.115. WSL2 uses the Linux sandbox.
- Official install methods: `curl -fsSL https://chatgpt.com/codex/install.sh | sh`, PowerShell `install.ps1`,
  `npm i -g @openai/codex`, `brew install --cask codex` (https://learn.chatgpt.com/docs/codex/cli). There is also
  `codex update`. The standalone installer puts binaries in `~/.local/bin` and package metadata under
  `CODEX_HOME/packages/standalone` (https://learn.chatgpt.com/docs/config-file/environment-variables).

## 7. Bundling and redistribution, pinning and updating

- **License.** The Codex CLI is Apache-2.0, so redistributing the *unmodified* binary is allowed if LICENSE and
  NOTICE go with it. Apache §6 grants no trademark rights.
- **Service Terms §10 "Licensed Materials"** says "You may not modify, redistribute, or sublicense the Licensed
  Materials", but open-source license provisions "will expressly override" for those components
  (https://openai.com/policies/service-terms/, updated 2026-09-21).
- **Terms of Use** forbid "Modify, copy, lease, sell or distribute any of our Services"
  (https://openai.com/policies/row-terms-of-use/, effective 2026-01-01). The Apache license most likely governs the
  CLI binary, but the overlap is **ambiguous**. Having the student's machine download from OpenAI's own release
  page avoids the question.
- **LGPL helper.** The Linux package ships a bundled `bwrap` (`codex-rs/vendor/bubblewrap`, **GNU LGPL v2**). A
  redistributor takes on LGPL duties, and PageLamp's policy bans GPL-family dependencies.
- **Size and cadence.** ≈250 MB unpacked per architecture (a universal macOS build would be about double). Weekly
  stable releases.
- **Precedent for pinning.** OpenAI's own Python SDK "include[s] a pinned Codex CLI runtime dependency". The
  TypeScript SDK spawns `codex exec --experimental-json` from the `@openai/codex` package. Pinning is the pattern
  OpenAI uses itself.
- **Recommended pinning/update scheme:**
  - Each PageLamp release carries `codex_pin = { version, per-target sha256 }`. The per-target hashes come from the
    GitHub asset `digest` and `codex-package_SHA256SUMS`.
  - Download on the explicit "Use my ChatGPT plan" action (show the size) into `<data_dir>/runtimes/codex/<ver>/`.
  - Verify the SHA-256. On macOS also verify the code signature with Team ID `2DC432GLL2`. On Linux, optionally
    verify the sigstore bundle.
  - Keep N-1 for rollback.
  - `check_for_update_on_startup = false` in the dedicated config. PageLamp's own updater (Tauri, signed) moves the
    pin forward.
  - Bump the pin at least monthly, and always before a model retirement date.
  - Optionally let an advanced user use an installed `codex` if `--version` falls in the tested range.
- **CI contract test** (leader): download the pinned Linux x64 binary, then:
  - run `codex exec --help`;
  - run `--strict-config` against PageLamp's generated `config.toml`;
  - run the JSONL tripwire parser on recorded fixtures;
  - optionally run a live exec over a **PageLamp-owned API key with a small budget** (never a ChatGPT login in CI),
    or `--oss` with a tiny local model **[output-schema with --oss unverified]**.

## 8. OpenAI terms relevant to a third-party app launching Codex for the user

| Clause | Text (short quote) | Source | Effect on PageLamp |
|---|---|---|---|
| Programmatic extraction | may not "Automatically or programmatically extract data or Output" | ToU (row), eff. 2026-01-01 | Offset by docs listing `codex exec`/SDK for Plus and higher, and by the "Codex as a platform" post. Runs are user-initiated, local, and for the user only |
| Account sharing | may not "share your account credentials or make your account available to anyone else" | ToU | One student, one machine, their own sign-in. Branded (UTMCSSA) builds must never pool accounts |
| Rate limits | may not "circumvent any rate limits or restrictions" | ToU | No aggressive retries or account rotation. Show the reset time |
| Distribution | may not "Modify, copy, lease, sell or distribute any of our Services" | ToU | Don't redistribute. Download from OpenAI's release |
| Licensed Materials | "may not modify, redistribute, or sublicense"; OSS terms override | Service Terms §10 (upd. 2026-09-21) | as above |
| Codex output | Codex output "may be subject to third party licenses" | Service Terms §4 | Minor, since PageLamp output is study text |
| Terms that apply | "the ChatGPT Terms of Use and Privacy Policy … apply to data shared between Codex and ChatGPT" | help.openai.com/…/11369540 | The student's own terms govern. PageLamp is not a party |
| Usage Policies | prohibits use for "academic dishonesty" | https://openai.com/policies/usage-policies/ (eff. 2025-10-29) | Matches PageLamp's no-assignment-solving rule. Keep the policy engine and prompts strict |
| Embedding invited | "you can start with Codex instead of inventing a new runtime"; "model access and managed services remain separate"; examples include support, ops, and tax prep | https://developers.openai.com/blog/codex-as-a-platform (Aug 19, 2026) | Supports embedding for non-coding work. The harness is open, but **model access still follows the user's plan terms** |
| Security guidance | "Don't expose Codex execution in untrusted or public environments" | learn.chatgpt.com/docs/auth | Local desktop use for one user fits |
| Brand | Don't imply endorsement; "ChatGPT"/"GPT" marks are OpenAI's | openai.com/brand (seen via search snippet only; **[unverified]** full text) | Label it "Use my ChatGPT plan (through OpenAI Codex)". No OpenAI logos |
| Age / region | 13+ (under 18 needs guardian permission); trade controls | ToU | Students are fine. ChatGPT is unavailable in some regions, e.g. while a student is in mainland China **[unverified list]** |

No OpenAI document I found **explicitly** permits or forbids a third-party consumer app launching the official CLI with
the user's ChatGPT login. Secondary reports say OpenAI has not restricted third-party use of ChatGPT OAuth the way
Anthropic (2026-02-20) and Google did (zeniteq.com, **[secondary, unverified]**). PageLamp's approach is safer than
those third-party OAuth re-implementations, because the unmodified binary handles its own login.

## 9. What changed since 2025

- **Docs.** Moved to `learn.chatgpt.com`. Codex usage is now shared with "ChatGPT Work". `codex app` now "Launch[es]
  ChatGPT desktop app", and there is a Linux desktop app.
- **Versions.** 0.77.0 (Dec 2025) → 0.157.1 (Sep 2026). Commands now carry maturity labels.
- **Removals and deprecations.** `codex mcp-server` removed (0.154.0). `approval_policy="untrusted"` removed
  (0.149.0). `--full-auto` deprecated. The legacy `exec --json` format was removed back in 0.45.0. WSL1 dropped (0.115).
- **New auth options.** Device-code (beta), keyring/auto/ephemeral storage, Enterprise access tokens, workload
  identity, Bedrock.
- **SDKs.** The Python SDK is stable (runs on app-server, pinned runtime). The TypeScript SDK wraps exec.
- **Plans.** Free and Go "include" Codex in the desktop app. Pro split into $100/$200 tiers. Token-based credits.
  $100 student credits (US/CA).
- **Models.** GPT-6 Sol/Luna/Astra. GPT-5.4 removed; GPT-5.5 retires 2026-10-14.
- **MCP client.** Reads server `instructions`. Adds `required`, per-tool approval modes and token limits, and
  CIMD/DCR OAuth.
- **Positioning.** The "Codex as a platform" post (2026-08-19) explicitly invites embedding in non-coding products.

## 10. Risks

1. **Terms ambiguity** (medium-low). See §8. *Mitigation:* ask OpenAI in writing. Keep the path behind a switch. Use
   only documented surfaces. Never touch tokens.
2. **Churn** (high likelihood, medium impact). Weekly releases, removed commands, retired models, config keys that
   change. *Mitigation:* pin; `--strict-config`; CI contract test; monthly pin bumps; tripwire.
3. **Free/Go coverage** (high impact on reach). The cheapest students may get no CLI. *Mitigation:* test. If CLI is
   unavailable, fall back to BYOK/local and to v0.1 reverse MCP through the ChatGPT desktop app, which Free/Go do get.
4. **Prompt injection via course material** (medium). *Mitigation:* tools off (shell, web, apps, code mode, multi-agent,
   view_image), read-only sandbox, empty temp cwd, dedicated CODEX_HOME, and a **JSONL tripwire** that kills the run
   on any `command_execution`, `file_change`, `web_search`, or non-allow-listed `mcp_tool_call` item.
5. **Silent switch to API billing** (medium). Inherited `CODEX_API_KEY`/`OPENAI_API_KEY`, or a Codex login that uses
   an API key. *Mitigation:* scrub the environment; show the auth mode from `login status`.
6. **Brittle error handling in exec** (medium). Errors are plain strings. *Mitigation:* heuristic mapping plus a raw
   fallback. Optionally, a failure-tolerant app-server probe for `account/rateLimits/read` and `planType`.
7. **Edu workspace visibility** (privacy). Compliance logs cover local Codex. *Mitigation:* disclose it; show the plan
   type.
8. **Keychain/ACL prompts after a Codex binary update on macOS** (low–medium) **[unverified]**. *Mitigation:* test.
   Fall back to `auto`, which uses a 0600 file inside the dedicated CODEX_HOME.
9. **Windows sandbox setup/UAC prompts** even with tools off **[unverified]**. *Mitigation:* `[windows] sandbox =
   "unelevated"` in the dedicated config, and test.
10. **Disk and bandwidth.** ≈65–75 MB download and ≈250 MB on disk per version. *Mitigation:* opt-in download; keep
    at most 2 versions.

## 11. Recommended integration shape (for ARCHITECTURE/v0.3 design)

```
PageLamp (Rust, pagelamp-app facade → Tauri + Swift/UniFFI)
 ├─ policy engine: AiMaterialsState filter → prompt text (course text only if readable)
 ├─ pagelamp-llm::Provider  ← Codex is one provider next to BYOK + local
 │    └─ CodexProvider → CodexBackend trait { Exec (default, Stable) | AppServer (spike, Experimental) }
 └─ codex_runtime: pin, download+verify, CODEX_HOME=<data_dir>/codex-home, login/status/logout, run queue (1 at a time)

spawn: env_clear + allowlist; CODEX_HOME=<data_dir>/codex-home; cwd=<data_dir>/codex-runs/<uuid> (empty)
  codex exec - --json --ephemeral --skip-git-repo-check --strict-config -s read-only
       -m <per-feature model, e.g. gpt-6-luna for summaries> --output-schema <schema.json> -o <out.json>
       -c features.shell_tool=false -c web_search="disabled" -c tools.view_image=false -c features.apps=false
       -c features.multi_agent=false -c features.code_mode.enabled=false -c features.hooks=false
       -c history.persistence="none" -c project_doc_max_bytes=0 -c model_instructions_file=<pagelamp-tutor.md>
  stdin: prompt (never argv) → parse JSONL → tripwire → validate JSON locally (schema) → repair/retry once
```

- Dedicated `config.toml` written by PageLamp on each launch: `cli_auth_credentials_store="auto"`,
  `check_for_update_on_startup=false`, `feedback.enabled=false`, `[windows] sandbox="unelevated"`,
  `analytics.enabled` = owner's decision. The same keys also go in as `-c` overrides. **Exact keys need
  re-checking against each pinned version.**
- New facade methods (FFI-friendly):
  - `codex_status() -> CodexStatus { install: Option<{version, source: managed|system}>, pinned, auth: signed_out|chatgpt|api_key|unknown, supported }`
  - `install_codex(on_event)`, `remove_codex()`
  - `codex_login_start(method: browser|device_code, on_event)`, `codex_login_cancel()`, `codex_logout()`
  - generation through the provider API.
  - "Remove all data" runs `codex logout` first, then deletes `codex-home/`.
- Tutor mode later: attach `-c mcp_servers.pagelamp.command=<abs pagelamp>` with read-only args,
  `enabled_tools=[…read tools…]`, `default_tools_approval_mode="approve"`, `required=true`.
- Rough effort **[estimate]**: backend 2–3 person-weeks (runtime manager, login, exec driver, tripwire, tests);
  frontend about 1 week (setup, sign-in, and status screens); leader: pin, CI contract test, Swift wiring.

### Manual test checklist for the owner
1. Free, Go, and Plus accounts: `codex login` then `codex exec -` with a tiny prompt. Does it work? Record the exact
   error text if it fails.
2. With student credits (if eligible): does a Free account's CLI draw on the credits?
3. `--output-schema` with the ChatGPT login on `gpt-6-luna` and `gpt-6-sol`. Record usage tokens per run and the
   `/status` change.
4. With a dedicated CODEX_HOME, confirm the global `~/.codex/AGENTS.md` is not used (ask the model to list its
   instruction sources).
5. `--strict-config` with the generated config on the pinned version.
6. Keychain storage on macOS. Is there a prompt after swapping to a newer codex binary?
7. Windows: does exec with tools off trigger sandbox setup or UAC?
8. Linux without `bwrap`: only a warning, or a failure?
9. A Codex login that uses an API key: `login status` output; PageLamp warns about API billing.
10. Hit a usage limit (or simulate one): capture the `turn.failed` message text.
11. If available, a ChatGPT Edu account: is local Codex enabled, and which workspace gets forced?
12. (Added 2026-09-28) **Default-model canary:** on each test account, run the pinned version once
    without `-m` and once with the pin's `-m`, with stdin closed (`</dev/null` or the prompt on
    stdin). Record whether either fails with "requires a newer version of Codex".

## 12. Questions only the owner can answer

1. Is download-on-demand OK (≈70 MB download, ≈250 MB on disk), or must PageLamp work fully offline after install?
   (I recommend download.)
2. Should PageLamp use a dedicated `CODEX_HOME`? It means a second sign-in but a clean, deterministic setup. The
   alternative is reusing `~/.codex`: no second sign-in, but the student's AGENTS.md, skills, and plugins get
   inherited and their Codex history gets polluted. (I recommend dedicated.)
3. Should we email OpenAI (developer support/partnerships) for written confirmation before shipping mode A? Is it a
   blocker, as for Anthropic, or only nice to have? (I recommend asking, but not blocking.)
4. Are Free/Go students in scope for mode A? If the CLI turns out paid-only, is "BYOK/local + reverse MCP" enough for
   them?
5. Does UofT give students ChatGPT Edu? If yes, add the admin-visibility disclosure and test an Edu account.
6. Should OpenAI analytics in the dedicated Codex config (`analytics.enabled`) be off, or left at the default?
7. Offer "use my installed Codex" as an advanced option? It makes the support matrix bigger.
8. Supported targets: all six (macOS arm64/x64, Windows x64/arm64, Linux x64/arm64)? Minimum macOS (Codex binary
   minos is 11.0)?
9. CI: will you fund a small PageLamp-owned OpenAI API key for live exec contract tests? A ChatGPT login must never be
   used in CI.
10. Show remaining ChatGPT allowance? That needs the experimental app-server `account/rateLimits/read`, not exec.

---
Sources (all accessed 2026-09-27): https://github.com/openai/codex (releases, LICENSE, NOTICE, tag `rust-v0.157.1` source) ·
https://learn.chatgpt.com/llms.txt · /docs/non-interactive-mode · /docs/developer-commands?surface=cli · /docs/app-server ·
/docs/mcp-server · /docs/codex-sdk · /docs/auth · /docs/extend/mcp · /docs/config-file/config-reference ·
/docs/config-file/environment-variables · /docs/feature-maturity · /docs/pricing · /docs/models · /docs/sandboxing ·
/docs/agent-approvals-security · /docs/windows/windows-sandbox · /docs/codex/cli · /docs/open-source ·
/docs/agent-configuration/agents-md · https://developers.openai.com/blog/codex-as-a-platform ·
https://openai.com/policies/row-terms-of-use/ · https://openai.com/policies/service-terms/ ·
https://openai.com/policies/usage-policies/ · https://help.openai.com/en/articles/11369540-using-codex-with-your-chatgpt-plan ·
https://help.openai.com/en/articles/20001147-codex-credits-for-students-terms-of-service ·
https://developers.openai.com/community/students · secondary: https://www.morphllm.com/codex-pricing ,
https://www.zeniteq.com/you-can-now-sign-into-openclaw-using-your-chatgpt-account-q4kbdm ,
https://community.openai.com/t/login-with-chatgpt-allow-users-to-use-their-own-plus-subscription-in-3rd-party-apps/1378506
