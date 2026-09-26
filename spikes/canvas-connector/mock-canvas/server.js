// Local mock of the parts of Canvas LMS the connector spike touches.
//
// Two HTTP servers in one process:
//   canvas origin  http://localhost:<port>       login form, session cookie, /api/v1/*, /files/:id/download
//   files origin   http://127.0.0.1:<filesPort>  signed-URL byte server (stands in for Canvas's file store)
//
// Behaviour modelled on canvas-lms source (github.com/instructure/canvas-lms, read for reference only):
//   * session cookie "_normandy_session" (config/initializers/session_store.rb: SameSite=None + Secure when
//     force_ssl; HttpOnly via the Rails cookie store default; expire_after 1 day)
//   * "_csrf_token" cookie holding a masked token, NOT HttpOnly on the main host
//     (lib/canvas/request_forgery_protection.rb + gems/canvas_breach_mitigation)
//   * non-GET requests with session auth need a valid X-CSRF-Token
//   * 401 body {"status":"unauthenticated","errors":[{"message":"user authorization required"}]}
//     plus WWW-Authenticate: Bearer realm="canvas-lms" (lib/authentication_methods.rb)
//   * Link-header pagination with opaque "bookmark:" page tokens (lib/api.rb)
//   * optional "while(1);" JSON prefix: older Canvas versions prefixed session-authenticated GET JSON
//     with it; current master no longer does. Off by default, switchable for T8.
//
// Everything here is synthetic. This file is original code (Apache-2.0).

import http from 'node:http';
import crypto from 'node:crypto';
import { URL, fileURLToPath } from 'node:url';
import { TEST_LOGIN, USER, COURSES, FILES, modulesFor, plannerItems } from './fixtures.js';

const SESSION_COOKIE = '_normandy_session';
const CSRF_COOKIE = '_csrf_token';
const TOKEN_LEN = 32;

export function createMockCanvas(options = {}) {
  const cfg = {
    host: 'localhost',
    port: 0,
    filesHost: '127.0.0.1',
    filesPort: 0,
    sameSite: 'None', // 'Lax' | 'None' | 'Strict' | '' (attribute omitted)
    secure: 'auto', // true | false | 'auto' (= Secure iff SameSite=None, as browsers require)
    jsonPrefix: 'off', // 'off' | 'session-get' | 'session-get-unless-accept-json'
    filesCors: 'none', // 'none' | '*'
    maxPerPage: 3, // server-side clamp for /courses so the client MUST follow Link headers
    forbiddenModulesCourse: 2008, // this course's modules are not visible to the student
    legacyUnauthorized401: false, // true = old Canvas: permission errors as 401 {"status":"unauthorized"} instead of 403
    sessionTtlMs: 24 * 3600 * 1000,
    signedUrlTtlS: 300,
    fileDownloadRequiresSession: true,
    quiet: true,
    ...options,
  };
  const signingKey = crypto.randomBytes(32);
  const sessions = new Map(); // token -> { userId, expiresAt }
  const log = [];
  let seq = 0;
  let rateBucket = 700;
  let rateAt = Date.now();

  const canvasOrigin = () => `http://${cfg.host}:${canvasServer.address().port}`;
  const filesOrigin = () => `http://${cfg.filesHost}:${filesServer.address().port}`;

  // ---------- helpers ----------
  function parseCookies(req) {
    const out = {};
    for (const part of (req.headers.cookie || '').split(';')) {
      const i = part.indexOf('=');
      if (i > 0) out[part.slice(0, i).trim()] = decodeURIComponent(part.slice(i + 1).trim());
    }
    return out;
  }
  function cookieAttrs({ httpOnly }) {
    const secure = cfg.secure === 'auto' ? cfg.sameSite === 'None' : !!cfg.secure;
    return ['Path=/', httpOnly ? 'HttpOnly' : null, secure ? 'Secure' : null, cfg.sameSite ? `SameSite=${cfg.sameSite}` : null]
      .filter(Boolean)
      .join('; ');
  }
  function maskToken(raw) {
    const pad = crypto.randomBytes(TOKEN_LEN);
    const enc = Buffer.alloc(TOKEN_LEN);
    for (let i = 0; i < TOKEN_LEN; i++) enc[i] = pad[i] ^ raw[i];
    return Buffer.concat([pad, enc]).toString('base64');
  }
  function unmaskToken(masked) {
    try {
      const b = Buffer.from(masked || '', 'base64');
      if (b.length !== TOKEN_LEN * 2) return null;
      const out = Buffer.alloc(TOKEN_LEN);
      for (let i = 0; i < TOKEN_LEN; i++) out[i] = b[i] ^ b[i + TOKEN_LEN];
      return out;
    } catch {
      return null;
    }
  }
  function csrfCookieFor(cookies) {
    const existing = unmaskToken(cookies[CSRF_COOKIE]);
    const raw = existing || crypto.randomBytes(TOKEN_LEN);
    const masked = maskToken(raw);
    return { masked, setCookie: `${CSRF_COOKIE}=${encodeURIComponent(masked)}; ${cookieAttrs({ httpOnly: false })}` };
  }
  function csrfValid(cookies, headerOrField) {
    const a = unmaskToken(cookies[CSRF_COOKIE]);
    const b = unmaskToken(headerOrField);
    return !!(a && b && crypto.timingSafeEqual(a, b));
  }
  function currentSession(cookies) {
    const tok = cookies[SESSION_COOKIE];
    if (!tok) return { present: false, valid: false };
    const s = sessions.get(tok);
    if (!s) return { present: true, valid: false, reason: 'unknown' };
    if (s.expiresAt <= Date.now()) return { present: true, valid: false, reason: 'expired' };
    return { present: true, valid: true, session: s };
  }
  function rateLimit(res) {
    const now = Date.now();
    rateBucket = Math.min(700, rateBucket + ((now - rateAt) / 1000) * 10);
    rateAt = now;
    const cost = 0.5 + Math.random();
    rateBucket -= cost;
    res.setHeader('X-Request-Cost', cost.toFixed(6));
    res.setHeader('X-Rate-Limit-Remaining', rateBucket.toFixed(6));
  }
  function sign(fileId, exp) {
    return crypto.createHmac('sha256', signingKey).update(`${fileId}.${exp}`).digest('base64url');
  }
  function signedUrl(file) {
    const exp = Math.floor(Date.now() / 1000) + cfg.signedUrlTtlS;
    return `${filesOrigin()}/files/${file.id}/${encodeURIComponent(file.filename)}?exp=${exp}&sig=${sign(file.id, exp)}`;
  }
  function record(server, req, extra) {
    const u = new URL(req.url, 'http://x');
    const cookies = parseCookies(req);
    const entry = {
      seq: ++seq,
      t: new Date().toISOString(),
      server,
      method: req.method,
      path: u.pathname,
      search: u.search,
      cookieNames: Object.keys(cookies),
      sessionCookieSent: SESSION_COOKIE in cookies,
      csrfHeaderSent: 'x-csrf-token' in req.headers,
      origin: req.headers.origin ?? null,
      secFetchSite: req.headers['sec-fetch-site'] ?? null,
      secFetchMode: req.headers['sec-fetch-mode'] ?? null,
      secFetchDest: req.headers['sec-fetch-dest'] ?? null,
      accept: req.headers.accept ?? null,
      referer: req.headers.referer ?? null,
      ...extra,
    };
    log.push(entry);
    if (!cfg.quiet) process.stderr.write(`[mock ${server}] ${req.method} ${u.pathname}${u.search} session=${entry.sessionCookieSent}\n`);
    return entry;
  }
  function sendJson(req, res, status, body, entry, { session } = {}) {
    let text = JSON.stringify(body);
    const acceptsJson = (req.headers.accept || '').includes('application/json');
    const prefix =
      req.method === 'GET' &&
      session &&
      (cfg.jsonPrefix === 'session-get' || (cfg.jsonPrefix === 'session-get-unless-accept-json' && !acceptsJson));
    if (prefix) text = 'while(1);' + text;
    entry.status = status;
    entry.jsonPrefixed = !!prefix;
    res.writeHead(status, { 'Content-Type': 'application/json; charset=utf-8', 'Cache-Control': 'no-store' });
    res.end(text);
  }
  function html(res, status, body, entry, headers = {}) {
    entry.status = status;
    res.writeHead(status, { 'Content-Type': 'text/html; charset=utf-8', 'Cache-Control': 'no-store', ...headers });
    res.end(`<!doctype html><html><head><meta charset="utf-8"><title>Mock Canvas</title></head><body>${body}</body></html>`);
  }
  function redirect(res, location, entry, headers = {}) {
    entry.status = 302;
    entry.location = location;
    res.writeHead(302, { Location: location, 'Cache-Control': 'no-store', ...headers });
    res.end();
  }
  function readBody(req) {
    return new Promise((resolve) => {
      const chunks = [];
      req.on('data', (c) => chunks.push(c));
      req.on('end', () => resolve(Buffer.concat(chunks).toString('utf8')));
    });
  }
  function fileJson(file) {
    const { bytes, sha256, courseId, ...pub } = file;
    return {
      ...pub,
      folder_id: 90,
      url: `${canvasOrigin()}/files/${file.id}/download?download_frd=1&verifier=${file.uuid}`,
      created_at: '2026-09-03T14:00:00Z',
      updated_at: '2026-09-03T14:00:00Z',
      locked: false,
      hidden: false,
      locked_for_user: false,
      mime_class: file['content-type'] === 'application/pdf' ? 'pdf' : 'file',
    };
  }

  // ---------- canvas origin ----------
  async function handleCanvas(req, res) {
    const u = new URL(req.url, canvasOrigin());
    const cookies = parseCookies(req);
    const sess = currentSession(cookies);
    const entry = record('canvas', req, { sessionValid: sess.valid, sessionInvalidReason: sess.reason ?? null });

    if (u.pathname.startsWith('/api/v1/')) return handleApi(req, res, u, cookies, sess, entry);

    if (req.method === 'GET' && u.pathname === '/login') return redirect(res, '/login/canvas', entry);
    if (req.method === 'GET' && u.pathname === '/login/canvas') {
      const { masked, setCookie } = csrfCookieFor(cookies);
      return html(
        res,
        200,
        `<h1>Log In to Mock Canvas</h1>
<form id="login_form" method="post" action="/login/canvas">
  <input type="hidden" name="authenticity_token" value="${masked}">
  <label>Email <input id="pseudonym_session_unique_id" name="pseudonym_session[unique_id]" autocomplete="off"></label>
  <label>Password <input id="pseudonym_session_password" type="password" name="pseudonym_session[password]" autocomplete="off"></label>
  <button type="submit" id="login_submit">Log In</button>
</form>`,
        entry,
        { 'Set-Cookie': setCookie },
      );
    }
    if (req.method === 'POST' && u.pathname === '/login/canvas') {
      const form = new URLSearchParams(await readBody(req));
      if (!csrfValid(cookies, form.get('authenticity_token'))) return html(res, 422, '<p>InvalidAuthenticityToken</p>', entry);
      if (form.get('pseudonym_session[unique_id]') !== TEST_LOGIN.unique_id || form.get('pseudonym_session[password]') !== TEST_LOGIN.password) {
        return html(res, 400, '<p id="error">Invalid username or password</p>', entry);
      }
      const token = crypto.randomBytes(24).toString('hex');
      sessions.set(token, { userId: USER.id, expiresAt: Date.now() + cfg.sessionTtlMs });
      const { setCookie } = csrfCookieFor({}); // rotate CSRF token on login
      entry.loggedIn = true;
      return redirect(res, '/', entry, {
        'Set-Cookie': [`${SESSION_COOKIE}=${token}; ${cookieAttrs({ httpOnly: true })}`, setCookie],
      });
    }
    if (req.method === 'GET' && u.pathname === '/') {
      if (!sess.valid) return redirect(res, '/login', entry);
      const { setCookie } = csrfCookieFor(cookies);
      return html(res, 200, `<h1 id="dashboard">Dashboard</h1><p>Welcome, ${USER.short_name}.</p>`, entry, { 'Set-Cookie': setCookie });
    }
    const dl = u.pathname.match(/^\/files\/(\d+)\/download$/);
    if (req.method === 'GET' && dl) {
      const file = FILES.get(Number(dl[1]));
      if (!file) return html(res, 404, 'not found', entry);
      if (cfg.fileDownloadRequiresSession && !sess.valid) return redirect(res, '/login', entry);
      if (u.searchParams.get('verifier') !== file.uuid && !sess.valid) return html(res, 401, 'unauthorized', entry);
      return redirect(res, signedUrl(file), entry);
    }
    return html(res, 404, '<p>Page not found</p>', entry);
  }

  function handleApi(req, res, u, cookies, sess, entry) {
    rateLimit(res);
    if (req.method !== 'GET' && req.method !== 'HEAD') {
      entry.nonGetApi = true;
      if (!csrfValid(cookies, req.headers['x-csrf-token'])) {
        return sendJson(req, res, 422, { errors: [{ message: 'InvalidAuthenticityToken' }] }, entry);
      }
      return sendJson(req, res, 405, { errors: [{ message: 'mock canvas is read-only' }] }, entry);
    }
    if (!sess.valid) {
      res.setHeader('WWW-Authenticate', 'Bearer realm="canvas-lms"');
      return sendJson(req, res, 401, { status: 'unauthenticated', errors: [{ message: 'user authorization required' }] }, entry);
    }
    const ok = (body) => sendJson(req, res, 200, body, entry, { session: true });
    const p = u.pathname;
    let m;
    if (p === '/api/v1/users/self') return ok(USER);
    if (p === '/api/v1/courses') {
      const perPage = Math.max(1, Math.min(Number(u.searchParams.get('per_page') || 10), cfg.maxPerPage));
      const pageParam = u.searchParams.get('page') || '1';
      let offset = 0;
      if (pageParam.startsWith('bookmark:')) {
        try {
          offset = JSON.parse(Buffer.from(pageParam.slice(9), 'base64url').toString())[0];
        } catch {
          return sendJson(req, res, 400, { errors: [{ message: 'invalid bookmark' }] }, entry);
        }
      } else offset = (Number(pageParam) - 1) * perPage;
      const items = COURSES.slice(offset, offset + perPage);
      const link = (page) => {
        const q = new URLSearchParams(u.searchParams);
        q.set('page', page);
        q.set('per_page', String(perPage));
        return `${canvasOrigin()}${p}?${q}`;
      };
      const links = [`<${link(pageParam)}>; rel="current"`];
      if (offset + perPage < COURSES.length) links.push(`<${link('bookmark:' + Buffer.from(JSON.stringify([offset + perPage])).toString('base64url'))}>; rel="next"`);
      links.push(`<${link('1')}>; rel="first"`);
      res.setHeader('Link', links.join(','));
      entry.page = pageParam;
      return ok(items);
    }
    if ((m = p.match(/^\/api\/v1\/courses\/(\d+)\/modules$/))) {
      const cid = Number(m[1]);
      if (!COURSES.find((c) => c.id === cid)) return sendJson(req, res, 404, { errors: [{ message: 'The specified resource does not exist.' }] }, entry);
      if (cid === cfg.forbiddenModulesCourse) {
        return sendJson(req, res, cfg.legacyUnauthorized401 ? 401 : 403, { status: 'unauthorized', errors: [{ message: 'user not authorized to perform that action' }] }, entry);
      }
      const includeItems = u.searchParams.getAll('include[]').includes('items');
      const { mods, items } = modulesFor(cid);
      return ok(
        mods.map((mod) => ({
          ...mod,
          items_url: `${canvasOrigin()}/api/v1/courses/${cid}/modules/${mod.id}/items`,
          ...(includeItems
            ? {
                items: items[mod.id].map((it) => ({
                  ...it,
                  module_id: mod.id,
                  html_url: `${canvasOrigin()}/courses/${cid}/modules/items/${it.id}`,
                  ...(it.type === 'File' ? { url: `${canvasOrigin()}/api/v1/courses/${cid}/files/${it.content_id}` } : {}),
                })),
              }
            : {}),
        })),
      );
    }
    if (p === '/api/v1/planner/items') return ok(plannerItems());
    if ((m = p.match(/^\/api\/v1\/(?:courses\/(\d+)\/)?files\/(\d+)$/))) {
      const f = FILES.get(Number(m[2]));
      if (!f || (m[1] && Number(m[1]) !== f.courseId)) return sendJson(req, res, 404, { errors: [{ message: 'The specified resource does not exist.' }] }, entry);
      return ok(fileJson(f));
    }
    return sendJson(req, res, 404, { errors: [{ message: 'The specified resource does not exist.' }] }, entry);
  }

  // ---------- files origin ----------
  function handleFiles(req, res) {
    const u = new URL(req.url, filesOrigin());
    const entry = record('files', req, {});
    const cors = cfg.filesCors === '*' ? { 'Access-Control-Allow-Origin': '*' } : {};
    if (u.pathname === '/__probe') {
      entry.status = 200;
      res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
      return res.end('<!doctype html><title>cross-site probe</title><p>A page on a different site than the mock Canvas.</p>');
    }
    const m = u.pathname.match(/^\/files\/(\d+)\/[^/]+$/);
    const file = m && FILES.get(Number(m[1]));
    const exp = Number(u.searchParams.get('exp'));
    const sig = u.searchParams.get('sig') || '';
    const expected = file ? sign(file.id, exp) : '';
    const sigOk = file && sig.length === expected.length && crypto.timingSafeEqual(Buffer.from(sig), Buffer.from(expected));
    if (!file || !sigOk || exp < Date.now() / 1000) {
      entry.status = 403;
      entry.denied = !file ? 'no-file' : !sigOk ? 'bad-signature' : 'expired';
      res.writeHead(403, { 'Content-Type': 'text/plain', ...cors });
      return res.end('signature invalid or expired');
    }
    entry.status = 200;
    entry.bytes = file.bytes.length;
    res.writeHead(200, {
      'Content-Type': file['content-type'],
      'Content-Length': file.bytes.length,
      'Content-Disposition': `attachment; filename="${file.filename}"`,
      'Cache-Control': 'private, max-age=0',
      ...cors,
    });
    res.end(file.bytes);
  }

  const wrap = (fn) => (req, res) =>
    Promise.resolve(fn(req, res)).catch((e) => {
      process.stderr.write(`[mock] handler error: ${e.stack}\n`);
      if (!res.headersSent) res.writeHead(500);
      res.end();
    });
  const canvasServer = http.createServer(wrap(handleCanvas));
  const filesServer = http.createServer(wrap(handleFiles));

  return {
    cfg,
    log,
    files: FILES,
    login: TEST_LOGIN,
    async start() {
      await new Promise((r) => canvasServer.listen(cfg.port, cfg.host === 'localhost' ? '127.0.0.1' : cfg.host, r));
      await new Promise((r) => filesServer.listen(cfg.filesPort, cfg.filesHost, r));
      return { canvasOrigin: canvasOrigin(), filesOrigin: filesOrigin() };
    },
    async stop() {
      canvasServer.closeAllConnections?.();
      filesServer.closeAllConnections?.();
      await Promise.all([new Promise((r) => canvasServer.close(r)), new Promise((r) => filesServer.close(r))]);
    },
    get canvasOrigin() {
      return canvasOrigin();
    },
    get filesOrigin() {
      return filesOrigin();
    },
    set(patch) {
      Object.assign(cfg, patch);
    },
    expireAllSessions() {
      for (const s of sessions.values()) s.expiresAt = Date.now() - 1;
    },
    signedUrlFor(fileId) {
      return signedUrl(FILES.get(fileId));
    },
    mark() {
      return seq;
    },
    since(mark, filter = () => true) {
      return log.filter((e) => e.seq > mark && filter(e));
    },
    nonGetApiRequests() {
      return log.filter((e) => e.nonGetApi);
    },
  };
}

// ---------- CLI: node mock-canvas/server.js [--port 4580] [--files-port 4581] [--samesite None] ----------
if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const arg = (name, def) => {
    const i = process.argv.indexOf(`--${name}`);
    return i > 0 ? process.argv[i + 1] : def;
  };
  const mock = createMockCanvas({
    port: Number(arg('port', 4580)),
    filesPort: Number(arg('files-port', 4581)),
    sameSite: arg('samesite', 'None'),
    jsonPrefix: arg('json-prefix', 'off'),
    filesCors: arg('files-cors', 'none'),
    quiet: false,
  });
  const { canvasOrigin, filesOrigin } = await mock.start();
  console.log(`mock Canvas:      ${canvasOrigin}   (log in at ${canvasOrigin}/login; test login in mock-canvas/fixtures.js)`);
  console.log(`mock file store:  ${filesOrigin}`);
  const bye = async () => {
    await mock.stop();
    process.exit(0);
  };
  process.on('SIGINT', bye);
  process.on('SIGTERM', bye);
}
