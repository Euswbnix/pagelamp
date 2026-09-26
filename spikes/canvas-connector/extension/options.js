'use strict';
const $ = (id) => document.getElementById(id);
const show = (v) => ($('out').textContent = typeof v === 'string' ? v : JSON.stringify(v, null, 2));

chrome.storage.local.get({ canvasOrigin: null, autoSyncOnVisit: true }).then((s) => {
  if (s.canvasOrigin) $('canvas-url').value = s.canvasOrigin;
  $('auto').checked = s.autoSyncOnVisit;
});

$('save').addEventListener('click', async () => {
  let origin;
  try {
    origin = SOS_CANVAS.normalizeOrigin($('canvas-url').value);
  } catch (e) {
    return show(`Invalid URL: ${e.message}`);
  }
  // Must be called directly from the click (user gesture), before any other await.
  const granted = await chrome.permissions.request({ origins: [origin + '/*'] });
  if (!granted) return show(`Permission for ${origin} was not granted.`);
  const r = await chrome.runtime.sendMessage({ cmd: 'configure', origin });
  let verdict = 'Not a Canvas site (or unreachable).';
  if (r.ok && r.probe.canvas && r.probe.loggedIn) verdict = 'Canvas detected - you are logged in.';
  else if (r.ok && r.probe.canvas) verdict = 'Canvas detected - please log in to Canvas in this browser.';
  show({ verdict, ...r });
  document.body.dataset.state = r.ok ? (r.probe.loggedIn ? 'logged-in' : r.probe.canvas ? 'needs-login' : 'not-canvas') : 'error';
});

$('sync').addEventListener('click', async () => show(await chrome.runtime.sendMessage({ cmd: 'sync-now' })));
$('status').addEventListener('click', async () => show(await chrome.runtime.sendMessage({ cmd: 'status' })));
$('auto').addEventListener('change', (e) => chrome.storage.local.set({ autoSyncOnVisit: e.target.checked }));
