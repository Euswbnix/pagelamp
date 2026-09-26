# Contributing to PageLamp

Thanks for helping! Bug reports, ideas, docs fixes and code are all welcome.
[中文 ↓](#中文)

## Ground rules

- **Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) first**, especially §3 (hard rules): Canvas
  access is GET-only and only from sync; the MCP server never touches the network; secrets live only
  in the OS keychain; no assignment solving; course materials are withheld for "No AI" courses.
  Pull requests that break these rules can't be merged.
- **Synthetic data only.** Never commit real course materials, names, tokens or calendar links —
  not in tests, fixtures, screenshots or issues.
- **Licence:** contributions are accepted under [Apache-2.0](LICENSE). New dependencies must be
  Apache-2.0-compatible (checked by `cargo deny check licenses` in CI) — no GPL/AGPL/SSPL/BUSL/FSL.
  Don't copy code from AGPL projects such as canvas-lms or Better Canvas.

## Development setup

- Rust 1.89+ (`rustup`), Node 24, pnpm 12 (`corepack enable`).
- macOS: Xcode Command Line Tools · Windows: Visual Studio Build Tools (+ NASM or
  `AWS_LC_SYS_PREBUILT_NASM=1`) · Linux: `build-essential pkg-config libdbus-1-dev
  libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf`.

```bash
# Rust workspace (core, sources, MCP server, CLI)
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Desktop app
cd apps/desktop
pnpm install
pnpm run dev:mock        # UI in the browser with synthetic data
pnpm run typecheck && pnpm run lint && pnpm run test && pnpm run build
pnpm tauri dev           # the real app
pnpm run smoke           # real app against a throw-away synthetic course folder
```

Try the MCP server without touching your real data:

```bash
export PAGELAMP_HOME="$(mktemp -d)"
cargo run -p pagelamp-cli -- folder add path/to/synthetic/Courses
cargo run -p pagelamp-cli -- sync
cargo run -p pagelamp-cli -- mcp-config claude-desktop
```

## Where things live

| Path | What |
|---|---|
| `crates/pagelamp-core` | data model, SQLite store, timeline inference, read views |
| `crates/pagelamp-extract` | PDF/PPTX/DOCX/notebook/HTML text extraction |
| `crates/pagelamp-local`, `crates/pagelamp-canvas` | folder + calendar-feed sources, Canvas (read-only) |
| `crates/pagelamp-mcp` | MCP server — **all wording sent to AI apps is in `src/text.rs`** (editable without Rust knowledge) |
| `crates/pagelamp-app` | the facade used by the CLI and the desktop app |
| `apps/pagelamp-cli` | the `pagelamp` command |
| `apps/desktop` | Tauri 2 + React app (copy in `src/i18n`, brand in `src/brand`) |

## Pull requests

- Small, focused PRs with tests. CI must be green (Linux, macOS, Windows).
- Commit subject: imperative, ≤ 72 characters, area prefix — `core:`, `extract:`, `canvas:`,
  `local:`, `mcp:`, `app:`, `cli:`, `desktop:`, `docs:`, `ci:`. Explain *why* in the body.
- User-facing text changes need both English and 简体中文.

## 中文

欢迎提 bug、提建议、改文档和提交代码。提交前请先读 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) 第 3 节的硬性规则
（Canvas 只读、MCP 不联网、密钥只存系统钥匙串、不代写作业、"禁止使用 AI"的课不共享课件）。
测试和截图只能用合成数据，不能出现真实课件、姓名、令牌或日历链接。贡献按 Apache-2.0 授权，新依赖必须与之兼容。
给 AI 看的所有文字都在 `crates/pagelamp-mcp/src/text.rs`，界面文案在 `apps/desktop/src/i18n`，不会 Rust 也能改。
面向用户的文字改动需要同时提供英文和简体中文。
