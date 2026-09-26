// End-to-end spike tests: Playwright's bundled Chromium (Chrome for Testing) + the unpacked extension
// + the Node native host + the local mock Canvas. Never touches a real browser profile or a real Canvas.
//
//   npm test            (or: node tests/run.mjs [--keep-tmp] [--headed])
//
// Results: printed table + out/<run>/results.json. Host output: out/<run>/<variant>/.
import { chromium } from 'playwright';
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { createMockCanvas } from '../mock-canvas/server.js';
import { SPIKE_DIR, HOST_NAME, extensionId, hostManifest, writeLauncher } from '../native-host/manifest-util.mjs';

const RUN = new Date().toISOString().replace(/[:.]/g, '-');
const TMP = path.join(SPIKE_DIR, '.tmp', RUN);
const OUT = path.join(SPIKE_DIR, 'out', RUN);
const KEEP_TMP = process.argv.includes('--keep-tmp');
const HEADED = process.argv.includes('--headed');
const EXT_ID = extensionId();
const MiB = 1024 * 1024;
fs.mkdirSync(TMP, { recursive: true });
fs.mkdirSync(OUT, { recursive: true });

// ---------------- result bookkeeping ----------------
const QUESTIONS = {
  T1: 'Content-script same-origin GET carries the session cookie (200 with data; 401 when logged out)',
  T2: "Service-worker fetch(credentials:'include') carries the session cookie: SameSite=Lax and None, with and without a Canvas tab (background sync)",
  T3: 'Link-header pagination followed correctly across 3 pages',
  T4: 'Native messaging round trip; ext->host ~5 MB (chunked and unchunked, find the limit); host->ext ~2 MB vs documented 1 MB',
  T5: 'File download: file object -> 302 to second origin with signature -> bytes, from content script and service worker, with/without host permission for the second origin; desktop download via signed URL only',
  T6: 'GET-only guard: attempted writes throw in the extension and the mock logs zero non-GET API requests',
  T7: 'Logged-out / expired session: 401 -> structured needs_login message to the host',
  T8: "while(1); JSON prefix stripping",
};
const R = {};
const t = (id) => (R[id] ??= { id, question: QUESTIONS[id], checks: [], notes: [], evidence: [], blocked: null });
function check(id, name, ok, detail) {
  t(id).checks.push({ name, ok: !!ok, detail });
  console.log(`  ${ok ? 'ok  ' : 'FAIL'} [${id}] ${name}${detail !== undefined ? ' :: ' + short(detail) : ''}`);
}
const note = (id, s) => {
  t(id).notes.push(s);
  console.log(`  note [${id}] ${s}`);
};
const evidence = (id, label, obj) => t(id).evidence.push(`${label}: ${short(obj, 600)}`);
const short = (v, n = 220) => {
  const s = typeof v === 'string' ? v : JSON.stringify(v);
  return s && s.length > n ? s.slice(0, n) + '...' : s;
};
const sha256 = (b) => crypto.createHash('sha256').update(b).digest('hex');
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// ---------------- infrastructure ----------------
const mock = createMockCanvas({ sameSite: 'Lax' });
const { canvasOrigin: C, filesOrigin: F } = await mock.start();
console.log(`run ${RUN}\nmock canvas ${C}\nmock files  ${F}\nextension id ${EXT_ID}\n`);

const apiReqs = (mk) => mock.since(mk, (e) => e.server === 'canvas' && e.path.startsWith('/api/v1/'));

async function launch(variant, { hostPermissions, prefs } = {}) {
  const dir = path.join(TMP, variant);
  const ext = path.join(dir, 'extension');
  fs.cpSync(path.join(SPIKE_DIR, 'extension'), ext, { recursive: true });
  // TEST-ONLY manifest variant: grant the mock origins at install time via host_permissions, because the
  // chrome.permissions.request() prompt cannot be clicked in automation. The shipped manifest only has
  // optional_host_permissions.
  const m = JSON.parse(fs.readFileSync(path.join(ext, 'manifest.json'), 'utf8'));
  m.host_permissions = hostPermissions;
  fs.writeFileSync(path.join(ext, 'manifest.json'), JSON.stringify(m, null, 2));
  const profile = path.join(dir, 'profile');
  fs.mkdirSync(path.join(profile, 'NativeMessagingHosts'), { recursive: true });
  if (prefs) {
    fs.mkdirSync(path.join(profile, 'Default'), { recursive: true });
    fs.writeFileSync(path.join(profile, 'Default', 'Preferences'), JSON.stringify(prefs));
  }
  const launcher = writeLauncher(path.join(dir, 'run-host.sh'), { env: { STUDENTOS_CONNECTOR_OUT: OUT } });
  fs.writeFileSync(path.join(profile, 'NativeMessagingHosts', `${HOST_NAME}.json`), JSON.stringify(hostManifest(launcher, EXT_ID), null, 2));
  const ctx = await chromium.launchPersistentContext(profile, {
    channel: 'chromium',
    headless: !HEADED,
    args: [`--disable-extensions-except=${ext}`, `--load-extension=${ext}`],
  });
  let [sw] = ctx.serviceWorkers();
  if (!sw) sw = await ctx.waitForEvent('serviceworker', { timeout: 15000 });
  await sw.evaluate((runId) => chrome.storage.local.set({ runId, autoSyncOnVisit: false }), variant);
  openContexts.add(ctx);
  ctx.on('close', () => openContexts.delete(ctx));
  browserVersion ??= ctx.browser()?.version() ?? null;
  return { ctx, sw, variant, extId: new URL(sw.url()).host, browserVersion };
}

async function configureViaOptionsPage(ctx) {
  const p = await ctx.newPage();
  await p.goto(`chrome-extension://${EXT_ID}/options.html`);
  await p.fill('#canvas-url', C);
  await p.click('#save'); // real user gesture -> chrome.permissions.request()
  await p.waitForFunction(() => document.body.dataset.state, null, { timeout: 15000 });
  const state = await p.evaluate(() => document.body.dataset.state);
  const out = JSON.parse(await p.textContent('#out'));
  await p.close();
  return { state, out };
}

async function login(ctx) {
  const p = await ctx.newPage();
  await p.goto(C + '/login');
  await p.fill('#pseudonym_session_unique_id', mock.login.unique_id);
  await p.fill('#pseudonym_session_password', mock.login.password);
  await Promise.all([p.waitForURL(C + '/'), p.click('#login_submit')]);
  await p.waitForSelector('#dashboard');
  return p;
}

async function canvasPage(ctx) {
  const existing = ctx.pages().find((p) => p.url().startsWith(C));
  if (existing) return existing;
  const p = await ctx.newPage();
  await p.goto(C + '/');
  await p.waitForLoadState('load');
  return p;
}

async function closeCanvasPages(ctx) {
  for (const p of ctx.pages()) if (p.url().startsWith(C)) await p.close();
}

async function crossSiteProbe(ctx, tag) {
  // Control: a normal web page on a DIFFERENT site sends a credentialed no-cors request to the Canvas API.
  const p = await ctx.newPage();
  await p.goto(F + '/__probe');
  const mk = mock.mark();
  await p.evaluate(async (u) => {
    try {
      await fetch(u, { mode: 'no-cors', credentials: 'include' });
    } catch {}
  }, `${C}/api/v1/users/self?probe=${tag}`);
  await sleep(200);
  await p.close();
  const e = apiReqs(mk).find((x) => x.search.includes(`probe=${tag}`));
  return e ? { cookieSent: e.sessionCookieSent, secFetchSite: e.secFetchSite } : { cookieSent: null };
}

function readOut(rel) {
  return fs.readFileSync(path.join(OUT, rel), 'utf8');
}

const sw_ = (h, fn, arg) => h.sw.evaluate(fn, arg);
const openContexts = new Set();
let browserVersion = null;
let fatal = null;
try {

// =====================================================================================
// Launch A: test manifest grants BOTH mock origins (Canvas + file store)
// =====================================================================================
console.log('== launch A: host_permissions = [http://localhost/*, http://127.0.0.1/*] ==');
const A = await launch('A-both-origins', { hostPermissions: ['http://localhost/*', 'http://127.0.0.1/*'] });
console.log(`   browser ${A.browserVersion}`);
check('T4', 'extension ID from manifest "key" is stable (matches computed ID)', A.extId === EXT_ID, A.extId);

// --- native host round trip first: proves Chromium found the host manifest in <user-data-dir>/NativeMessagingHosts
{
  const t0 = Date.now();
  const r = await sw_(A, () => self.spike.nativeHello());
  check('T4', 'native host found via <temp-user-data-dir>/NativeMessagingHosts and hello round-trips', r.type === 'hello-ack', { ms: Date.now() - t0, callerOrigin: r.callerOrigin, runDir: r.runDir });
  evidence('T4', 'hello-ack', r);
}

// --- options page: enter Canvas URL, permissions.request, register content script, probe (still logged out)
{
  const { state, out } = await configureViaOptionsPage(A.ctx);
  const reg = await sw_(A, () => chrome.scripting.getRegisteredContentScripts());
  check('T1', 'options page: permissions.request -> configure -> content script registered dynamically for the granted origin', out.ok && out.contentScriptRegistered && reg[0]?.matches?.[0] === C + '/*', reg.map((r) => r.matches));
  check('T7', 'Canvas detection probe while logged out classifies the origin as Canvas + needs login', state === 'needs-login', out.probe);
  evidence('T1', 'options page result', out);
}

// --- T7a: SW sync while never logged in
{
  const r = await sw_(A, () => self.spike.swSync('test-logged-out'));
  check('T7', 'SW sync with no session -> 401 -> needsLogin result', r.needsLogin === true && r.status === 401, { status: r.status, needsLogin: r.needsLogin });
  const written = r.hostAck?.written;
  const onDisk = written ? JSON.parse(fs.readFileSync(path.join(OUT, written), 'utf8')) : null;
  check('T7', 'needs_login message reached the native host and was written to out/', onDisk?.type === 'needs_login' && onDisk.context === 'service-worker', written);
  evidence('T7', 'needs_login file (service worker, never logged in)', onDisk);
}

// --- log in with SameSite=Lax
mock.set({ sameSite: 'Lax' });
let page = await login(A.ctx);
{
  const cookies = await A.ctx.cookies(C);
  const sess = cookies.find((c) => c.name === '_normandy_session');
  const csrf = cookies.find((c) => c.name === '_csrf_token');
  evidence('T2', 'cookies after Lax login', cookies.map(({ name, httpOnly, secure, sameSite }) => ({ name, httpOnly, secure, sameSite })));
  check('T1', 'login sets HttpOnly session cookie + non-HttpOnly _csrf_token', sess?.httpOnly === true && csrf?.httpOnly === false, { sess: sess && { httpOnly: sess.httpOnly, sameSite: sess.sameSite }, csrf: csrf && { httpOnly: csrf.httpOnly } });
}

// --- T1 + T3: content-script sync (same-origin)
{
  const mk = mock.mark();
  const r = await sw_(A, () => self.spike.csSync('test'));
  const reqs = apiReqs(mk);
  check('T1', 'content-script sync succeeds with real data (logged in)', r.ok && r.summary.user.id === 1001 && r.summary.courses === 8, r.summary && { user: r.summary.user, courses: r.summary.courses, modules: r.summary.modules, planner: r.summary.planner, fileObjects: r.summary.fileObjects });
  const forbidden = (e) => e.path === `/api/v1/courses/${mock.cfg.forbiddenModulesCourse}/modules`;
  check('T1', 'every content-script API request carried the session cookie; all 200 except the deliberately forbidden course (403, recorded as a soft error)', reqs.length > 0 && reqs.every((e) => e.sessionCookieSent && (e.status === 200 || (forbidden(e) && e.status === 403))) && r.summary.errors.length === 1, { requests: reqs.length, statuses: [...new Set(reqs.map((e) => e.status))], softErrors: r.summary.errors, secFetchSite: [...new Set(reqs.map((e) => e.secFetchSite))], originHeader: [...new Set(reqs.map((e) => e.origin))] });
  const snap = r.ack?.written ? JSON.parse(readOut(r.ack.written)) : null;
  check('T1', 'snapshot delivered to native host and written to disk', snap?.courses?.length === 8 && snap.context === 'content-script', r.ack?.written);
  evidence('T1', 'content-script request headers seen by mock', reqs.slice(0, 2).map(({ path: p, sessionCookieSent, secFetchSite, secFetchMode, origin, referer }) => ({ p, sessionCookieSent, secFetchSite, secFetchMode, origin, referer })));
  const courseReqs = reqs.filter((e) => e.path === '/api/v1/courses');
  check('T3', 'content script: /courses fetched as exactly 3 pages via Link rel="next" (server clamps per_page=3, opaque bookmark tokens)', courseReqs.length === 3 && r.summary.coursePages.length === 3 && r.summary.stats.linkHeaderReadable === true, courseReqs.map((e) => e.page));
  check('T3', 'all 8 courses collected, no duplicates', r.summary.courses === 8 && new Set(snap.courses.map((c) => c.id)).size === 8, snap.courses.map((c) => c.id));
  evidence('T3', 'content-script course page URLs', r.summary.coursePages);
}

// --- T1 extra: auto-sync on visit (content script -> SW -> host)
{
  await sw_(A, () => chrome.storage.local.set({ autoSyncOnVisit: true }).then(() => chrome.storage.local.remove('lastSync')));
  await page.goto(C + '/');
  let last = null;
  for (let i = 0; i < 40 && !last; i++) {
    await sleep(250);
    last = await sw_(A, () => chrome.storage.local.get('lastSync').then((s) => s.lastSync || null));
  }
  check('T1', 'opportunistic sync on Canvas page visit (content script -> service worker -> host)', last?.context === 'content-script' && last?.trigger === 'visit', last);
  await sw_(A, () => chrome.storage.local.set({ autoSyncOnVisit: false }));
}

// --- T2 (Lax): service worker, tab open / no tab / alarm
async function t2Suite(h, label) {
  const results = {};
  let mk = mock.mark();
  await canvasPage(h.ctx);
  let r = await sw_(h, () => self.spike.swSync('test-tab-open'));
  let reqs = apiReqs(mk);
  results.tabOpen = { ok: r.ok, needsLogin: !!r.needsLogin, cookieOnAll: reqs.length > 0 && reqs.every((e) => e.sessionCookieSent), n: reqs.length };
  results.headers = {
    secFetchSite: [...new Set(reqs.map((e) => e.secFetchSite))],
    originHeader: [...new Set(reqs.map((e) => e.origin))],
    secFetchMode: [...new Set(reqs.map((e) => e.secFetchMode))],
    referer: [...new Set(reqs.map((e) => e.referer))],
  };
  results.swPagination = r.summary ? { pages: r.summary.coursePages.length, courses: r.summary.courses, linkHeaderReadable: r.summary.stats.linkHeaderReadable, rateLimitRemaining: r.summary.stats.rateLimitRemaining } : null;

  for (const p of h.ctx.pages()) await p.close(); // no tabs at all
  mk = mock.mark();
  r = await sw_(h, () => self.spike.swSync('test-no-tab'));
  reqs = apiReqs(mk);
  results.noTab = { ok: r.ok, needsLogin: !!r.needsLogin, cookieOnAll: reqs.length > 0 && reqs.every((e) => e.sessionCookieSent), n: reqs.length, pagesOpen: h.ctx.pages().length };
  return results;
}

async function alarmSync(h) {
  for (const p of h.ctx.pages()) await p.close();
  const sched = await sw_(h, () => self.spike.scheduleOneShotAlarm(0.02));
  let res = null;
  const t0 = Date.now();
  for (let i = 0; i < 150 && !res; i++) {
    await sleep(500);
    res = await sw_(h, () => chrome.storage.local.get('lastAlarmResult').then((s) => s.lastAlarmResult || null));
  }
  return { res, requestedDelayMs: sched.scheduledFor - sched.now, waitedMs: Date.now() - t0 };
}

{
  const r = await t2Suite(A, 'Lax');
  check('T2', 'SameSite=Lax: SW sync with Canvas tab open carries the cookie', r.tabOpen.ok && r.tabOpen.cookieOnAll, r.tabOpen);
  check('T2', 'SameSite=Lax: SW sync with NO tabs open at all carries the cookie', r.noTab.ok && r.noTab.cookieOnAll && r.noTab.pagesOpen === 0, r.noTab);
  check('T3', 'service worker: 3 pages followed, Link header readable from SW fetch', r.swPagination?.pages === 3 && r.swPagination.courses === 8 && r.swPagination.linkHeaderReadable === true, r.swPagination);
  evidence('T2', 'SW request headers as seen by server (Lax)', r.headers);
  note('T2', `Server sees SW requests with Sec-Fetch-Site=${r.headers.secFetchSite.join('/')}, Origin header ${r.headers.originHeader.every((o) => o == null) ? 'absent' : r.headers.originHeader.join('/')}, Referer ${r.headers.referer.every((o) => o == null) ? 'absent' : r.headers.referer.join('/')} (content-script requests instead look like same-origin page requests).`);
  const al = await alarmSync(A);
  check('T2', 'SameSite=Lax: chrome.alarms-triggered sync with zero open tabs delivers a snapshot to the host', al.res?.ok && al.res.summary?.courses === 8 && al.res.openTabUrls.length === 0, { ...al, res: al.res && { ok: al.res.ok, openTabUrls: al.res.openTabUrls, written: al.res.ack?.written } });
  note('T2', `one-shot alarm requested ~${Math.round(al.requestedDelayMs)} ms delay (unpacked extension), fired after ~${al.waitedMs} ms of polling`);
  const x = await crossSiteProbe(A.ctx, 'xsite-lax');
  note('T2', `control: a web page on another site (${F}) sending a credentialed no-cors request -> session cookie sent: ${x.cookieSent} (Lax)`);
  evidence('T2', 'control cross-site probe (Lax)', x);
}

// --- T8: while(1); prefix
{
  page = await canvasPage(A.ctx);
  mock.set({ jsonPrefix: 'session-get' });
  const swr = await sw_(A, () => self.spike.rawGet('/api/v1/users/self'));
  const csr = await sw_(A, () => self.spike.csCommand({ cmd: 'cs-raw-get', path: '/api/v1/users/self' }));
  check('T8', 'prefix on (old-Canvas behaviour): SW client strips "while(1);" and parses JSON', swr.ok && swr.rawPrefixed && swr.parsedId === 1001, swr);
  check('T8', 'prefix on: content-script client strips it too', csr.ok && csr.rawPrefixed && csr.parsedId === 1001, csr);
  const full = await sw_(A, () => self.spike.swSync('test-prefix'));
  check('T8', 'full SW sync works with every 200 JSON response prefixed (the one 403 is not prefixed)', full.ok && full.summary.courses === 8 && full.summary.stats.strippedPrefixes === full.summary.stats.requestCount - full.summary.errors.length, full.summary && { stripped: full.summary.stats.strippedPrefixes, requests: full.summary.stats.requestCount, softErrors: full.summary.errors.length });
  mock.set({ jsonPrefix: 'session-get-unless-accept-json' });
  const a1 = await sw_(A, () => self.spike.rawGet('/api/v1/users/self'));
  const a2 = await sw_(A, () => self.spike.rawGet('/api/v1/users/self', '*/*'));
  check('T8', 'variant "unless Accept: application/json": client default Accept avoids the prefix; Accept */* gets it and still parses', a1.ok && !a1.rawPrefixed && a2.ok && a2.rawPrefixed, { defaultAccept: a1.rawPrefixed, starAccept: a2.rawPrefixed });
  mock.set({ jsonPrefix: 'off' });
  note('T8', 'canvas-lms master no longer emits the prefix (render() in application_controller.rb has no while(1)); older versions prefixed session-auth GET JSON. Stripping is kept as a cheap defence.');
}

// --- T4: native messaging sizes
{
  const n5 = 5 * MiB;
  const expect5 = sha256('a'.repeat(n5));
  const u = await sw_(A, (n) => self.spike.nativeSend(n, { chunkChars: Infinity }), n5);
  check('T4', 'ext->host 5 MiB UNCHUNKED single message delivered intact', u.ok && u.ack.chars === n5 && u.ack.sha256 === expect5, { ok: u.ok, ms: u.ms, error: u.error });
  const c = await sw_(A, (n) => self.spike.nativeSend(n, { forceChunk: true, chunkChars: 512 * 1024 }), n5);
  check('T4', 'ext->host 5 MiB CHUNKED (512 KiB chunks) reassembled intact', c.ok && c.ack.chunks === 10 && c.ack.sha256 === expect5, { ok: c.ok, chunks: c.ack?.chunks, ms: c.ms });
  const probes = {};
  for (const n of [32 * MiB, 64 * MiB - 4096, 64 * MiB + 4096]) {
    const r = await sw_(A, (n) => self.spike.nativeSend(n, { chunkChars: Infinity }), n);
    probes[`${(n / MiB).toFixed(3)} MiB`] = r.ok ? `ok (${r.ms} ms)` : `FAILED: ${r.error} (thrownByPostMessage=${r.thrownSynchronouslyByPostMessage})`;
  }
  evidence('T4', 'ext->host single-message probes', probes);
  note('T4', `ext->host single-message probes: ${JSON.stringify(probes)}`);
  const big = await sw_(A, ([n, cc]) => self.spike.nativeSend(n, { forceChunk: true, chunkChars: cc }), [80 * MiB, MiB]);
  check('T4', 'ext->host 80 MiB CHUNKED (1 MiB chunks) delivered - chunking removes the single-message cap', big.ok && big.ack.chars === 80 * MiB, { ok: big.ok, chunks: big.ack?.chunks, ms: big.ms, error: big.error });

  const back = {};
  for (const n of [MiB - 1, MiB, MiB + 1, 2_000_000]) {
    const r = await sw_(A, (n) => self.spike.nativeAskBig(n), n);
    back[n] = r.ok ? `received (${r.messageJsonLength} bytes)` : `FAILED: ${r.error}; disconnect=${JSON.stringify(r.disconnect)}`;
  }
  evidence('T4', 'host->ext message sizes', back);
  check('T4', 'host->ext: <= 1 MiB delivered', back[MiB - 1].startsWith('received') && back[MiB].startsWith('received'), { [MiB - 1]: back[MiB - 1], [MiB]: back[MiB] });
  check('T4', 'host->ext: > 1 MiB (1 MiB + 1 and ~2 MB) is rejected by Chrome and the port is disconnected (host must chunk)', back[MiB + 1].startsWith('FAILED') && back[2_000_000].startsWith('FAILED'), { [MiB + 1]: back[MiB + 1], 2000000: back[2_000_000] });
  const again = await sw_(A, () => self.spike.nativeHello());
  check('T4', 'extension reconnects (new host process) after the over-limit disconnect', again.type === 'hello-ack', { newPid: again.pid });
}

// --- T5 in launch A (second origin permitted)
async function t5Suite(h, label) {
  const out = {};
  await canvasPage(h.ctx);
  mock.set({ filesCors: 'none' });
  for (const id of [5001, 5002]) {
    const exp = mock.files.get(id).sha256;
    const s = await sw_(h, ([id]) => self.spike.swDownload(id, { forwardBytes: true, handoffUrl: true }), [id]);
    out[`SW ${id} filesCors=none`] = {
      ok: !!s.ok && s.sha256 === exp,
      size: s.size,
      redirected: s.redirected,
      finalOrigin: s.finalUrl ? new URL(s.finalUrl).origin : null,
      error: s.error,
      hostBytesMatch: s.hostBytesAck?.sha256Match ?? null,
      desktopSignedUrl: s.hostUrlAck ? { status: s.hostUrlAck.status, match: s.hostUrlAck.sha256Match } : null,
      ms: s.ms,
    };
    const c = await sw_(h, ([id]) => self.spike.csDownload(id, { forwardBytes: true }), [id]);
    out[`CS ${id} filesCors=none`] = { ok: !!c.ok && c.sha256 === exp, size: c.size, error: c.error, hostBytesMatch: c.hostBytesAck?.sha256Match ?? null };
  }
  mock.set({ filesCors: '*' });
  for (const id of [5001, 5002]) {
    const exp = mock.files.get(id).sha256;
    const s = await sw_(h, ([id]) => self.spike.swDownload(id, { forwardBytes: false, handoffUrl: true }), [id]);
    out[`SW ${id} filesCors=*`] = { ok: !!s.ok && s.sha256 === exp, error: s.error, desktopSignedUrl: s.hostUrlAck ? { status: s.hostUrlAck.status, match: s.hostUrlAck.sha256Match } : null };
    const c = await sw_(h, ([id]) => self.spike.csDownload(id, { forwardBytes: true }), [id]);
    out[`CS ${id} filesCors=*`] = { ok: !!c.ok && c.sha256 === exp, size: c.size, error: c.error, hostBytesMatch: c.hostBytesAck?.sha256Match ?? null };
  }
  mock.set({ filesCors: 'none' });
  return out;
}

{
  const m = await t5Suite(A, 'A');
  evidence('T5', 'matrix, 2nd origin PERMITTED (launch A)', m);
  check('T5', '[2nd-origin permission] SW: small + 5 MB file via 302 to second origin, sha256 verified (no CORS headers needed)', m['SW 5001 filesCors=none'].ok && m['SW 5002 filesCors=none'].ok && m['SW 5002 filesCors=none'].finalOrigin === F, { small: m['SW 5001 filesCors=none'], large: m['SW 5002 filesCors=none'] });
  check('T5', '[2nd-origin permission] bytes forwarded SW -> native host (chunked) match', m['SW 5001 filesCors=none'].hostBytesMatch && m['SW 5002 filesCors=none'].hostBytesMatch);
  check('T5', '[2nd-origin permission] desktop (Node fetch, no cookies) downloads using only the signed URL the SW observed (response.url)', m['SW 5002 filesCors=none'].desktopSignedUrl?.status === 200 && m['SW 5002 filesCors=none'].desktopSignedUrl.match, m['SW 5002 filesCors=none'].desktopSignedUrl);
  check('T5', '[content script] file store WITHOUT CORS headers: cross-origin redirect is blocked by CORS even though the extension has host permission for it', !m['CS 5001 filesCors=none'].ok && !m['CS 5002 filesCors=none'].ok, { error: m['CS 5002 filesCors=none'].error });
  check('T5', '[content script] file store WITH Access-Control-Allow-Origin:* works (small + 5 MB), bytes forwarded to host match', m['CS 5001 filesCors=*'].ok && m['CS 5002 filesCors=*'].ok && m['CS 5002 filesCors=*'].hostBytesMatch, { small: m['CS 5001 filesCors=*'], large: m['CS 5002 filesCors=*'] });
}

// --- T5 desktop-only checks (plain Node, no browser)
{
  const f = mock.files.get(5002);
  const apiUrl = `${C}/files/5002/download?download_frd=1&verifier=${f.uuid}`;
  const r1 = await fetch(apiUrl, { redirect: 'manual' });
  check('T5', 'desktop: Canvas file url (verifier) WITHOUT cookies does not yield bytes (mock policy: session required -> 302 /login)', r1.status === 302 && r1.headers.get('location') === '/login', { status: r1.status, location: r1.headers.get('location') });
  const signed = mock.signedUrlFor(5002);
  const r2 = await fetch(signed);
  const b2 = Buffer.from(await r2.arrayBuffer());
  const r3 = await fetch(signed.replace(/sig=([^&]{4})/, 'sig=AAAA'));
  check('T5', 'desktop: signed file-store URL alone -> 200 + correct 5 MB bytes; tampered signature -> 403', r2.status === 200 && sha256(b2) === f.sha256 && r3.status === 403, { signed: r2.status, bytes: b2.length, tampered: r3.status });
  const fileReqs = mock.log.filter((e) => e.server === 'files' && e.path.startsWith('/files/'));
  check('T5', 'no cookie was ever sent to the file-store origin', fileReqs.length > 0 && fileReqs.every((e) => e.cookieNames.length === 0), { requests: fileReqs.length });
}

// --- T6: GET-only guard
{
  await canvasPage(A.ctx);
  const s = await sw_(A, () => self.spike.tryWrites());
  const c = await sw_(A, () => self.spike.csCommand({ cmd: 'cs-try-writes' }));
  const blockedAll = (o, except = []) => Object.entries(o).every(([k, v]) => except.includes(k) || v.startsWith('GetOnlyViolation'));
  check('T6', 'service worker: POST / DELETE(Request) / X-CSRF-Token all throw GetOnlyViolation before any network I/O', blockedAll(s, ['SOS client getJson (GET, control)']) && s['SOS client getJson (GET, control)'].startsWith('NOT BLOCKED'), s);
  check('T6', 'content script: POST / PUT(Request) / XHR POST / sendBeacon / X-CSRF-Token all throw GetOnlyViolation', blockedAll(c), c);
}

// --- T7: expired session + logged out, both contexts
{
  await canvasPage(A.ctx);
  mock.expireAllSessions();
  const s = await sw_(A, () => self.spike.swSync('test-expired'));
  const c = await sw_(A, () => self.spike.csSync('test-expired'));
  check('T7', 'server-side expired session (cookie still present): SW -> needsLogin + host ack', s.needsLogin && s.status === 401 && s.hostAck?.kind === 'needs_login', { status: s.status, written: s.hostAck?.written });
  check('T7', 'server-side expired session: content script -> needsLogin + host ack', c.needsLogin && c.status === 401 && c.hostAck?.kind === 'needs_login', { status: c.status, written: c.hostAck?.written });
  const invalid = mock.log.filter((e) => e.sessionInvalidReason === 'expired').length;
  note('T7', `mock saw ${invalid} requests with an expired session cookie`);

  await A.ctx.clearCookies();
  const s2 = await sw_(A, () => self.spike.swSync('test-logged-out'));
  const c2 = await sw_(A, () => self.spike.csSync('test-logged-out'));
  check('T1', 'content script while logged out -> 401 (needsLogin)', c2.needsLogin && c2.status === 401, { status: c2.status });
  check('T7', 'cookies cleared (logged out): SW and content script both -> needsLogin delivered to host', s2.needsLogin && c2.needsLogin && s2.hostAck?.kind === 'needs_login' && c2.hostAck?.kind === 'needs_login');
  const nl = JSON.parse(fs.readFileSync(path.join(OUT, c2.hostAck.written), 'utf8'));
  evidence('T7', 'needs_login written by host (content script, logged out)', nl);
}

// --- T7: permission errors (logged in, but a course's modules are hidden) must NOT be treated as logout
{
  mock.set({ sameSite: 'Lax' });
  await login(A.ctx);
  const out = {};
  for (const legacy of [false, true]) {
    mock.set({ legacyUnauthorized401: legacy });
    const r = await sw_(A, () => self.spike.swSync('test-permission-error'));
    out[legacy ? 'legacy401' : 'current403'] = { ok: r.ok, needsLogin: !!r.needsLogin, errors: r.summary?.errors, courses: r.summary?.courses };
  }
  mock.set({ legacyUnauthorized401: false });
  check('T7', 'logged in but one course forbidden (403 now / 401 "unauthorized" in old Canvas): sync completes, error recorded, NOT needs_login', Object.values(out).every((o) => o.ok && !o.needsLogin && o.courses === 8 && o.errors?.length === 1), out);
}

// --- T2 SameSite=None and Strict (re-login in the same profile)
for (const ss of ['None', 'Strict']) {
  mock.set({ sameSite: ss });
  await A.ctx.clearCookies();
  await login(A.ctx);
  const sess = (await A.ctx.cookies(C)).find((c) => c.name === '_normandy_session');
  if (ss === 'None') check('T2', 'SameSite=None requires Secure: Chromium stored the Secure cookie on http://localhost (localhost counts as secure)', sess?.secure === true && sess?.sameSite === 'None', sess && { secure: sess.secure, sameSite: sess.sameSite });
  else note('T2', `Strict cookie stored: ${!!sess}`);
  const r = await t2Suite(A, ss);
  check('T2', `SameSite=${ss}: SW sync with Canvas tab open carries the cookie`, r.tabOpen.ok && r.tabOpen.cookieOnAll, r.tabOpen);
  check('T2', `SameSite=${ss}: SW sync with NO tabs open carries the cookie`, r.noTab.ok && r.noTab.cookieOnAll, r.noTab);
  if (ss === 'None') {
    const al = await alarmSync(A);
    check('T2', 'SameSite=None: alarm-triggered sync with zero open tabs', al.res?.ok && al.res.openTabUrls.length === 0, { ok: al.res?.ok, tabs: al.res?.openTabUrls });
  }
  const x = await crossSiteProbe(A.ctx, `xsite-${ss.toLowerCase()}`);
  note('T2', `control: cross-site web page credentialed request -> cookie sent: ${x.cookieSent} (SameSite=${ss})`);
  evidence('T2', `control cross-site probe (${ss})`, x);
}
await A.ctx.close();

// =====================================================================================
// Launch B: test manifest grants ONLY the Canvas origin (no permission for the file store)
// =====================================================================================
console.log('\n== launch B: host_permissions = [http://localhost/*] only ==');
mock.set({ sameSite: 'None', filesCors: 'none' });
const B = await launch('B-canvas-only', { hostPermissions: ['http://localhost/*'] });
{
  const { state } = await configureViaOptionsPage(B.ctx);
  await login(B.ctx);
  const r = await sw_(B, () => self.spike.swSync('test'));
  check('T2', 'Canvas-only permission: SW API sync works (SameSite=None)', r.ok && r.summary.courses === 8, { configured: state, ok: r.ok });
  const m = await t5Suite(B, 'B');
  evidence('T5', 'matrix, 2nd origin NOT permitted (launch B)', m);
  check('T5', '[no 2nd-origin permission] SW: redirect to file store without CORS headers FAILS (CORS applies to the redirected hop)', !m['SW 5001 filesCors=none'].ok && !m['SW 5002 filesCors=none'].ok, { error: m['SW 5002 filesCors=none'].error });
  check('T5', "[no 2nd-origin permission] SW with ACAO:* still FAILS (credentials:'include' forbids wildcard ACAO) -> no signed URL for the desktop", !m['SW 5002 filesCors=*'].ok, { error: m['SW 5002 filesCors=*'].error });
  check('T5', '[no 2nd-origin permission] content script behaves exactly as in launch A (fails w/o CORS, works with ACAO:*)', !m['CS 5002 filesCors=none'].ok && m['CS 5002 filesCors=*'].ok, { none: m['CS 5002 filesCors=none'].error, star: m['CS 5002 filesCors=*'].ok });
  // Pin down WHY the SW fails with ACAO:*: fetch the signed URL directly with each credentials mode.
  const direct = {};
  for (const cors of ['*', 'none']) {
    mock.set({ filesCors: cors });
    for (const cred of ['include', 'omit']) direct[`ACAO=${cors} credentials=${cred}`] = await sw_(B, ([u, c]) => self.spike.swFetchUrl(u, c), [mock.signedUrlFor(5001), cred]);
  }
  mock.set({ filesCors: 'none' });
  evidence('T5', 'SW direct fetch of signed URL, no 2nd-origin permission', direct);
  check('T5', "[no 2nd-origin permission] cause confirmed: signed URL + ACAO:* works with credentials:'omit' but not 'include'; without ACAO neither works", direct['ACAO=* credentials=omit'].ok && !direct['ACAO=* credentials=include'].ok && !direct['ACAO=none credentials=omit'].ok, Object.fromEntries(Object.entries(direct).map(([k, v]) => [k, v.ok ? v.status : v.error])));
}
await B.ctx.close();

// =====================================================================================
// Launch C: third-party cookies blocked in the profile (robustness of SW fetch)
// =====================================================================================
console.log('\n== launch C: third-party cookies blocked via profile prefs ==');
const C3 = await launch('C-3pc-blocked', {
  hostPermissions: ['http://localhost/*', 'http://127.0.0.1/*'],
  prefs: { profile: { cookie_controls_mode: 1, block_third_party_cookies: true } },
});
for (const ss of ['None', 'Lax']) {
  mock.set({ sameSite: ss });
  await C3.ctx.clearCookies();
  if (ss === 'None') await configureViaOptionsPage(C3.ctx);
  await login(C3.ctx);
  const x = await crossSiteProbe(C3.ctx, `3pc-${ss.toLowerCase()}`);
  const r = await t2Suite(C3, ss);
  const effective = ss === 'None' ? x.cookieSent === false : null;
  if (ss === 'None') note('T2', `3PC-blocked profile: control cross-site request with SameSite=None cookie -> cookie sent: ${x.cookieSent} (false = the blocking pref is in effect)`);
  check('T2', `third-party cookies BLOCKED, SameSite=${ss}: SW sync with no tabs still carries the cookie`, r.noTab.ok && r.noTab.cookieOnAll, { ...r.noTab, blockingPrefEffective: effective });
}
await C3.ctx.close();

// =====================================================================================
// Global checks, cleanup, report
// =====================================================================================
{
  const nonGet = mock.nonGetApiRequests();
  const csrf = mock.log.filter((e) => e.csrfHeaderSent);
  check('T6', 'mock logged ZERO non-GET /api/v1 requests over the whole run (all launches)', nonGet.length === 0, { nonGet: nonGet.length, totalLogged: mock.log.length });
  check('T6', 'mock never received an X-CSRF-Token header', csrf.length === 0, csrf.length);
  const src = ['sw.js', 'content.js', 'options.js', 'lib/canvas-core.js', 'lib/native.js'].map((f) => [f, fs.readFileSync(path.join(SPIKE_DIR, 'extension', f), 'utf8')]);
  const cookieReads = src.filter(([, s]) => /document\.cookie|chrome\.cookies/.test(s)).map(([f]) => f);
  check('T6', 'static: no extension file reads document.cookie / chrome.cookies (the _csrf_token is never touched)', cookieReads.length === 0, cookieReads);
}
} catch (e) {
  fatal = e;
  console.error('FATAL', e);
} finally {
  for (const c of openContexts) await c.close().catch(() => {});
  await mock.stop().catch(() => {});
}

// verify the host processes are gone
await sleep(500);
let leftovers = '';
try {
  leftovers = execFileSync('pgrep', ['-fl', 'native-host/host.mjs'], { encoding: 'utf8' });
} catch {}
let chromeLeft = '';
try {
  chromeLeft = execFileSync('pgrep', ['-fl', TMP], { encoding: 'utf8' });
} catch {}
console.log(`\nleftover host processes: ${leftovers.trim() || 'none'}; leftover browser processes for this run: ${chromeLeft.trim() ? 'YES' : 'none'}`);
if (!KEEP_TMP) fs.rmSync(TMP, { recursive: true, force: true });

// ---------------- report ----------------
const rows = Object.values(R).sort((a, b) => a.id.localeCompare(b.id, 'en', { numeric: true }));
for (const r of rows) r.result = r.blocked ? 'blocked' : r.checks.every((c) => c.ok) ? 'pass' : 'fail';
const report = {
  run: RUN,
  browser: `Chrome for Testing ${browserVersion} (Playwright ${JSON.parse(fs.readFileSync(path.join(SPIKE_DIR, 'node_modules/playwright/package.json'), 'utf8')).version}), headless=${!HEADED}`,
  extensionId: EXT_ID,
  node: process.version,
  results: rows,
  leftoverProcesses: { host: leftovers.trim() || null, browser: chromeLeft.trim() || null },
  fatal: fatal ? String(fatal.stack || fatal) : null,
};
fs.writeFileSync(path.join(OUT, 'results.json'), JSON.stringify(report, null, 2));
console.log('\n| Test | Result | Checks |\n|---|---|---|');
for (const r of rows) console.log(`| ${r.id} | ${r.result} | ${r.checks.filter((c) => c.ok).length}/${r.checks.length} |`);
console.log(`\nresults: ${path.relative(SPIKE_DIR, path.join(OUT, 'results.json'))}`);
process.exit(!fatal && rows.every((r) => r.result === 'pass') ? 0 : 1);
