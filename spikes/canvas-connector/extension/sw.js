// StudentOS Connector (spike) - MV3 service worker.
// Classic worker so the same GET-only guard + Canvas core can be shared with the content script.
importScripts('lib/getonly.js', 'lib/canvas-core.js', 'lib/native.js');

const { makeClient, normalizeOrigin, errorToResult } = SOS_CANVAS;
const CS_ID = 'studentos-canvas';
const ALARM_PERIODIC = 'studentos-sync';
const ALARM_ONESHOT = 'studentos-sync-once';
const native = new SOS_NATIVE.NativeLink();

const DEFAULTS = { canvasOrigin: null, autoSyncOnVisit: true, visitSyncMinIntervalMs: 15 * 60 * 1000, syncPeriodMinutes: 60, runId: null };

async function settings() {
  return chrome.storage.local.get(DEFAULTS);
}

// ---------- content script registration (only for the origin the user granted) ----------
async function ensureContentScript(origin) {
  const existing = await chrome.scripting.getRegisteredContentScripts({ ids: [CS_ID] });
  const granted = origin ? await chrome.permissions.contains({ origins: [origin + '/*'] }) : false;
  if (!granted) {
    if (existing.length) await chrome.scripting.unregisterContentScripts({ ids: [CS_ID] });
    return null;
  }
  const def = {
    id: CS_ID,
    matches: [origin + '/*'],
    js: ['lib/getonly.js', 'lib/canvas-core.js', 'content.js'],
    runAt: 'document_idle',
    allFrames: false,
    persistAcrossSessions: true,
  };
  if (existing.length) await chrome.scripting.updateContentScripts([def]);
  else await chrome.scripting.registerContentScripts([def]);
  return def;
}

async function ensureAlarm() {
  const { syncPeriodMinutes, canvasOrigin } = await settings();
  if (!canvasOrigin) return chrome.alarms.clear(ALARM_PERIODIC);
  const a = await chrome.alarms.get(ALARM_PERIODIC);
  if (!a || a.periodInMinutes !== syncPeriodMinutes) chrome.alarms.create(ALARM_PERIODIC, { periodInMinutes: syncPeriodMinutes, delayInMinutes: 1 });
}

// ---------- native host ----------
async function ensureNative() {
  if (!native.port) {
    const { runId } = await settings();
    await native.hello(runId);
  }
}

async function reportNeedsLogin(origin, context, detail) {
  await ensureNative();
  return native.request({ type: 'needs_login', origin, context, status: detail.status ?? null, url: detail.url ?? null, at: new Date().toISOString() });
}

function summarize(s) {
  const mods = Object.values(s.modules);
  return {
    context: s.context,
    user: s.user,
    courses: s.courses.length,
    coursePages: s.coursePages,
    modules: mods.reduce((a, m) => a + m.length, 0),
    moduleItems: mods.reduce((a, m) => a + m.reduce((b, x) => b + (x.items || []).length, 0), 0),
    fileObjects: Object.keys(s.fileObjects).length,
    planner: s.planner.length,
    errors: s.errors,
    stats: { ...s.stats, urls: undefined, requestCount: s.stats.urls.length },
  };
}

async function deliverSnapshot(snapshot, trigger) {
  await ensureNative();
  const ack = await native.sendPayload('snapshot', snapshot, { meta: { context: snapshot.context, trigger } });
  await chrome.storage.local.set({ lastSync: { at: Date.now(), context: snapshot.context, trigger } });
  return { ok: true, summary: summarize(snapshot), ack };
}

// ---------- sync, variant (b): from the service worker ----------
async function swSync(trigger = 'manual') {
  const { canvasOrigin } = await settings();
  if (!canvasOrigin) return { ok: false, error: 'not configured' };
  const client = makeClient({ origin: canvasOrigin, credentials: 'include', context: 'service-worker' });
  try {
    const snapshot = await client.syncAll();
    return await deliverSnapshot(snapshot, trigger);
  } catch (e) {
    const r = errorToResult(e);
    if (r.needsLogin) r.hostAck = await reportNeedsLogin(canvasOrigin, 'service-worker', r).catch((err) => ({ error: String(err) }));
    return r;
  }
}

// ---------- sync, variant (a): from the content script ----------
async function canvasTab() {
  const { canvasOrigin } = await settings();
  if (!canvasOrigin) return null;
  const tabs = await chrome.tabs.query({ url: canvasOrigin + '/*' });
  return tabs.find((t) => t.status === 'complete') || tabs[0] || null;
}

async function handleContentResult(result, trigger) {
  if (result && result.ok) return deliverSnapshot(result.snapshot, trigger);
  if (result && result.needsLogin) result.hostAck = await reportNeedsLogin(result.origin, 'content-script', result).catch((err) => ({ error: String(err) }));
  return result;
}

async function csCommand(msg) {
  const tab = await canvasTab();
  if (!tab) return { ok: false, error: 'no Canvas tab open' };
  return chrome.tabs.sendMessage(tab.id, msg);
}

async function csSync(trigger = 'manual') {
  const r = await csCommand({ cmd: 'cs-sync' });
  if (r && r.error === 'no Canvas tab open') return r;
  return handleContentResult(r, trigger);
}

// ---------- messages from our own pages / content script ----------
chrome.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  if (sender.id !== chrome.runtime.id) return false;
  (async () => {
    const s = await settings();
    // Extension pages (options page, even when shown in a tab) have our chrome-extension:// origin;
    // anything else is the content script running in the Canvas page.
    const fromContent = sender.origin !== new URL(chrome.runtime.getURL('')).origin;
    if (fromContent && sender.origin !== s.canvasOrigin) return { ok: false, error: 'content message from unexpected origin' };
    switch (msg.cmd) {
      case 'configure': {
        if (fromContent) return { ok: false, error: 'forbidden' };
        const origin = normalizeOrigin(msg.origin);
        if (!(await chrome.permissions.contains({ origins: [origin + '/*'] }))) return { ok: false, error: 'host permission not granted' };
        await chrome.storage.local.set({ canvasOrigin: origin });
        const cs = await ensureContentScript(origin);
        await ensureAlarm();
        const probe = await makeClient({ origin, credentials: 'include', context: 'service-worker' }).probe();
        return { ok: true, origin, contentScriptRegistered: !!cs, probe };
      }
      case 'status': {
        let host;
        try {
          host = await native.hello(s.runId);
        } catch (e) {
          host = { error: String(e.message || e), lastDisconnect: native.disconnects.at(-1) || null };
        }
        return { ok: true, settings: s, host };
      }
      case 'sync-now':
        return swSync('manual');
      case 'cs-hello': {
        const due = s.autoSyncOnVisit && (!s.lastSync || Date.now() - s.lastSync.at > s.visitSyncMinIntervalMs);
        return { autoSync: !!due };
      }
      case 'cs-result':
        return handleContentResult(msg.result, msg.trigger || 'visit');
      default:
        return { ok: false, error: `unknown cmd ${msg.cmd}` };
    }
  })().then(sendResponse, (e) => sendResponse({ ok: false, error: `${e.name}: ${e.message}` }));
  return true;
});

chrome.alarms.onAlarm.addListener(async (alarm) => {
  if (alarm.name !== ALARM_PERIODIC && alarm.name !== ALARM_ONESHOT) return;
  const r = await swSync(`alarm:${alarm.name}`);
  const tabs = await chrome.tabs.query({});
  await chrome.storage.local.set({ lastAlarmResult: { at: Date.now(), alarm: alarm.name, ok: r.ok, needsLogin: !!r.needsLogin, summary: r.summary || null, ack: r.ack || null, openTabUrls: tabs.map((t) => t.url) } });
});

chrome.permissions.onRemoved.addListener(async () => {
  const { canvasOrigin } = await settings();
  await ensureContentScript(canvasOrigin);
});

chrome.runtime.onInstalled.addListener(async () => {
  const { canvasOrigin } = await settings();
  await ensureContentScript(canvasOrigin);
  await ensureAlarm();
});
chrome.runtime.onStartup.addListener(async () => {
  const { canvasOrigin } = await settings();
  await ensureContentScript(canvasOrigin);
});
chrome.action.onClicked.addListener(() => chrome.runtime.openOptionsPage());

// ---------- spike-only hooks, driven by tests/run.mjs through Playwright's worker.evaluate ----------
self.spike = {
  native,
  settings,
  swSync,
  csSync,
  csCommand,
  canvasTab,
  ensureContentScript,
  async rawGet(path, accept) {
    const { canvasOrigin } = await settings();
    const c = makeClient({ origin: canvasOrigin, credentials: 'include', context: 'service-worker' });
    try {
      const r = await c.getJson(path, { acceptOverride: accept });
      return { ok: true, rawPrefixed: r.rawPrefixed, parsedId: r.json.id, strippedPrefixes: c.stats.strippedPrefixes, responseType: r.type };
    } catch (e) {
      return errorToResult(e);
    }
  },
  async swDownload(fileId, { forwardBytes = false, handoffUrl = false } = {}) {
    const { canvasOrigin } = await settings();
    const c = makeClient({ origin: canvasOrigin, credentials: 'include', context: 'service-worker' });
    const fileObj = (await c.getJson(`/api/v1/files/${fileId}`)).json;
    const r = await c.downloadFile(fileObj, { includeBase64: forwardBytes });
    const out = { context: 'service-worker', fileUrl: fileObj.url, expectedSize: fileObj.size, ...r, base64: undefined };
    if (r.ok && forwardBytes) {
      await ensureNative();
      out.hostBytesAck = await native.sendPayload('file', { name: fileObj.filename, sha256: r.sha256, base64: r.base64 }, { meta: { context: 'service-worker' } });
    }
    if (r.ok && handoffUrl) {
      await ensureNative();
      out.hostUrlAck = await native.request({ type: 'file_url', url: r.finalUrl, name: fileObj.filename, sha256Expected: r.sha256 }, { timeoutMs: 60000 });
    }
    return out;
  },
  // Direct fetch of an arbitrary URL from the SW (used to pin down CORS behaviour on the file store).
  async swFetchUrl(url, credentials) {
    try {
      const r = await fetch(url, { credentials, cache: 'no-store' });
      const b = await r.arrayBuffer();
      return { ok: r.ok, status: r.status, size: b.byteLength, type: r.type };
    } catch (e) {
      return { ok: false, error: `${e.name}: ${e.message}` };
    }
  },
  async csDownload(fileId, { forwardBytes = false } = {}) {
    const r = await csCommand({ cmd: 'cs-download', fileId, includeBase64: forwardBytes });
    const out = { context: 'content-script', ...r, base64: undefined };
    if (r.ok && forwardBytes) {
      await ensureNative();
      out.hostBytesAck = await native.sendPayload('file', { name: r.name, sha256: r.sha256, base64: r.base64 }, { meta: { context: 'content-script' } });
    }
    return out;
  },
  async tryWrites() {
    const { canvasOrigin } = await settings();
    const url = canvasOrigin + '/api/v1/courses/2001/discussion_topics';
    const attempts = {
      'getOnlyFetch POST': () => SOS_GETONLY.getOnlyFetch(url, { method: 'POST', credentials: 'include', body: '{}' }),
      'global fetch POST': () => fetch(url, { method: 'POST', credentials: 'include', body: '{}' }),
      'fetch(Request DELETE)': () => fetch(new Request(url, { method: 'DELETE' })),
      'GET with X-CSRF-Token': () => fetch(url, { headers: { 'X-CSRF-Token': 'x' } }),
      'SOS client getJson (GET, control)': () => makeClient({ origin: canvasOrigin, credentials: 'include', context: 'sw' }).getJson('/api/v1/users/self'),
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
  },
  async nativeHello() {
    const { runId } = await settings();
    return native.hello(runId);
  },
  async nativeSend(chars, opts = {}) {
    await ensureNative();
    const data = 'a'.repeat(chars);
    const t0 = Date.now();
    try {
      const ack = await native.sendPayload('blob', data, opts);
      return { ok: true, ack, ms: Date.now() - t0 };
    } catch (e) {
      return { ok: false, error: `${e.name}: ${e.message}`, thrownSynchronouslyByPostMessage: !e.nativeError, lastDisconnect: native.disconnects.at(-1) || null, ms: Date.now() - t0 };
    }
  },
  async nativeAskBig(bytes) {
    await ensureNative();
    const before = native.disconnects.length;
    try {
      const msg = await native.request({ type: 'send_big', bytes }, { timeoutMs: 15000 });
      return { ok: true, receivedChars: msg.data.length, messageJsonLength: JSON.stringify(msg).length };
    } catch (e) {
      return { ok: false, error: e.message, disconnect: native.disconnects.slice(before) };
    }
  },
  async scheduleOneShotAlarm(delayInMinutes) {
    await chrome.storage.local.remove('lastAlarmResult');
    await chrome.alarms.create(ALARM_ONESHOT, { delayInMinutes });
    const a = await chrome.alarms.get(ALARM_ONESHOT);
    return { scheduledFor: a && a.scheduledTime, now: Date.now() };
  },
};
