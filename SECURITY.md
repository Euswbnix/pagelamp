# Security policy

PageLamp handles students' course data and Canvas access tokens on their own computers, so we take
security reports seriously.

## Reporting a vulnerability

**Please don't open a public issue.** Use GitHub's private reporting instead:
[Security → Report a vulnerability](https://github.com/Euswbnix/pagelamp/security/advisories/new).
Include steps to reproduce with **synthetic** data only — never real tokens, calendar links or
course materials.

We aim to acknowledge reports within 7 days. Only the latest release is supported during the 0.x
series.

## In scope

- Anything that could leak a Canvas token or calendar-feed link (logs, database, MCP output, errors).
- Any way for course content (e.g. a malicious page or PDF) to make PageLamp write to Canvas, run
  commands, reach the network from the MCP server, or escape the `<course_material>` wrapper.
- Material text reaching an AI app for a course whose AI access is off or policy is "No AI".
- Crashes or resource exhaustion from malformed files during sync.

## Out of scope

What your AI app or AI provider does with the data you choose to show it, and vulnerabilities in
third-party AI apps.
