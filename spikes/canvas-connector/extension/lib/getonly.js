// GET-only network guard, shared by the service worker (importScripts) and the content script.
//
// Loaded FIRST in every extension JS world. It captures the real fetch, then replaces the world's
// global fetch (and, in documents, XMLHttpRequest.open / navigator.sendBeacon) with guarded versions,
// so no code path in the extension can issue a write to Canvas, even by accident.
// It also refuses to send an X-CSRF-Token header: the connector never reads the _csrf_token cookie.
(function (g) {
  'use strict';
  if (g.SOS_GETONLY) return; // content scripts can be injected twice into the same world

  const realFetch = g.fetch.bind(g);

  class GetOnlyViolation extends Error {
    constructor(message) {
      super(message);
      this.name = 'GetOnlyViolation';
    }
  }

  function describe(input) {
    try {
      return typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
    } catch {
      return String(input);
    }
  }

  // Throws synchronously (before any network activity) on anything that is not a plain GET.
  function assertGetOnly(input, init) {
    let method = 'GET';
    if (typeof Request !== 'undefined' && input instanceof Request) method = input.method;
    if (init && init.method != null) method = String(init.method);
    method = method.toUpperCase();
    if (method !== 'GET') throw new GetOnlyViolation(`read-only connector refused ${method} ${describe(input)}`);
    if (init && init.body != null) throw new GetOnlyViolation(`read-only connector refused a request body for ${describe(input)}`);
    const headers = new Headers((init && init.headers) || (input instanceof Request ? input.headers : undefined));
    if (headers.has('x-csrf-token')) throw new GetOnlyViolation('read-only connector never sends X-CSRF-Token');
    return headers;
  }

  function getOnlyFetch(input, init = {}) {
    const headers = assertGetOnly(input, init);
    return realFetch(input, { ...init, method: 'GET', headers });
  }

  function guardedFetch(input, init) {
    try {
      return getOnlyFetch(input, init);
    } catch (e) {
      return Promise.reject(e);
    }
  }

  Object.defineProperty(g, 'fetch', { value: guardedFetch, writable: false, configurable: false });

  if (typeof XMLHttpRequest !== 'undefined') {
    const realOpen = XMLHttpRequest.prototype.open;
    Object.defineProperty(XMLHttpRequest.prototype, 'open', {
      value: function (method, ...rest) {
        if (String(method).toUpperCase() !== 'GET') throw new GetOnlyViolation(`read-only connector refused XHR ${method}`);
        return realOpen.call(this, method, ...rest);
      },
      writable: false,
      configurable: false,
    });
  }
  if (typeof navigator !== 'undefined' && typeof navigator.sendBeacon === 'function') {
    Object.defineProperty(navigator, 'sendBeacon', {
      value: () => {
        throw new GetOnlyViolation('read-only connector refused sendBeacon (always POST)');
      },
      writable: false,
      configurable: false,
    });
  }

  g.SOS_GETONLY = Object.freeze({ getOnlyFetch, assertGetOnly, GetOnlyViolation });
})(globalThis);
