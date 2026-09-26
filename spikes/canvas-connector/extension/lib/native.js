// Native messaging link from the service worker to the StudentOS desktop side
// (host name dev.studentos.connector_spike). Request/ack by id, optional chunking for large payloads.
(function (g) {
  'use strict';
  const HOST_NAME = 'dev.studentos.connector_spike';
  const DEFAULT_CHUNK_CHARS = 512 * 1024;

  class NativeLink {
    constructor(hostName = HOST_NAME) {
      this.hostName = hostName;
      this.port = null;
      this.seq = 0;
      this.pending = new Map(); // id -> {resolve, reject, timer}
      this.unsolicited = []; // messages from the host with no matching id
      this.disconnects = []; // {at, error}
      this.connectCount = 0;
      this.runId = null;
    }

    connect() {
      if (this.port) return this.port;
      const port = chrome.runtime.connectNative(this.hostName);
      this.connectCount++;
      port.onMessage.addListener((msg) => {
        const p = msg && msg.re != null ? this.pending.get(msg.re) : null;
        if (p) {
          clearTimeout(p.timer);
          this.pending.delete(msg.re);
          p.resolve(msg);
        } else this.unsolicited.push({ at: Date.now(), type: msg && msg.type, size: JSON.stringify(msg).length });
      });
      port.onDisconnect.addListener(() => {
        const error = chrome.runtime.lastError ? chrome.runtime.lastError.message : null;
        this.disconnects.push({ at: Date.now(), error });
        if (this.port === port) this.port = null;
        for (const [id, p] of this.pending) {
          clearTimeout(p.timer);
          p.reject(Object.assign(new Error(`native host disconnected: ${error}`), { nativeError: error }));
        }
        this.pending.clear();
      });
      this.port = port;
      return port;
    }

    disconnect() {
      if (this.port) this.port.disconnect();
      this.port = null;
    }

    post(msg) {
      this.connect().postMessage(msg); // throws synchronously if the message is too large
    }

    request(msg, { timeoutMs = 60000 } = {}) {
      const id = ++this.seq;
      return new Promise((resolve, reject) => {
        const timer = setTimeout(() => {
          this.pending.delete(id);
          reject(new Error(`native host timeout for ${msg.type}`));
        }, timeoutMs);
        this.pending.set(id, { resolve, reject, timer });
        try {
          this.post({ ...msg, id });
        } catch (e) {
          clearTimeout(timer);
          this.pending.delete(id);
          reject(e);
        }
      });
    }

    async hello(runId) {
      this.runId = runId || this.runId;
      return this.request({ type: 'hello', run: this.runId, extensionVersion: chrome.runtime.getManifest().version });
    }

    // Send a JSON-serialisable value (or a string) either as one message or as N chunk messages.
    async sendPayload(kind, value, { chunkChars = DEFAULT_CHUNK_CHARS, forceChunk = false, meta = {} } = {}) {
      const text = typeof value === 'string' ? value : JSON.stringify(value);
      if (!forceChunk && (chunkChars === Infinity || text.length <= chunkChars)) {
        return this.request({ type: 'payload', kind, meta, data: text });
      }
      const xfer = `x${Date.now().toString(36)}${Math.random().toString(36).slice(2, 8)}`;
      const total = Math.ceil(text.length / chunkChars);
      const id = ++this.seq;
      const done = new Promise((resolve, reject) => {
        const timer = setTimeout(() => {
          this.pending.delete(id);
          reject(new Error('native host timeout for chunked transfer'));
        }, 120000);
        this.pending.set(id, { resolve, reject, timer });
      });
      try {
        for (let i = 0; i < total; i++) {
          this.post({ type: 'chunk', id, xfer, kind, meta, seq: i, total, data: text.slice(i * chunkChars, (i + 1) * chunkChars) });
        }
      } catch (e) {
        const p = this.pending.get(id);
        if (p) {
          clearTimeout(p.timer);
          this.pending.delete(id);
        }
        throw e;
      }
      const ack = await done;
      return { ...ack, chunks: total };
    }
  }

  g.SOS_NATIVE = Object.freeze({ NativeLink, HOST_NAME });
})(globalThis);
