# Browser "Connector" extension — evaluation (2026-09-25)

Status: **evaluated, not scheduled.** Recommendation below awaits the maintainer's decision.
Evidence: 4 research tracks with adversarial fact-checks (Chromium source, canvas-lms source,
Chrome/MDN/Instructure/UofT primary docs, unauthenticated probes of a real Canvas host) and a local
prototype (`spikes/canvas-connector`, not committed) run against a mock Canvas: 58/58 checks, re-run
3× by an independent reviewer.

## Idea

A GET-only MV3 extension that reads the student's own Canvas data using the session they are
already logged into (the model used by Better Canvas / BetterCampus, ~2M users, and Tasks for
Canvas, ~1M), and hands it to the Weekmark core over Chrome Native Messaging. It would avoid
personal access tokens (≤ 30-day expiry, not distributable) and institutional Developer Keys.

## What the prototype proved (Chrome for Testing 153, mock Canvas)

| Question | Result |
|---|---|
| Content-script same-origin GET carries the HttpOnly session cookie | ✅ |
| Service-worker fetch with host permission carries it with SameSite=Lax/None/Strict, **with zero tabs open**, from `chrome.alarms`, and with third-party cookies blocked | ✅ (Chromium treats permitted extension requests as same-site and exempts the `chrome-extension` scheme from 3PC blocking) |
| Link-header pagination, rate-limit headers readable | ✅ |
| Native Messaging limits | extension→host 64 MiB per message; host→extension **exactly 1 MiB** (1 byte more → Chrome kills the host). Chunk both ways. Host manifest found in `<user-data-dir>/NativeMessagingHosts`. |
| Files | 302 → signed file-store URL; SW needs host permission for the file-store domain; the desktop can download the signed URL **without cookies** (sha256 verified, 5 MB). |
| GET-only guard | fetch/XHR/sendBeacon non-GET all throw before I/O; wire log shows 0 non-GET API requests. |
| Logged-out / expired session | 401 `unauthenticated` → structured `needs_login`; 403 recorded as soft error. |

Not proven: real SSO/MFA, real file-store domain and CORS, MV3 service-worker lifetime during long
syncs, Firefox/Safari (Safari's background fetch does not send cookies — content-script only).
Server can distinguish service-worker requests (`Sec-Fetch-Site: none`, no Origin/Referer).

## Why not ship it now (policy, not technology)

1. **Session auth is not a documented third-party auth method.** Canvas documents Bearer tokens
   / OAuth; multi-user apps "MUST use OAuth". A Connector functionally sidesteps the Developer Key
   approval that Instructure deliberately tightened (2025–26, citing AI homework tools). Its
   exposure to API Policy §3(i) ("technologies not approved by Instructure") is no better than
   token mode.
2. **Instructure AUP (2025-08-12) binds students:** publicly supported interfaces only, no
   circumventing authentication measures, no sharing credentials. Canvas sessions are sliding
   (idle expiry refreshed by requests), so **background polling would keep sessions alive
   indefinitely** — defeating the institution's idle timeout. → No background sync, ever.
3. **Reads have instructor-visible side effects** (true for every authenticated client, token mode
   too): PageViews, `last_activity_at` / `total_activity_time`, AssetUserAccess, and **file
   downloads can complete "must view" module requirements**. → Never auto-download files; make it
   an explicit, disclosed student action (applied to the Canvas token source in v0.1 as well).
4. UofT InfoSec advises using a browser with few extensions for school portals; CWS requires
   disclosure *before* install. Promotion to UofT students carries medium risk; Instructure
   objection risk rises with "AI + Canvas" marketing and user count.

## Recommendation

- **v0.2:** do **not** publish the Connector. Improve folder + calendar-feed (distributable, lowest
  risk); apply for a UofT Developer Key and ask for a written policy clarification; make
  `weekmark-canvas` transport-agnostic (one GET allow-list, pagination and mapping behind a
  `CanvasTransport` trait: token now, OAuth next, extension possibly later).
- **Later, only if** UofT/Instructure confirm in writing (or legal review concludes) that
  student-initiated, read-only, session-based export of one's own course data to a personal study
  tool is acceptable — and no Developer Key is available — ship it as *experimental, opt-in,
  foreground-only* (sync only while the student has Canvas open and clicks Sync; files only on
  explicit request; one Canvas origin; open source; signed releases). If a Developer Key is
  granted, use OAuth and drop the Connector for that school.
