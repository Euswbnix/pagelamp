# StudentOS

**Connect your courses. Your AI app knows what you're learning.**

> 🚧 **Early development — not usable yet.** v0.1 is being built in the open. Star/watch to follow along.

StudentOS keeps a local, searchable picture of your courses — what week each course is in, this
week's materials, upcoming deadlines — and hands it to the AI app you already use (Claude Desktop,
ChatGPT desktop, Claude Code, Codex) through [MCP](https://modelcontextprotocol.io). Ask
*"What's happening in my courses this week?"*, *"Explain this week's CSC-whatever lecture and cite
the slides"*, or *"Make me a study plan for the next two weeks"* — without re-uploading anything.

## Principles

- **Local-first.** Your course data lives in a SQLite file on your computer. StudentOS runs no
  server and collects nothing.
- **Your AI, your account.** StudentOS doesn't run a model or resell one; your own AI app reads
  your local course data when you ask it to.
- **Read-only, learning-first.** StudentOS never submits anything, never posts, never fetches
  assignment instructions to solve them. It helps you follow and understand your courses and
  respects each course's AI policy.
- **Works beyond one LMS.** Canvas first; a course folder + calendar feed works with any LMS.

## How it works (v0.1)

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
- `apps/desktop` — Tauri 2 + React desktop app (onboarding, sources, courses, "connect your AI app").
- `docs/` — [architecture & team contract](docs/ARCHITECTURE.md), design notes.

## A note on Canvas access tokens

Canvas personal access tokens are for **your own use**. Canvas's API policy does not allow apps to
ask other users to generate tokens, and tokens expire (Canvas shows the maximum, often 30–90 days). The shareable way to
use StudentOS is a course folder + your LMS calendar feed; institution-approved sign-in is on the
roadmap.

## Roadmap

v0.1 MCP-first core · v0.2 reminders, one-click Claude Desktop extension, signed releases ·
v0.3 StudentOS generates plans and explanations itself (bring your ChatGPT/Claude subscription,
an API key, or a local model). Details: [docs/ARCHITECTURE.md §7](docs/ARCHITECTURE.md).

## 中文简介

StudentOS 把你的课程（每门课讲到第几周、本周材料、截止日期）整理成一份**存在你电脑上**的课程知识库，
再通过 MCP 交给你已经在用的 AI（Claude、ChatGPT、Codex 等）。它只读、不提交任何东西、不替你写作业，
帮助你跟上课程、理解内容、规划学习。目前处于早期开发阶段。

## License

[Apache-2.0](LICENSE). Not affiliated with, endorsed by, or sponsored by any university or
Instructure (Canvas). Canvas is a trademark of Instructure, Inc.
