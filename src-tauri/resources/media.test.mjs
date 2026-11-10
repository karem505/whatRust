// Run with: node --test src-tauri/resources/media.test.mjs
// media.js is the Linux (WebKitGTK) fix for attaching a video. These tests use a
// small fake DOM: the engine behaviour itself was verified in a real WebKitGTK.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { test } from 'node:test';

const source = readFileSync(new URL('./media.js', import.meta.url), 'utf8');

function harness({ origin = 'https://web.whatsapp.com' } = {}) {
  const logs = [];
  class Event { constructor(type) { this.type = type; this.isTrusted = false; this.stopped = false; } stopImmediatePropagation() { this.stopped = true; } }
  class Element {
    constructor(tag) { this.tagName = String(tag).toUpperCase(); this.attrs = {}; this.style = { cssText: '' }; this.children = []; this.parentNode = null; this.listeners = []; this.isRoot = false; }
    getAttribute(n) { return n in this.attrs ? this.attrs[n] : null; }
    setAttribute(n, v) { this.attrs[n] = String(v); if (n === 'src') this._src = String(v); if (n === 'preload') this._preload = String(v); }
    removeAttribute(n) { delete this.attrs[n]; if (n === 'preload') this._preload = 'auto'; }
    addEventListener(type, fn, capture) { this.listeners.push({ type, fn, capture: !!capture }); }
    removeEventListener(type, fn) { this.listeners = this.listeners.filter(l => !(l.type === type && l.fn === fn)); }
    // Capture listeners first, then the rest, then the on* handler: as at a real target.
    dispatchEvent(e) {
      e.target = this;
      const mine = this.listeners.filter(l => l.type === e.type);
      for (const l of [...mine.filter(l => l.capture), ...mine.filter(l => !l.capture)]) { if (e.stopped) return true; l.fn.call(this, e); }
      if (!e.stopped && typeof this['on' + e.type] === 'function') this['on' + e.type](e);
      return true;
    }
    appendChild(c) { if (c.parentNode) c.parentNode.removeChild(c); this.children.push(c); c.parentNode = this; (this.observers || []).forEach(f => f()); return c; }
    removeChild(c) { this.children = this.children.filter(x => x !== c); c.parentNode = null; (this.observers || []).forEach(f => f()); return c; }
    get firstChild() { return this.children[0] || null; }
    get isConnected() { for (let n = this; n; n = n.parentNode) if (n.isRoot) return true; return false; }
  }
  class HTMLMediaElement extends Element {
    constructor(tag) { super(tag); this._src = ''; this._preload = 'auto'; this._rs = 0; }
    get src() { return this._src; } set src(v) { this._src = String(v); }
    get preload() { return this._preload; } set preload(v) { this._preload = String(v); this.attrs.preload = String(v); }
    get readyState() { return this._rs; }
  }
  class HTMLVideoElement extends HTMLMediaElement {}
  class HTMLImageElement extends Element {
    constructor() { super('img'); this._src = ''; }
    get src() { return this._src; } set src(v) { this._src = String(v); }
  }
  class MutationObserver {
    constructor(fn) { this.fn = fn; }
    observe(node) { (node.observers ||= []).push(this.fn); this.node = node; }
    disconnect() { if (this.node) this.node.observers = this.node.observers.filter(f => f !== this.fn); }
  }
  const body = new Element('body'); body.isRoot = true;
  const document = { body, createElement: tag => (tag === 'video' ? new HTMLVideoElement(tag) : tag === 'audio' ? new HTMLMediaElement(tag) : tag === 'img' ? new HTMLImageElement() : new Element(tag)) };
  let n = 0;
  const URL = { createObjectURL: () => 'blob:https://web.whatsapp.com/' + (++n), revokeObjectURL() {} };
  const timers = [];
  const window = { location: { origin }, __TAURI__: { core: { invoke: (cmd, args) => { logs.push(args.msg); return Promise.resolve(); } } } };
  runInNewContext(source, { window, document, URL, Event, Element, HTMLMediaElement, HTMLVideoElement, HTMLImageElement, MutationObserver,
    WeakSet, WeakMap, Map, Object, String, setTimeout: fn => timers.push(fn) });
  const flush = () => { while (timers.length) timers.shift()(); };
  const trusted = type => { const e = new Event(type); e.isTrusted = true; return e; };
  return { document, body, URL, logs, flush, trusted, Element, HTMLMediaElement };
}

test('does nothing outside web.whatsapp.com', () => {
  const { document, URL, logs } = harness({ origin: 'https://example.com' });
  const v = document.createElement('video');
  v.src = URL.createObjectURL({ type: 'video/mp4' });
  assert.equal(v.preload, 'auto');
  assert.equal(v.parentNode, null);
  assert.deepEqual(logs, []);
});

test('a blob video loads with preload=metadata; other sources are left alone', () => {
  const { document, URL } = harness();
  const v = document.createElement('video');
  v.src = 'https://media.example/v.mp4';
  assert.equal(v.preload, 'auto');
  v.src = URL.createObjectURL({ type: 'video/mp4' });
  assert.equal(v.preload, 'metadata');
  v.preload = 'auto'; // the page asking for auto on a blob still gets metadata
  assert.equal(v.preload, 'metadata');
  v.preload = 'none';
  assert.equal(v.preload, 'none');
});

test('setAttribute("src") is covered too, and the page preload comes back afterwards', () => {
  const { document, URL } = harness();
  const v = document.createElement('video');
  v.setAttribute('preload', 'auto');
  v.setAttribute('src', URL.createObjectURL({ type: 'video/mp4' }));
  assert.equal(v.preload, 'metadata');
  v.src = 'https://media.example/v.mp4';
  assert.equal(v.getAttribute('preload'), 'auto');
  const w = document.createElement('video'); // no preload attribute of its own
  w.src = URL.createObjectURL({ type: 'video/mp4' });
  w.src = '';
  assert.equal(w.getAttribute('preload'), null);
});

test('canplaythrough is fired after canplay, once, and HAVE_FUTURE_DATA reads as enough', () => {
  const { document, URL, flush, trusted } = harness();
  const v = document.createElement('video');
  let through = 0;
  v.oncanplaythrough = () => { through++; };
  v.src = URL.createObjectURL({ type: 'video/mp4' });
  v._rs = 3;
  assert.equal(v.readyState, 4);
  v.dispatchEvent(trusted('canplay'));
  assert.equal(through, 0, 'not synchronously');
  flush();
  assert.equal(through, 1);
  v.dispatchEvent(trusted('canplaythrough')); // the engine catching up later is swallowed
  assert.equal(through, 1);
  v.dispatchEvent(trusted('emptied')); // a new load starts over
  v.dispatchEvent(trusted('canplay')); flush();
  assert.equal(through, 2);
});

test('a real canplaythrough is passed through and not duplicated', () => {
  const { document, URL, flush, trusted } = harness();
  const v = document.createElement('video');
  let through = 0;
  v.oncanplaythrough = () => { through++; };
  v.src = URL.createObjectURL({ type: 'video/mp4' });
  v._rs = 4;
  v.dispatchEvent(trusted('canplay'));
  v.dispatchEvent(trusted('canplaythrough'));
  flush();
  assert.equal(through, 1);
});

test('readyState is untouched for non-blob media and below HAVE_FUTURE_DATA', () => {
  const { document, URL } = harness();
  const a = document.createElement('video');
  a.src = 'https://media.example/v.mp4'; a._rs = 3;
  assert.equal(a.readyState, 3);
  const b = document.createElement('video');
  b.src = URL.createObjectURL({ type: 'video/mp4' }); b._rs = 2;
  assert.equal(b.readyState, 2);
});

test('a detached blob video is parked in the viewport and released when disposed', () => {
  const { document, body, URL, trusted } = harness();
  const v = document.createElement('video');
  v.src = URL.createObjectURL({ type: 'video/mp4' });
  const box = v.parentNode;
  assert.ok(box && box.parentNode === body, 'parked under <body>');
  assert.equal(box.getAttribute('data-whatrust'), 'video-park');
  assert.match(box.style.cssText, /position:fixed/);
  assert.match(box.style.cssText, /pointer-events:none/);
  assert.equal(v.style.cssText, '', 'the element itself is not restyled');
  v.src = ''; v.dispatchEvent(trusted('emptied')); // WhatsApp's disposeVideo
  assert.equal(v.parentNode, null);
  assert.equal(body.children.length, 0, 'the box is gone');
});

test('a parked video the page moves into its own tree leaves no box behind', () => {
  const { document, body, URL, Element } = harness();
  const v = document.createElement('video');
  v.src = URL.createObjectURL({ type: 'video/mp4' });
  const mine = new Element('div'); body.appendChild(mine);
  mine.appendChild(v);
  assert.equal(v.parentNode, mine);
  assert.deepEqual(body.children, [mine]);
});

test('a video already in the page, and audio, are not parked', () => {
  const { document, body, URL, Element } = harness();
  const holder = new Element('div'); body.appendChild(holder);
  const v = document.createElement('video'); holder.appendChild(v);
  v.src = URL.createObjectURL({ type: 'video/mp4' });
  assert.equal(v.parentNode, holder);
  const a = document.createElement('audio');
  a.src = URL.createObjectURL({ type: 'audio/ogg' });
  assert.equal(a.parentNode, null);
  assert.equal(a.preload, 'metadata');
});

test('a video blob given to <img> is refused; image blobs and revoked URLs are not', () => {
  const { document, URL, logs } = harness();
  const video = URL.createObjectURL({ type: 'video/mp4' });
  const image = URL.createObjectURL({ type: 'image/jpeg' });
  const img = document.createElement('img');
  img.src = video;
  assert.match(img._src, /^data:image\/png/, 'the engine never sees the video');
  assert.equal(img.src, video, 'the page reads back what it set');
  assert.ok(logs.includes('media: refused a video blob given to <img>'));
  img.src = image;
  assert.equal(img._src, image);
  const viaAttr = document.createElement('img');
  viaAttr.setAttribute('src', video);
  assert.match(viaAttr._src, /^data:image\/png/);
  URL.revokeObjectURL(video);
  const later = document.createElement('img');
  later.src = video;
  assert.equal(later._src, video);
});
