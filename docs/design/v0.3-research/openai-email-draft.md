Subject: Permission question — local open-source student app running the user's own signed-in, unmodified Codex CLI (`codex exec`)

Hello,

I maintain PageLamp (https://github.com/Euswbnix/pagelamp), a free, Apache-2.0 desktop app that keeps a
university student's own course materials in a local database on their computer. There is no PageLamp
server: we don't resell or pay for model usage, and we never read, store or transmit OpenAI credentials.

For our next release we would like students who already have a ChatGPT plan that includes Codex to
optionally generate study plans and weekly explanations of their own course materials. PageLamp would:

- download the official, unmodified Codex CLI release from github.com/openai/codex on demand (checking
  its checksum and OpenAI's code signature) instead of bundling a copy;
- let the student sign in only through Codex's own "Sign in with ChatGPT" flow (`codex login`), in a
  Codex home folder dedicated to PageLamp;
- run `codex exec` locally, only when the student asks, with a read-only sandbox, no tools, no MCP
  servers and a JSON output schema: one run per generation, capped per week, with the token count shown
  to the student.
- turn the model's tools off only through Codex's documented configuration: `features.shell_tool = false`,
  web search disabled, and a `model_catalog_json` file that repeats the bundled catalog entries of the
  models we use with `tool_mode = "direct"` and `shell_type = "disabled"` (so no code-mode or shell tool
  is offered). The Codex binary itself is never modified.

Your documentation says "Use API key authentication for programmatic Codex CLI workflows, such as CI/CD
jobs" and "API keys are the right default for automation". Our case is interactive (the student clicks
"Generate" and waits for the result), but it is still a program invoking `codex exec`, so before we ship
we would like written confirmation on these questions:

1. Is it acceptable under OpenAI's terms for a local, open-source, single-user app, started by the
   student, to run the student's own signed-in, unmodified `codex exec` with their ChatGPT plan as
   described above, or should such an app use API keys only?
2. Is downloading the official release on demand (rather than bundling it) the right approach? Are there
   naming or attribution requirements (for example, may we label the option "Use your ChatGPT plan
   (runs OpenAI Codex)")?
3. Which ChatGPT plans may use `codex exec` this way (Free, Go, Plus, Pro, Business, Edu)? Can usage
   beyond a plan's limits draw ChatGPT credits without the student taking an explicit action?
4. Is overriding those catalog entries through `model_catalog_json` an acceptable way to run Codex with
   no tools, or is there a supported setting we should use instead? Is there anything else we should
   change in the design to stay within your policies?
5. May we quote your reply in our public documentation?

We're happy to adjust the design or join a short call. Thank you.

[Your name]
Maintainer, PageLamp (GitHub: Euswbnix)
