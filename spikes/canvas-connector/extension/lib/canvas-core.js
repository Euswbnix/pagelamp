// Canvas read-only sync core, shared by the content script and the service worker.
// The ONLY difference between the two contexts is the fetch credentials mode:
//   content script  -> same-origin fetch from the Canvas page (credentials: 'same-origin')
//   service worker  -> extension-origin fetch to the Canvas origin (credentials: 'include')
// All network access goes through SOS_GETONLY.getOnlyFetch.
(function (g) {
  'use strict';
  if (g.SOS_CANVAS) return;
  const { getOnlyFetch } = g.SOS_GETONLY;
  const JSON_PREFIX = 'while(1);';

  class NeedsLogin extends Error {
    constructor(status, url, body) {
      super(`Canvas says the session is missing or expired (HTTP ${status})`);
      this.name = 'NeedsLogin';
      this.status = status;
      this.url = url;
      this.body = body;
    }
  }
  class NotCanvas extends Error {
    constructor(message, detail) {
      super(message);
      this.name = 'NotCanvas';
      this.detail = detail;
    }
  }
  class HttpError extends Error {
    constructor(status, url, body) {
      super(`HTTP ${status} for ${url}`);
      this.name = 'HttpError';
      this.status = status;
      this.url = url;
      this.body = body;
    }
  }

  function stripJsonPrefix(text) {
    return text.startsWith(JSON_PREFIX) ? { text: text.slice(JSON_PREFIX.length), stripped: true } : { text, stripped: false };
  }

  // RFC 8288-ish Link header parser: '<url>; rel="next", <url>; rel="first"' -> { next, first }
  function parseLinkHeader(header) {
    const out = {};
    if (!header) return out;
    for (const part of header.split(/,(?=\s*<)/)) {
      const m = /^\s*<([^>]*)>\s*(.*)$/.exec(part);
      if (!m) continue;
      const rel = /(?:^|;)\s*rel\s*=\s*"?([^";]+)"?/i.exec(m[2]);
      if (rel) for (const r of rel[1].trim().split(/\s+/)) out[r.toLowerCase()] = m[1];
    }
    return out;
  }

  function normalizeOrigin(input) {
    let s = String(input || '').trim();
    if (!/^[a-z]+:\/\//i.test(s)) s = 'https://' + s;
    const u = new URL(s);
    const localDev = ['localhost', '127.0.0.1', '[::1]'].includes(u.hostname);
    if (u.protocol !== 'https:' && !(u.protocol === 'http:' && localDev)) throw new Error('Canvas URL must be https:// (http only for localhost testing)');
    return u.origin;
  }

  async function sha256Hex(buf) {
    const d = await crypto.subtle.digest('SHA-256', buf);
    return Array.from(new Uint8Array(d), (b) => b.toString(16).padStart(2, '0')).join('');
  }

  function bytesToBase64(bytes) {
    if (typeof bytes.toBase64 === 'function') return bytes.toBase64();
    let s = '';
    for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode.apply(null, bytes.subarray(i, i + 0x8000));
    return btoa(s);
  }

  function makeClient({ origin, credentials, context, accept = 'application/json' }) {
    const stats = { context, credentials, requests: 0, strippedPrefixes: 0, rateLimitRemaining: null, linkHeaderReadable: null, urls: [] };

    async function getJson(pathOrUrl, { acceptOverride } = {}) {
      const url = new URL(pathOrUrl, origin);
      if (url.origin !== origin) throw new Error(`refusing to follow an off-origin API URL: ${url.origin}`);
      const res = await getOnlyFetch(url.href, {
        credentials,
        cache: 'no-store',
        redirect: 'follow',
        headers: { Accept: acceptOverride ?? accept },
      });
      stats.requests++;
      stats.urls.push(url.pathname + url.search);
      const rl = res.headers.get('X-Rate-Limit-Remaining');
      if (rl != null) stats.rateLimitRemaining = Number(rl);
      const raw = await res.text();
      const { text, stripped } = stripJsonPrefix(raw);
      if (stripped) stats.strippedPrefixes++;
      if (res.status === 401) {
        // Canvas: 401 {"status":"unauthenticated"} = no/expired session. Older Canvas versions also used 401
        // (with status "unauthorized") for permission errors, and current ones use 403 - those are NOT logouts.
        let canvasStatus = null;
        try {
          canvasStatus = JSON.parse(text).status ?? null;
        } catch {}
        if (canvasStatus === null || canvasStatus === 'unauthenticated') throw new NeedsLogin(res.status, url.href, text.slice(0, 300));
        throw Object.assign(new HttpError(res.status, url.href, text.slice(0, 300)), { canvasStatus });
      }
      if (!res.ok) throw new HttpError(res.status, url.href, text.slice(0, 300));
      let json;
      try {
        json = JSON.parse(text);
      } catch {
        // e.g. we were redirected to an HTML login page
        if (res.redirected && /\/login/.test(res.url)) throw new NeedsLogin(res.status, url.href, 'redirected to login page');
        throw new NotCanvas(`non-JSON response from ${url.pathname}`, { status: res.status, contentType: res.headers.get('content-type'), finalUrl: res.url });
      }
      const linkHeader = res.headers.get('Link');
      return { json, status: res.status, links: parseLinkHeader(linkHeader), linkHeader, rawPrefixed: stripped, type: res.type };
    }

    async function getAll(path, { maxPages = 100 } = {}) {
      const items = [];
      const pages = [];
      let next = path;
      while (next) {
        if (pages.length >= maxPages) throw new Error(`pagination did not terminate after ${maxPages} pages`);
        const r = await getJson(next);
        pages.push(next);
        if (!Array.isArray(r.json)) throw new NotCanvas(`expected an array from ${next}`);
        items.push(...r.json);
        if (r.linkHeader != null) stats.linkHeaderReadable = true;
        else if (stats.linkHeaderReadable == null) stats.linkHeaderReadable = false;
        next = r.links.next || null;
      }
      return { items, pages };
    }

    // Detect whether an origin is Canvas and whether we are logged in, without touching any write API.
    async function probe() {
      try {
        const r = await getJson('/api/v1/users/self');
        return { canvas: true, loggedIn: true, userId: r.json.id };
      } catch (e) {
        if (e instanceof NeedsLogin) {
          let looksLikeCanvas = false;
          try {
            looksLikeCanvas = JSON.parse(e.body).status === 'unauthenticated';
          } catch {}
          return { canvas: looksLikeCanvas, loggedIn: false, status: e.status };
        }
        return { canvas: false, loggedIn: false, error: `${e.name}: ${e.message}` };
      }
    }

    async function syncAll() {
      const t0 = Date.now();
      const user = (await getJson('/api/v1/users/self')).json;
      const courses = await getAll('/api/v1/courses?per_page=100&enrollment_state=active&include[]=term');
      const modules = {};
      const fileObjects = {};
      const errors = [];
      // A per-resource permission error (e.g. a course whose Modules page is hidden) must not abort the sync.
      const soft = async (what, fn) => {
        try {
          return await fn();
        } catch (e) {
          if (e instanceof HttpError) {
            errors.push({ what, status: e.status, canvasStatus: e.canvasStatus ?? null });
            return null;
          }
          throw e; // NeedsLogin / NotCanvas / network errors abort
        }
      };
      for (const c of courses.items) {
        const mods = await soft(`modules:${c.id}`, () => getAll(`/api/v1/courses/${c.id}/modules?include[]=items&per_page=100`));
        if (!mods) continue;
        modules[c.id] = mods.items;
        for (const mod of modules[c.id]) {
          for (const it of mod.items || []) {
            // File metadata only; bytes are downloaded separately and on demand.
            if (it.type === 'File' && it.url) {
              const f = await soft(`file:${it.content_id}`, () => getJson(it.url));
              if (f) fileObjects[it.content_id] = f.json;
            }
          }
        }
      }
      const planner = (await getAll('/api/v1/planner/items?per_page=100&start_date=2026-09-01')).items;
      return {
        schema: 'studentos.connector.snapshot/0-spike',
        origin,
        context,
        fetchedAt: new Date().toISOString(),
        user: { id: user.id, name: user.name, short_name: user.short_name },
        courses: courses.items,
        coursePages: courses.pages,
        modules,
        fileObjects,
        planner,
        errors,
        stats: { ...stats, ms: Date.now() - t0 },
      };
    }

    // Follow the file object's url (Canvas origin, 302 -> file store origin) and read the bytes.
    async function downloadFile(fileObj, { includeBase64 = false } = {}) {
      const t0 = Date.now();
      try {
        const res = await getOnlyFetch(fileObj.url, { credentials, cache: 'no-store', redirect: 'follow' });
        const out = { ok: res.ok, status: res.status, redirected: res.redirected, finalUrl: res.url, responseType: res.type };
        if (!res.ok) return out;
        const buf = new Uint8Array(await res.arrayBuffer());
        out.size = buf.byteLength;
        out.sha256 = await sha256Hex(buf);
        out.ms = Date.now() - t0;
        if (includeBase64) out.base64 = bytesToBase64(buf);
        return out;
      } catch (e) {
        return { ok: false, error: `${e.name}: ${e.message}`, ms: Date.now() - t0 };
      }
    }

    return { getJson, getAll, probe, syncAll, downloadFile, stats };
  }

  function errorToResult(e) {
    if (e instanceof NeedsLogin) return { ok: false, needsLogin: true, status: e.status, url: e.url, body: e.body };
    return { ok: false, error: `${e.name}: ${e.message}`, status: e.status ?? null };
  }

  g.SOS_CANVAS = Object.freeze({ makeClient, normalizeOrigin, parseLinkHeader, stripJsonPrefix, errorToResult, sha256Hex, bytesToBase64, NeedsLogin, NotCanvas, HttpError });
})(globalThis);
