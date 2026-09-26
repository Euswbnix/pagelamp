# StudentOS

**Connect your courses. Your AI app knows what you're learning.**

[![CI](https://github.com/Euswbnix/studentos/actions/workflows/ci.yml/badge.svg)](https://github.com/Euswbnix/studentos/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

> **Beta.** v0.1 is new — please [report anything odd](https://github.com/Euswbnix/studentos/issues/new/choose).

![StudentOS demo](docs/assets/demo.gif)

StudentOS keeps a local, searchable picture of your courses — which week each course is in, this
week's materials, upcoming deadlines — and hands it to the AI app you already use (Claude Desktop,
ChatGPT desktop, Claude Code, Codex) through [MCP](https://modelcontextprotocol.io). Ask
*"What's happening in my courses this week?"*, *"Explain this week's lecture and cite the slides"*,
or *"Make me a study plan for the next two weeks"* — without re-uploading anything.

[中文说明 ↓](#中文说明)

## Principles

- **Local-first.** Your course data lives in a database on your computer. StudentOS runs no server,
  has no account and collects nothing. See [PRIVACY.md](PRIVACY.md).
- **Your AI, your account.** StudentOS doesn't run or resell a model; your own AI app reads your
  local course data when you ask it something.
- **Read-only and learning-first.** StudentOS never submits, posts or marks anything, and never
  fetches assignment instructions to solve them. It tells your AI app to tutor, cite its sources and
  respect each course's AI policy — and for courses you mark "No AI", it doesn't share the materials.
- **Works with any LMS.** A course folder + your LMS calendar feed works everywhere; Canvas sync with
  your own token is available for personal use.

## Install

### Desktop app (recommended)

Download the installer for your system from the
[latest release](https://github.com/Euswbnix/studentos/releases): `.dmg` for macOS (Apple silicon
or Intel), `.msi`/`.exe` for Windows, `.AppImage`/`.deb` for Linux. The desktop app includes the
`studentos` command-line tool your AI app needs.

The beta is **not code-signed yet**:
- **macOS:** the first time, right-click StudentOS.app → **Open** → **Open**. If your AI app later
  can't start StudentOS, run once: `xattr -dr com.apple.quarantine /Applications/StudentOS.app`
- **Windows:** if SmartScreen appears, click **More info → Run anyway**.

### Command-line tool only

**From a release.** Download the archive for your system from the
[releases page](https://github.com/Euswbnix/studentos/releases), unpack it, and put `studentos`
(`studentos.exe` on Windows) in a folder on your PATH (e.g. `~/.local/bin`). On macOS the binary
isn't notarized yet; if macOS refuses to run it, run
`xattr -d com.apple.quarantine /path/to/studentos` once. Check with `studentos --version`.

**From source.** You need Rust 1.89 or newer ([rustup.rs](https://rustup.rs)) and a C compiler
(Xcode Command Line Tools on macOS, Visual Studio Build Tools on Windows, `build-essential` on
Linux). On Windows also install NASM, or set `AWS_LC_SYS_PREBUILT_NASM=1`. Then:

```bash
git clone https://github.com/Euswbnix/studentos
cd studentos
cargo install --path apps/studentos-cli --locked
```

This installs `studentos` into `~/.cargo/bin`.

On Linux, saving a Canvas token or calendar-feed link needs a desktop keyring (GNOME Keyring or
KWallet); course folders work without one.

## Quick start

1. **Put your course files in one folder**, one sub-folder per course. Week folders help StudentOS
   tell which week each course is in:

   ```
   Courses/
     CHEM101 General Chemistry/
       course.toml          ← optional: term_start = 2026-09-08
       Week 1/  lecture01.pdf  notes.md
       Week 2/  lecture02.pptx
     MATH102 Calculus II/
       Week 1/ …
   ```

   PDF, PowerPoint, Word, Jupyter notebooks, Markdown, text, HTML and source code are indexed.

2. **Add your calendar feed** for deadlines (optional): in Canvas, open **Calendar → Calendar Feed**
   and copy the link; other LMSs call it "export calendar" or iCal. It's private — StudentOS keeps it
   in your system keychain.

3. **Add them and sync** — in the desktop app's onboarding, or:

   ```bash
   studentos folder add ~/Courses --term-start 2026-09-08
   studentos ical add            # paste the feed link when asked (optional)
   studentos sync
   studentos courses
   ```

4. **Connect your AI app.** Open *Connect your AI app* in the desktop app, or run
   `studentos mcp-config claude-desktop` (also `claude-code`, `codex`), and follow the printed steps.
   Then restart your AI app and ask: *"Using StudentOS, where is each of my courses this week?"*

| AI app | How it connects | Plans |
|---|---|---|
| Claude Desktop | local MCP server in `claude_desktop_config.json` | every Claude plan, including Free |
| ChatGPT desktop (Work/Codex mode), Codex CLI | `[mcp_servers.studentos]` in `~/.codex/config.toml` | documented for Plus and above and Edu |
| Claude Code | the `claude mcp add …` command printed by `studentos mcp-config claude-code` | paid Claude plans |

### Canvas access tokens

Canvas personal access tokens are for **your own use only** — Canvas's API policy doesn't allow apps
to ask other people to create them. Tokens expire (Canvas shows the maximum when you create one).
If you share StudentOS with classmates, point them to the course folder + calendar feed setup.
Canvas sync never downloads files unless you ask, because downloads can count as "viewed" in
module requirements.

### Course AI policies

Many universities don't allow generative AI in a course unless the instructor permits it — check
your syllabus. Record each course's policy in StudentOS (`studentos course policy CHEM101
learning_aid`, or the course's *AI policy* tab). Your AI app sees it and adjusts; for courses marked
`prohibited` ("No AI"), StudentOS doesn't share the course materials at all.

## How it works

```
your AI app (Claude Desktop / ChatGPT desktop / Claude Code / Codex)
        │  local MCP (stdio)
        ▼
studentos mcp  ──reads──▶  studentos.db (on your computer)  ◀──writes── studentos sync
                                                               ▲
                              course folder · calendar feed · Canvas (read-only)
```

- `crates/` — Rust core: data model & SQLite store, text extraction, Canvas / folder / iCal
  sources, MCP server, app facade.
- `apps/studentos-cli` — the `studentos` command.
- `apps/desktop` — Tauri 2 + React desktop app.
- `docs/` — [architecture & team contract](docs/ARCHITECTURE.md), design notes.

## Roadmap

v0.1 MCP-first core · v0.2 reminders, one-click Claude Desktop extension, signed releases ·
v0.3 StudentOS generates plans and explanations itself (bring your ChatGPT/Claude subscription,
an API key, or a local model). Details: [docs/ARCHITECTURE.md §7](docs/ARCHITECTURE.md).

## Contributing

Bug reports, ideas and pull requests are welcome — see [CONTRIBUTING.md](CONTRIBUTING.md).
Security issues: see [SECURITY.md](SECURITY.md).

## 中文说明

StudentOS 把你的课程——每门课讲到第几周、本周材料、截止日期——整理成一份**存在你电脑上**的课程知识库，
再通过 MCP 交给你已经在用的 AI（Claude Desktop、ChatGPT 桌面版、Claude Code、Codex）。它只读、不提交任何东西、
不替你写作业；会提醒 AI 标注出处、以辅导为主，并遵守每门课的 AI 政策。

**安装**：从 [Releases](https://github.com/Euswbnix/studentos/releases) 下载对应系统的安装包。
测试版还没有代码签名：macOS 首次打开请右键 StudentOS.app →「打开」；Windows 出现 SmartScreen 时点「更多信息 → 仍要运行」。

**上手**：
1. 把课件放进一个文件夹，每门课一个子文件夹，里面可以按「Week 1」「Week 2」分周；
2. （可选）在 Canvas 的 **Calendar → Calendar Feed** 复制日历订阅链接，用来导入截止日期；
3. 在桌面应用的引导页添加文件夹和日历订阅并同步；
4. 打开「连接你的 AI 应用」，按提示把 StudentOS 加到 Claude Desktop 等应用里，重启后问：「用 StudentOS 看看我这周各门课在讲什么？」

**关于 Canvas 令牌**：个人访问令牌仅供你本人使用，不要让同学生成令牌填进来；推荐给同学时请用「课程文件夹 + 日历订阅」方式。

**隐私**：数据只存在你的电脑上；只有你向 AI 提问时，AI 读取的课程内容才会发到你自己的 AI 账号。详见 [PRIVACY.md](PRIVACY.md)。

## License

[Apache-2.0](LICENSE). Not affiliated with, endorsed by, or sponsored by any university or
Instructure (Canvas). Canvas is a trademark of Instructure, Inc.
