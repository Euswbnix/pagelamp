# Spike: StudentOS Connector (browser extension + native messaging)

> 📜 **Historical note:** this prototype was written when the project was called **StudentOS** (renamed to **Weekmark** on 2026-09-26). Identifiers inside it (e.g. the native-host name `dev.studentos.connector_spike`) are kept as they were tested; the product itself is Weekmark.

> **Status: throwaway prototype, not product code.** It exists to check, with real browser mechanics,
> whether a GET-only MV3 extension that reuses the student's logged-in Canvas session can feed the
> StudentOS desktop app. Everything runs against a **local mock Canvas** with synthetic data.
> It never contacts a real Canvas/LMS host.

## Why

Canvas personal access tokens can't be handed out by third-party apps (Instructure's API policy), student
tokens expire, and OAuth needs an institution-issued Developer Key. One alternative, used by extensions like
Better Canvas, is to piggyback on the browser session. This spike measures whether that actually works, and
where it breaks, before we decide anything. The spike is **only technical**. It does not settle the policy
questions. Instructure's Canvas API Policy §3(i) (effective 2025-08-12) forbids accessing its APIs "using model
context protocol servers or other technologies not approved by Instructure". That applies to this design
whether or not the mechanics work.

No code was copied from Better Canvas (AGPL-3.0) or canvas-lms (AGPL-3.0). canvas-lms source was *read* only to
learn protocol facts (cookie names, JSON shapes), which the mock reproduces. All code here is original and
licensed Apache-2.0.

## Architecture

```
 Chrome profile of the student (logged in to Canvas)
 +------------------------------------------------------------------------------------+
 |  Canvas tab                                                                        |
 |    content.js  --- same-origin GET (browser attaches session cookie) --->  Canvas  |
 |       | chrome.runtime message                                                     |
 |       v                                                                            |
 |  sw.js (MV3 service worker) --- GET, credentials:'include' ------------->  Canvas  |
 |       |   triggers: "Sync now", chrome.alarms (no tab needed), visit to Canvas     |
 |       |   all network I/O through lib/getonly.js (throws on anything but GET)      |
 |       | chrome.runtime.connectNative('dev.studentos.connector_spike')              |
 +-------+----------------------------------------------------------------------------+
         v  stdio frames: 4-byte native-endian length + UTF-8 JSON
 native-host/host.mjs  (stands in for the StudentOS desktop app)
         -> out/<run>/NNN-snapshot-*.json, NNN-needs_login.json, NNN-file-*, host-events.jsonl
```

| Path | What |
|---|---|
| `extension/manifest.json` | MV3. `permissions: storage, alarms, nativeMessaging, scripting` (`scripting` is required by `chrome.scripting.registerContentScripts`). `optional_host_permissions` only; no host access until the user grants their Canvas origin. Fixed `key`, so the ID is always `lofmnmpamocdliinbojckhfdoojoknim`. |
| `extension/lib/getonly.js` | GET-only guard. Replaces `fetch` (and in documents `XMLHttpRequest.open` / `sendBeacon`) in every extension JS world. Throws `GetOnlyViolation` on non-GET, a request body, or an `X-CSRF-Token` header. |
| `extension/lib/canvas-core.js` | Shared sync core used by **both** content script and service worker (they differ only in `credentials`). Handles Link-header pagination (same-origin only), `while(1);` stripping, 401 means `needs_login`, soft per-course errors, and file download. |
| `extension/lib/native.js` | Native port wrapper: request/ack by id, chunking, disconnect bookkeeping. |
| `extension/sw.js` | Service worker: dynamic content-script registration for the granted origin, alarms, relay to host. It also has `self.spike.*` hooks that the tests drive. |
| `extension/content.js`, `options.*` | Content script (sync variant a); options page (enter URL, then `chrome.permissions.request`). |
| `native-host/host.mjs` | Node native host. Logs to stderr only, reassembles chunks, writes to `out/`, downloads signed URLs with plain `fetch` (no cookies), and can send an oversized reply for the limit test. |
| `native-host/install.mjs` | Registers or unregisters the host for **your** browser (see "Try it" below). Prints everything it writes. Supports `--dry-run` and `--uninstall`. |
| `mock-canvas/` | Mock Canvas on `http://localhost:<p1>` and a signed-URL file store on `http://127.0.0.1:<p2>` (a second site). |
| `tests/run.mjs` | Playwright end-to-end suite (details below). |
| `extension-key.pem` | Private key behind the manifest `key`. **Gitignored**, only needed to pack a `.crx`. The ID comes from the public key in the manifest. |

### What the mock reproduces (from canvas-lms source)

- Session cookie `_normandy_session`, HttpOnly (`config/initializers/session_store.rb`: `same_site: :none` + `secure` under `force_ssl`, `expire_after` 1 day). SameSite is configurable in the mock (Lax / None / Strict). Secure is added automatically for None.
- `_csrf_token` cookie holds a masked token and is **not** HttpOnly on the main host (`lib/canvas/request_forgery_protection.rb`, `gems/canvas_breach_mitigation`). Non-GET API calls need a matching `X-CSRF-Token`. Every non-GET `/api/v1` request is logged.
- Not logged in: `401 {"status":"unauthenticated","errors":[{"message":"user authorization required"}]}` + `WWW-Authenticate: Bearer realm="canvas-lms"`. Logged in but not allowed: current Canvas sends **403** `{"status":"unauthorized",...}` (`lib/authentication_methods.rb`); older Canvas sent 401. The mock has one course (2008) whose modules are forbidden, and can switch to legacy 401 behaviour.
- `Link` pagination with opaque `page=bookmark:...` tokens. The mock clamps `per_page` to 3 so `/courses` takes 3 pages. It also sends `X-Rate-Limit-Remaining` / `X-Request-Cost`.
- The file object's `url` (`/files/:id/download?download_frd=1&verifier=...`) returns a 302 to a signed URL on the second origin. The file store needs a valid, unexpired HMAC signature and ignores cookies. Its CORS header is switchable (`none` / `*`).
- `while(1);` JSON prefix is optional. Older Canvas prefixed session-authenticated GET JSON with it. **Current canvas-lms master no longer does** (there's no prefix in `ApplicationController#render`). The client strips it anyway.

## How to run

```sh
cd spikes/canvas-connector
npm install
npx playwright install chromium   # Playwright's own "Chrome for Testing" build, ~150 MB into ~/Library/Caches/ms-playwright
npm test                          # ~11 s; add -- --headed to watch, -- --keep-tmp to keep temp profiles
```

`npm test` starts the mock in-process. It then runs three **temporary** browser profiles under `.tmp/<run>/`, with the extension loaded via `--load-extension` in new headless mode. The host manifest goes in `<temp-profile>/NativeMessagingHosts/`, and Chromium does honour that location, so nothing is written to any real browser directory. At the end the suite closes everything and deletes the temp profiles. It checks that no host or browser process is left over, prints the table, and writes `out/<run>/results.json` with every check, observation and piece of evidence.

- **Launch A** uses a test manifest variant with `host_permissions: ["http://localhost/*", "http://127.0.0.1/*"]`, which covers both the Canvas and file-store origins.
- **Launch B** uses the same variant with only `["http://localhost/*"]` (no permission for the file store).
- **Launch C** is like A, plus profile prefs that block third-party cookies (`profile.cookie_controls_mode=1`).

The *test-only* manifest variant exists because the `chrome.permissions.request` prompt can't be clicked in automation. The shipped manifest has only `optional_host_permissions`. The options-page flow still runs for real: a Playwright click provides the user gesture, and `permissions.request` resolves `true` for an already-granted origin.

`npm run mock` runs the mock standalone on :4580/:4581 if you want to explore it manually. The test login is in `mock-canvas/fixtures.js`.

## Results (run of 2026-09-25, Chrome for Testing 153.0.8010.12, Playwright 1.63.0, Node 24.14.1, macOS arm64)

All 8 tests passed (58/58 checks).

| Test | Result | What was observed |
|---|---|---|
| **T1** Content-script same-origin GET | pass | Options page → `permissions.request` → content script registered at runtime for `http://localhost:<port>/*` only. One sync = 15 GETs, and **every one carried `_normandy_session`**. The server saw `Sec-Fetch-Site: same-origin`, no `Origin`, and `Referer` = the Canvas page, which looks exactly like Canvas's own frontend. Got the user, 8 courses, modules, 8 planner items and 2 file objects. The forbidden course came back 403 and was recorded as a soft error. Logged out: 401, which became `needs_login`. Automatic sync on page visit (content script → SW → host) also worked. |
| **T2** Service-worker `fetch(credentials:'include')` | pass | The HttpOnly session cookie was sent with **SameSite=Lax, None and Strict**. That held with a Canvas tab open, with **zero tabs open**, and from a `chrome.alarms` one-shot with zero tabs (fired after ~1.2 s because the extension is unpacked; packed extensions have a 30 s minimum). Control: an ordinary page on another site sending a credentialed request did **not** get the Lax/Strict cookie (it did get None). So Chrome treats extension requests to a host it has permission for as same-site. With third-party cookies **blocked** (the control confirmed the pref was active), the SW still sent the cookie for None and Lax. A `SameSite=None; Secure` cookie was accepted on `http://localhost` because localhost counts as secure. The server can tell these requests apart: SW requests arrive with `Sec-Fetch-Site: none`, **no `Origin`, no `Referer`**. |
| **T3** Link pagination | pass | The server clamped `per_page=100` to 3, and the client followed `rel="next"` through `page=1` → `bookmark:WzNd` → `bookmark:WzZd`: exactly 3 requests, 8 unique courses, from both contexts. `Link` and `X-Rate-Limit-Remaining` are readable from the SW (host permission bypasses CORS; the response type is `basic`). |
| **T4** Native messaging | pass | The host was found through `<user-data-dir>/NativeMessagingHosts`, and the hello round trip took ~40 ms. The caller origin arrives as `argv[2]` = `chrome-extension://lofmn.../`. **Extension → host single message:** 5 MiB unchunked OK (~33 ms), 32 MiB OK, 63.996 MiB OK. **64.004 MiB: `postMessage` throws synchronously `Message exceeded maximum allowed size of 64MiB.`**, so the limit is 64 MiB. Chunked transfers went through: 5 MiB as 10 × 512 KiB and **80 MiB as 80 × 1 MiB** (~0.3 s), so chunking removes the cap. **Host → extension:** 1,048,575 and exactly **1,048,576 bytes delivered**. At 1,048,577 bytes and at ~2 MB, **Chrome kills the host** and the port disconnects with `Error when communicating with the native messaging host.` That matches the documented 1 MB limit (`> 1 MiB` fails), so the host must chunk its replies. After the disconnect the extension reconnects and gets a fresh host process. |
| **T5** File download (302 → signed URL on a second origin) | pass (the full matrix was measured, see below) | **SW + host permission for the file-store origin:** small file and 5 MB file both OK, sha256 verified, bytes forwarded (chunked) to the host and matched. `response.url` exposes the signed URL, and **the desktop (plain Node `fetch`, no cookies) downloaded the 5 MB file with only that URL** (tampered signature → 403). **SW without that permission: fails** (`TypeError: Failed to fetch`). CORS applies to the redirected hop. Even with `Access-Control-Allow-Origin: *` it fails, because `credentials:'include'` forbids a wildcard. This was confirmed directly: the signed URL with `*` works with `credentials:'omit'` but not `'include'`. **Content script: fails whenever the file store sends no CORS headers, even though the extension has host permission** (content-script fetches follow the page's CORS rules). With `ACAO: *` it works, including 5 MB. The Canvas `url` (with verifier) fetched **without** cookies only redirects to `/login` in the mock; that is mock policy, and real behaviour depends on institution settings. No cookie was ever sent to the file-store origin. |
| **T6** GET-only guard | pass | SW: `getOnlyFetch` POST, global `fetch` POST, `fetch(new Request(.., DELETE))`, and GET with an `X-CSRF-Token` header all threw `GetOnlyViolation` before any I/O. Content script: the same, plus XHR POST and `navigator.sendBeacon`. The mock logged **0 non-GET `/api/v1` requests out of 390** over 3 browser launches, and **0 `X-CSRF-Token` headers**. No extension file reads `document.cookie` or `chrome.cookies`. |
| **T7** Logged out / expired | pass | Three cases all produced `401 unauthenticated` → `{type:"needs_login", origin, context, status, url, at}`, which the host wrote to `out/`, from both contexts: never logged in, a session expired server-side while the cookie was still present, and cookies cleared. The detection probe on the options page reported "Canvas, needs login". **Permission errors aren't logouts:** a forbidden course (403, and legacy 401 `"unauthorized"`) still gives `ok`, lands in `snapshot.errors`, and the sync continues. |
| **T8** `while(1);` prefix | pass | With the prefix on for all session GETs, both clients strip it and a full sync parses all prefixed responses. In the older "only when Accept lacks `application/json`" variant, the client's default `Accept: application/json` avoids the prefix, and `*/*` gets it and still parses. Current Canvas doesn't send it. |

**T5 matrix** (✔ = bytes fetched and sha256 matched)

| Fetcher | Host permission for file store | File store CORS | Result |
|---|---|---|---|
| service worker | yes | none / `*` | ✔ (small + 5 MB); signed URL handed to desktop ✔ |
| service worker | no | none | ✘ CORS on redirected hop |
| service worker | no | `*` | ✘ (wildcard + credentials) |
| content script | yes or no | none | ✘ CORS |
| content script | yes or no | `*` | ✔ (small + 5 MB) |
| desktop Node, signed URL only | n/a | n/a | ✔ 200, 5 MB, sha256 match; tampered sig 403 |

### What this means (technical only)

1. **The session-piggyback mechanics work**, including true background sync from the service worker with no Canvas tab open. The SameSite value and third-party-cookie blocking don't matter, provided the user grants host permission for their Canvas origin.
2. **The two sync variants look different to the server.** Content-script requests are indistinguishable from Canvas's own frontend. SW requests are identifiable (`Sec-Fetch-Site: none`, no Origin/Referer). Either way they are requests made as the student, with the student's session.
3. **Native messaging is fine for this data volume.** Snapshots are KBs to a few MB, under 64 MiB per message out. Replies to the extension must stay ≤ 1 MiB or be chunked.
4. **File downloads are the hard part.** They need either an extra optional host permission for the institution's file-store domain (then the desktop can download from the signed URL with no cookies), or a file store that sends `ACAO: *` (then the content script can fetch). Which applies to Quercus is **unknown**; see step 9 below.

## Caveats

- The mock mimics Canvas from source reading. It is not Canvas. Real Quercus may differ: SSO flow, file-store domain and CORS, `verifier` behaviour, rate limits, and extra 401 flavours (e.g. `status:"unverified"`).
- Tests use Playwright's **Chrome for Testing** build in new headless mode, not branded Google Chrome. Branded Chrome 137+ ignores `--load-extension`, so there the extension must be loaded by hand (see below). Edge was not tested.
- **MV3 service-worker lifetime was not measured.** Playwright attaches DevTools to the worker, which can keep it alive. Long syncs in real Chrome could hit the 30 s idle / 5 min limits; an open native port is expected to extend lifetime, but that is unverified here.
- The one-shot alarm fired after ~1.2 s only because the extension is unpacked. Store/packed extensions are clamped to ≥ 30 s.
- `optional_host_permissions` includes `http://*/*` only so the spike can target localhost. A real connector should limit this to `https://`.
- The native host writes snapshots **in plaintext** to `out/` (gitignored). The real desktop app would store them in its SQLite store instead.
- This spike does not address the Canvas API Policy, UofT's GenAI rules, or academic-integrity questions. Those still need to be decided separately.

## Try it on your real Canvas (you do this, in your own Chrome)

> ⚠️ **Read [docs/design/connector-extension.md](../../docs/design/connector-extension.md) first.** The
> project's current decision is **not** to ship this approach until the institution confirms it is acceptable.
>
> Only on **your own** account, at your own risk, with the understanding that it uses the Canvas API through
> your browser session, which Instructure's API Policy and Acceptable Use Policy may not permit. Everything is
> GET-only, but **reads are not side-effect free on a real Canvas**: they create page views and activity time
> visible to instructors, and file downloads can mark "must view" module requirements complete. Only click
> sync manually, don't use the `chrome.alarms` background path, and skip file downloads.
>
> This build still contains **test hooks** (`self.spike` in `sw.js` — arbitrary GET fetch and oversized
> native messages; `file_url` / `send_big` in the host). They are reachable only from DevTools or the
> extension itself, but this is a prototype, not something to leave installed or share with others.
> `out/` will contain your **real** course data in plaintext, so don't share or commit it. When done, remove the
> extension and run `node native-host/install.mjs --uninstall`.

1. **Preview the host registration** (writes nothing):
   `cd spikes/canvas-connector && node native-host/install.mjs --dry-run`
   It prints the extension ID, the launcher path, and the exact JSON it would write to
   `~/Library/Application Support/Google/Chrome/NativeMessagingHosts/dev.studentos.connector_spike.json`.
   Use `--browser edge` for Edge. No `npm install` is needed; the host only uses Node built-ins.
2. **Register the host:** `node native-host/install.mjs`. This writes exactly two files, `native-host/run-host.sh` (pinned to your current `node`) and the JSON above, and prints both. If you later switch Node versions (nvm), run it again.
3. **Load the extension:** open `chrome://extensions`, turn on *Developer mode*, click *Load unpacked*, and choose `spikes/canvas-connector/extension`. Check that the ID is **`lofmnmpamocdliinbojckhfdoojoknim`**.
4. Click the extension's toolbar button to open the options page. Enter your school's Canvas address (e.g. `https://canvas.example.edu`) and click **Allow & save**. When Chrome asks for access to that host, allow it. The page should say *"Canvas detected - ..."*.
5. Log in to Quercus normally in a tab of the same Chrome profile. The extension never sees your password.
6. Click **Check desktop app**. You should see `hello-ack` with a pid.
   - *"Specified native messaging host not found"*: redo step 2.
   - *"Access to the specified native messaging host is forbidden"*: the ID doesn't match the one from step 3.
7. Click **Sync now** (the service-worker path). The page shows a summary with course count, `coursePages`, `errors`, `rateLimitRemaining` and timing. The full data is in `out/<timestamp>/NNN-snapshot-service-worker.json`.
8. **Content-script path:** open any Quercus page. With "Sync when I visit Canvas" on, this writes `NNN-snapshot-content-script.json` (at most every 15 min). **Background path:** close every Quercus tab and click **Sync now** again. The periodic alarm (every 60 min) also runs without any tab.
9. *(Optional, read-only, answers T5 for Quercus.)* In Quercus, open a course file, then look in DevTools → Network for the `/files/<id>/download` request. Note the **host it redirects to**, and whether the final file response has an **`Access-Control-Allow-Origin`** header.
10. **What to report back** (not the `out/` files): the options-page verdict (step 4), the step 6 result, the step 7 summary JSON, whether step 8 produced a content-script snapshot, and the two facts from step 9.
11. **Clean up:** remove the extension in `chrome://extensions`, run `node native-host/install.mjs --uninstall` (it deletes exactly the two files from step 2), and run `rm -rf spikes/canvas-connector/out`.
