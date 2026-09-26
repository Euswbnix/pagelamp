// StudentOS Connector (spike) - content script, registered at runtime ONLY for the Canvas origin the user granted.
// Sync variant (a): same-origin GETs from the Canvas page's origin, so the browser attaches the session cookie.
// The data is handed to the service worker, which forwards it to the native host.
(function () {
  'use strict';
  if (globalThis.__studentosConnectorLoaded) return;
  globalThis.__studentosConnectorLoaded = true;

  const { makeClient, errorToResult } = SOS_CANVAS;
  const origin = location.origin;
  const client = () => makeClient({ origin, credentials: 'same-origin', context: 'content-script' });

  async function sync() {
    try {
      return { ok: true, origin, snapshot: await client().syncAll() };
    } catch (e) {
      return { ...errorToResult(e), origin };
    }
  }

  async function tryWrites() {
    const url = origin + '/api/v1/courses/2001/discussion_topics';
    const attempts = {
      'getOnlyFetch POST': () => SOS_GETONLY.getOnlyFetch(url, { method: 'POST', body: '{}' }),
      'global fetch POST': () => fetch(url, { method: 'POST', body: '{}' }),
      'fetch(Request PUT)': () => fetch(new Request(url, { method: 'PUT' })),
      'XMLHttpRequest POST': () => {
        const x = new XMLHttpRequest();
        x.open('POST', url);
        x.send('{}');
      },
      'navigator.sendBeacon': () => navigator.sendBeacon(url, '{}'),
      'GET with X-CSRF-Token': () => fetch(url, { headers: { 'X-CSRF-Token': 'x' } }),
    };
    const out = {};
    for (const [k, fn] of Object.entries(attempts)) {
      try {
        await fn();
        out[k] = 'NOT BLOCKED (request allowed)';
      } catch (e) {
        out[k] = `${e.name}: ${e.message}`;
      }
    }
    return out;
  }

  chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
    // Commands only from our own service worker (not from other tabs/content scripts).
    if (sender.id !== chrome.runtime.id || sender.tab) return false;
    (async () => {
      switch (msg.cmd) {
        case 'cs-sync':
          return sync();
        case 'cs-raw-get': {
          const c = client();
          try {
            const r = await c.getJson(msg.path, { acceptOverride: msg.accept });
            return { ok: true, rawPrefixed: r.rawPrefixed, parsedId: r.json.id, strippedPrefixes: c.stats.strippedPrefixes };
          } catch (e) {
            return errorToResult(e);
          }
        }
        case 'cs-download': {
          const c = client();
          try {
            const fileObj = (await c.getJson(`/api/v1/files/${msg.fileId}`)).json;
            const r = await c.downloadFile(fileObj, { includeBase64: !!msg.includeBase64 });
            return { name: fileObj.filename, fileUrl: fileObj.url, expectedSize: fileObj.size, ...r };
          } catch (e) {
            return errorToResult(e);
          }
        }
        case 'cs-try-writes':
          return tryWrites();
        default:
          return { ok: false, error: `unknown cmd ${msg.cmd}` };
      }
    })().then(sendResponse, (e) => sendResponse({ ok: false, error: `${e.name}: ${e.message}` }));
    return true;
  });

  // Opportunistic sync when the student visits Canvas (the service worker decides/throttles).
  chrome.runtime
    .sendMessage({ cmd: 'cs-hello', origin })
    .then(async (r) => {
      if (r && r.autoSync) await chrome.runtime.sendMessage({ cmd: 'cs-result', trigger: 'visit', result: await sync() });
    })
    .catch(() => {});
})();
