// StudentOS Connector (spike) - Chrome/Edge native messaging host.
//
// Protocol (https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging):
//   each message = 4-byte length in native byte order + UTF-8 JSON, on stdin (from browser) / stdout (to browser).
//   stdout is reserved for protocol frames; ALL logging goes to stderr.
// Writes what it receives to <out>/<run>/ and acks every request by id ({ re: id }).
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const OUT_BASE = path.resolve(process.env.STUDENTOS_CONNECTOR_OUT || path.join(HERE, '..', 'out'));
const LE = os.endianness() === 'LE';
const MAX_TO_BROWSER = 1024 * 1024; // documented Chrome limit for host -> browser messages

let runDir = null;
let fileSeq = 0;
const partial = new Map(); // xfer -> { total, parts: [], received, kind, meta, id }

const log = (...a) => process.stderr.write(`[studentos-host ${process.pid}] ${a.join(' ')}\n`);
const safe = (s) => String(s || '').replace(/[^A-Za-z0-9._-]/g, '_').slice(0, 80) || 'x';

function ensureRunDir(hint) {
  if (runDir) return runDir;
  const run = safe(hint || process.env.STUDENTOS_CONNECTOR_RUN || new Date().toISOString().replace(/[:.]/g, '-'));
  runDir = path.join(OUT_BASE, run);
  if (!runDir.startsWith(OUT_BASE + path.sep)) throw new Error('bad run dir');
  fs.mkdirSync(path.join(runDir, 'files'), { recursive: true });
  return runDir;
}
function event(obj) {
  ensureRunDir();
  fs.appendFileSync(path.join(runDir, 'host-events.jsonl'), JSON.stringify({ at: new Date().toISOString(), pid: process.pid, ...obj }) + '\n');
}
const sha256 = (b) => crypto.createHash('sha256').update(b).digest('hex');

function send(obj) {
  const body = Buffer.from(JSON.stringify(obj), 'utf8');
  const hdr = Buffer.alloc(4);
  LE ? hdr.writeUInt32LE(body.length) : hdr.writeUInt32BE(body.length);
  process.stdout.write(Buffer.concat([hdr, body]));
  return body.length;
}

// Each connectNative() starts a fresh host process, so continue numbering from what is already on disk
// and create files exclusively ('wx') so a later process can never overwrite an earlier one's output.
function writeOut(name, data) {
  ensureRunDir();
  if (fileSeq === 0) {
    for (const f of fs.readdirSync(runDir)) {
      const n = parseInt(f, 10);
      if (n > fileSeq) fileSeq = n;
    }
  }
  for (;;) {
    const file = path.join(runDir, `${String(++fileSeq).padStart(3, '0')}-${safe(name)}`);
    try {
      fs.writeFileSync(file, data, { flag: 'wx' });
      return path.relative(OUT_BASE, file);
    } catch (e) {
      if (e.code !== 'EEXIST') throw e;
    }
  }
}

async function handlePayload(msg, text) {
  const bytes = Buffer.byteLength(text, 'utf8');
  const base = { re: msg.id, type: 'ack', kind: msg.kind, chars: text.length, bytes, sha256: sha256(text) };
  if (msg.kind === 'file') {
    const f = JSON.parse(text);
    const buf = Buffer.from(f.base64, 'base64');
    const got = sha256(buf);
    const written = writeOut(`file-${f.name}`, buf);
    event({ type: 'file-bytes', context: msg.meta?.context, name: f.name, size: buf.length, sha256: got, match: got === f.sha256 });
    return { ...base, fileSize: buf.length, fileSha256: got, sha256Match: got === f.sha256, written };
  }
  if (msg.kind === 'blob') {
    event({ type: 'blob', chars: text.length, sha256: base.sha256 });
    return base; // size tests: do not write multi-MB junk to disk
  }
  const written = writeOut(`${msg.kind}-${msg.meta?.context || 'x'}.json`, text);
  event({ type: 'payload', kind: msg.kind, meta: msg.meta, bytes, written });
  return { ...base, written };
}

async function handle(msg) {
  switch (msg.type) {
    case 'hello':
      ensureRunDir(msg.run);
      event({ type: 'hello', extensionVersion: msg.extensionVersion, callerOrigin: process.argv[2] || null });
      return send({ re: msg.id, type: 'hello-ack', host: 'dev.studentos.connector_spike', pid: process.pid, runDir: path.relative(OUT_BASE, runDir), callerOrigin: process.argv[2] || null });
    case 'payload':
      return send(await handlePayload(msg, msg.data));
    case 'chunk': {
      let p = partial.get(msg.xfer);
      if (!p) partial.set(msg.xfer, (p = { total: msg.total, parts: new Array(msg.total), received: 0 }));
      if (p.parts[msg.seq] == null) p.received++;
      p.parts[msg.seq] = msg.data;
      if (p.received < p.total) return;
      partial.delete(msg.xfer);
      const ack = await handlePayload(msg, p.parts.join(''));
      return send({ ...ack, chunks: p.total });
    }
    case 'needs_login': {
      const written = writeOut('needs_login.json', JSON.stringify(msg, null, 2));
      event({ type: 'needs_login', origin: msg.origin, context: msg.context, status: msg.status });
      return send({ re: msg.id, type: 'ack', kind: 'needs_login', written });
    }
    case 'file_url': {
      // Desktop-side download: plain Node fetch, no cookies, using only the signed URL the extension saw.
      const t0 = Date.now();
      try {
        const res = await fetch(msg.url, { redirect: 'follow' });
        const buf = Buffer.from(await res.arrayBuffer());
        const got = sha256(buf);
        const written = res.ok ? writeOut(`desktop-${msg.name}`, buf) : null;
        event({ type: 'file_url', status: res.status, size: buf.length, match: got === msg.sha256Expected });
        return send({ re: msg.id, type: 'ack', kind: 'file_url', status: res.status, size: buf.length, sha256: got, sha256Match: got === msg.sha256Expected, ms: Date.now() - t0, written });
      } catch (e) {
        return send({ re: msg.id, type: 'ack', kind: 'file_url', error: String(e) });
      }
    }
    case 'send_big': {
      // Build a reply whose serialised JSON is exactly msg.bytes long (ASCII only).
      const shell = { re: msg.id, type: 'big', data: '' };
      const pad = Math.max(0, msg.bytes - Buffer.byteLength(JSON.stringify(shell)));
      shell.data = 'b'.repeat(pad);
      const n = send(shell);
      event({ type: 'send_big', bytes: n, overDocumentedLimit: n > MAX_TO_BROWSER });
      return;
    }
    default:
      return send({ re: msg.id, type: 'error', error: `unknown type ${msg.type}` });
  }
}

// ---- stdin framing ----
let chunks = [];
let have = 0;
let queue = Promise.resolve();
function take(n) {
  const all = chunks.length === 1 ? chunks[0] : Buffer.concat(chunks, have);
  const head = all.subarray(0, n);
  const rest = all.subarray(n);
  chunks = rest.length ? [rest] : [];
  have = rest.length;
  return head;
}
let need = null;
process.stdin.on('data', (c) => {
  chunks.push(c);
  have += c.length;
  for (;;) {
    if (need == null) {
      if (have < 4) break;
      const h = take(4);
      need = LE ? h.readUInt32LE(0) : h.readUInt32BE(0);
    }
    if (have < need) break;
    const body = take(need);
    need = null;
    let msg;
    try {
      msg = JSON.parse(body.toString('utf8'));
    } catch (e) {
      log('bad json frame', e.message);
      continue;
    }
    queue = queue.then(() => handle(msg)).catch((e) => log('handler error', e.stack));
  }
});
process.stdin.on('end', () => {
  queue.finally(() => {
    log('stdin closed, exiting');
    process.exit(0);
  });
});
log(`started; caller=${process.argv[2] || '?'} out=${OUT_BASE}`);
