// Helpers shared by the installer and the tests: extension ID from the manifest key, host manifest, launcher script.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { fileURLToPath } from 'node:url';

export const HOST_NAME = 'dev.studentos.connector_spike';
export const SPIKE_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
export const HOST_SCRIPT = path.join(SPIKE_DIR, 'native-host', 'host.mjs');

// Chrome derives an unpacked extension's ID from the manifest "key": SHA-256 of the DER public key,
// first 16 bytes, each hex nibble mapped 0-f -> a-p.
export function extensionIdFromKey(b64) {
  const hex = crypto.createHash('sha256').update(Buffer.from(b64, 'base64')).digest('hex').slice(0, 32);
  return [...hex].map((c) => String.fromCharCode(97 + parseInt(c, 16))).join('');
}
export function extensionId(manifestPath = path.join(SPIKE_DIR, 'extension', 'manifest.json')) {
  return extensionIdFromKey(JSON.parse(fs.readFileSync(manifestPath, 'utf8')).key);
}

export function hostManifest(launcherPath, extId) {
  return {
    name: HOST_NAME,
    description: 'StudentOS Connector (spike) native host',
    path: launcherPath,
    type: 'stdio',
    allowed_origins: [`chrome-extension://${extId}/`],
  };
}

// Browsers launch the host with a minimal environment, so pin the absolute node binary.
export function writeLauncher(dest, { env = {} } = {}) {
  const q = (s) => `'${String(s).replace(/'/g, `'\\''`)}'`;
  const exports = Object.entries(env)
    .map(([k, v]) => `export ${k}=${q(v)}`)
    .join('\n');
  fs.writeFileSync(dest, `#!/bin/sh\n# generated launcher for ${HOST_NAME}\n${exports}\nexec ${q(process.execPath)} ${q(HOST_SCRIPT)} "$@"\n`, { mode: 0o755 });
  return dest;
}
