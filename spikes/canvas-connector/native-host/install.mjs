#!/usr/bin/env node
// Registers (or removes) the spike's native messaging host for YOUR browser profile.
//
//   node native-host/install.mjs [--browser chrome|edge|chromium|brave] [--dir <NativeMessagingHosts dir>] [--dry-run]
//   node native-host/install.mjs --uninstall [--browser ...] [--dir ...]
//
// It writes exactly two files and prints both before writing:
//   1. native-host/run-host.sh                               (launcher pinned to this node binary)
//   2. <browser NativeMessagingHosts dir>/dev.studentos.connector_spike.json
// --uninstall removes exactly those two files and nothing else.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { HOST_NAME, SPIKE_DIR, extensionId, hostManifest, writeLauncher } from './manifest-util.mjs';

const args = process.argv.slice(2);
const flag = (f) => args.includes(`--${f}`);
const opt = (f, d) => {
  const i = args.indexOf(`--${f}`);
  return i >= 0 && args[i + 1] ? args[i + 1] : d;
};

const home = os.homedir();
const DIRS = {
  darwin: {
    chrome: 'Library/Application Support/Google/Chrome/NativeMessagingHosts',
    edge: 'Library/Application Support/Microsoft Edge/NativeMessagingHosts',
    chromium: 'Library/Application Support/Chromium/NativeMessagingHosts',
    brave: 'Library/Application Support/BraveSoftware/Brave-Browser/NativeMessagingHosts',
  },
  linux: {
    chrome: '.config/google-chrome/NativeMessagingHosts',
    edge: '.config/microsoft-edge/NativeMessagingHosts',
    chromium: '.config/chromium/NativeMessagingHosts',
    brave: '.config/BraveSoftware/Brave-Browser/NativeMessagingHosts',
  },
};

if (flag('help')) {
  console.log(fs.readFileSync(new URL(import.meta.url), 'utf8').split('\n').slice(1, 11).join('\n'));
  process.exit(0);
}
const browser = opt('browser', 'chrome');
let dir = opt('dir', null);
if (!dir) {
  const rel = DIRS[process.platform]?.[browser];
  if (!rel) {
    console.error(`Unsupported platform/browser: ${process.platform}/${browser}. On Windows a registry key is needed; pass --dir for a custom location.`);
    process.exit(2);
  }
  dir = path.join(home, rel);
}
dir = path.resolve(dir);
const manifestFile = path.join(dir, `${HOST_NAME}.json`);
const launcher = path.join(SPIKE_DIR, 'native-host', 'run-host.sh');
const dry = flag('dry-run');

if (flag('uninstall')) {
  for (const f of [manifestFile, launcher]) {
    if (fs.existsSync(f)) {
      console.log(`${dry ? '[dry-run] would remove' : 'removing'} ${f}`);
      if (!dry) fs.unlinkSync(f);
    } else console.log(`not present: ${f}`);
  }
  process.exit(0);
}

const extId = extensionId();
const manifest = hostManifest(launcher, extId);
console.log(`Extension ID (from extension/manifest.json "key"): ${extId}`);
console.log(`\n[1/2] launcher: ${launcher}`);
console.log(`      exec ${process.execPath} ${path.join(SPIKE_DIR, 'native-host', 'host.mjs')}`);
console.log(`      host output directory: ${path.join(SPIKE_DIR, 'out')}/<timestamp>/`);
console.log(`\n[2/2] host manifest: ${manifestFile}`);
console.log(JSON.stringify(manifest, null, 2));
if (dry) {
  console.log('\n--dry-run: nothing written.');
  process.exit(0);
}
writeLauncher(launcher);
fs.mkdirSync(dir, { recursive: true });
fs.writeFileSync(manifestFile, JSON.stringify(manifest, null, 2) + '\n');
console.log(`\nWritten. Undo with: node native-host/install.mjs --uninstall${browser !== 'chrome' ? ` --browser ${browser}` : ''}${opt('dir') ? ` --dir ${JSON.stringify(dir)}` : ''}`);
