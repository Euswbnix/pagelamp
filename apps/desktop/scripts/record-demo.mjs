#!/usr/bin/env node
// Record the README demo GIF from mock mode (synthetic data only), headless.
//
//   pnpm exec vite --mode mock --port 1430     # in another terminal
//   node scripts/record-demo.mjs               # → docs/assets/demo.gif
//
// Drives Chrome/Chromium over the DevTools protocol (no extra npm packages): onboarding →
// courses → a course → its AI policy → "Connect your AI app", captures the screencast frames
// and turns them into a GIF with ffmpeg. Browser: $CHROME, else Playwright's cached Chromium,
// else Google Chrome. ffmpeg: $FFMPEG, else `ffmpeg` on PATH.

import { execFileSync, spawn } from "node:child_process";
import { existsSync, mkdtempSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const desktopDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const out = resolve(desktopDir, "../../docs/assets/demo.gif");
const BASE = process.env.DEMO_URL ?? "http://localhost:1430";
const WIDTH = 1280;
const HEIGHT = 800;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function findChrome() {
  if (process.env.CHROME) return process.env.CHROME;
  const cache = join(homedir(), "Library/Caches/ms-playwright");
  if (existsSync(cache)) {
    for (const dir of readdirSync(cache)
      .filter((d) => /^chromium-\d+$/.test(d))
      .sort()
      .reverse()) {
      for (const app of ["chrome-mac-arm64", "chrome-mac"]) {
        const bin = join(cache, dir, app, "Chromium.app/Contents/MacOS/Chromium");
        if (existsSync(bin)) return bin;
      }
    }
  }
  return "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
}

const frameDir = mkdtempSync(join(tmpdir(), "studentos-demo-"));
const profile = mkdtempSync(join(tmpdir(), "studentos-demo-profile-"));
const port = 9334;
const chrome = spawn(findChrome(), [
  "--headless=new",
  `--remote-debugging-port=${port}`,
  `--user-data-dir=${profile}`,
  "--no-first-run",
  "--no-default-browser-check",
  "--hide-scrollbars",
  `--window-size=${WIDTH},${HEIGHT}`,
  "about:blank",
]);

async function pageSocket() {
  for (let i = 0; i < 50; i++) {
    try {
      const list = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
      const page = list.find((t) => t.type === "page");
      if (page) return page.webSocketDebuggerUrl;
    } catch {}
    await sleep(200);
  }
  throw new Error("browser did not start");
}

const ws = new WebSocket(await pageSocket());
await new Promise((r) => ws.addEventListener("open", r, { once: true }));
let nextId = 0;
const pending = new Map();
const frames = [];
ws.addEventListener("message", (e) => {
  const msg = JSON.parse(e.data);
  if (msg.id && pending.has(msg.id)) {
    pending.get(msg.id)(msg);
    pending.delete(msg.id);
  } else if (msg.method === "Page.screencastFrame") {
    const { data, metadata, sessionId } = msg.params;
    const file = join(frameDir, `${String(frames.length).padStart(5, "0")}.jpg`);
    writeFileSync(file, Buffer.from(data, "base64"));
    frames.push({ file, t: metadata.timestamp });
    send("Page.screencastFrameAck", { sessionId });
  }
});
function send(method, params = {}) {
  const id = ++nextId;
  ws.send(JSON.stringify({ id, method, params }));
  return new Promise((ok, fail) =>
    pending.set(id, (m) => (m.error ? fail(new Error(JSON.stringify(m.error))) : ok(m.result))),
  );
}
const evaluate = async (expression) =>
  (await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true })).result
    ?.value;

const CLICKABLE = "button, a, [role=tab], [role=radio], [role=checkbox], label";

/** Wait (up to 10 s) until an element whose text contains `text` is on the page. */
async function waitFor(text, selector = CLICKABLE) {
  for (let waited = 0; waited < 10_000; waited += 100) {
    const found = await evaluate(
      `[...document.querySelectorAll(${JSON.stringify(selector)})].some((e) => e.textContent.trim().includes(${JSON.stringify(text)}))`,
    );
    if (found) return;
    await sleep(100);
  }
  throw new Error(`"${text}" never appeared`);
}

/** Click the first element whose text contains `text` (waiting for it to appear). */
async function click(text, selector = CLICKABLE) {
  await waitFor(text, selector);
  const ok = await evaluate(`(() => {
    const el = [...document.querySelectorAll(${JSON.stringify(selector)})]
      .find((e) => e.textContent.trim().includes(${JSON.stringify(text)}));
    if (!el) return false;
    el.scrollIntoView({ block: "center", behavior: "instant" });
    el.click();
    return true;
  })()`);
  if (!ok) throw new Error(`nothing to click with text "${text}"`);
}

/** Smoothly scroll the app's scrolling area (the <main> of the shell, else the window). */
async function scrollBy(px, ms = 900) {
  const steps = Math.max(1, Math.round(ms / 50));
  for (let i = 0; i < steps; i++) {
    await evaluate(
      `(document.querySelector("main") ?? document.scrollingElement).scrollBy(0, ${px / steps})`,
    );
    await sleep(50);
  }
}

try {
  await send("Page.enable");
  await send("Runtime.enable");
  await send("Emulation.setDeviceMetricsOverride", {
    width: WIDTH,
    height: HEIGHT,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await send("Emulation.setEmulatedMedia", {
    features: [{ name: "prefers-color-scheme", value: "light" }],
  });
  // Fresh first run: empty mock (its first folder sync "finds" the demo courses).
  await send("Page.navigate", { url: `${BASE}/?scenario=empty#/welcome` });
  await sleep(1200);
  await send("Page.startScreencast", {
    format: "jpeg",
    quality: 85,
    maxWidth: WIDTH,
    maxHeight: HEIGHT,
    everyNthFrame: 1,
  });

  await sleep(1400); // Welcome + AI disclosure
  await click("I understand", "label");
  await sleep(700);
  await click("Get started");
  await sleep(1200); // choose a source
  await click("Choose folder");
  await sleep(900);
  await click("Add and continue");
  await waitFor("Go to my courses"); // the first sync runs
  await sleep(1300); // "Your courses are ready"
  await click("Go to my courses");
  await sleep(1700); // This week + study plan
  await scrollBy(560, 1100);
  await sleep(1200); // course cards
  await click("DEMO101", "a");
  await sleep(2000); // course: this week's materials
  await click("AI policy", "[role=tab]");
  await sleep(1200);
  await scrollBy(420, 1000);
  await sleep(1500); // policy + "Let my AI app read this course's materials"
  await click("Connect your AI app", "a");
  await sleep(2200); // Claude Desktop first, copyable snippet
  await send("Page.stopScreencast");
} finally {
  ws.close();
  chrome.kill();
}

if (frames.length < 2) throw new Error("no frames captured");
// Frames arrive only when the page changes; hold each one until the next (true timing).
const lines = [];
for (let i = 0; i < frames.length; i++) {
  const next = frames[i + 1]?.t ?? frames[i].t + 1;
  lines.push(
    `file '${frames[i].file}'`,
    `duration ${Math.max(0.02, next - frames[i].t).toFixed(3)}`,
  );
}
lines.push(`file '${frames.at(-1).file}'`);
const list = join(frameDir, "frames.txt");
writeFileSync(list, `${lines.join("\n")}\n`);

const ffmpeg = process.env.FFMPEG ?? "ffmpeg";
const filter =
  "fps=10,scale=960:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=128:stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle";
execFileSync(
  ffmpeg,
  [
    "-y",
    "-loglevel",
    "error",
    "-f",
    "concat",
    "-safe",
    "0",
    "-i",
    list,
    "-filter_complex",
    filter,
    "-loop",
    "0",
    out,
  ],
  {
    stdio: "inherit",
  },
);
rmSync(frameDir, { recursive: true, force: true });
rmSync(profile, { recursive: true, force: true });
const seconds = frames.at(-1).t - frames[0].t;
console.log(
  `${out}: ${(statSync(out).size / 1024 / 1024).toFixed(2)} MB, ~${seconds.toFixed(1)} s, ${frames.length} frames`,
);
