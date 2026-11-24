// Run with: node --test src-tauri/resources/media.test.mjs
// media.js is the Linux (WebKitGTK) fix for attaching a video. These tests use a
// small fake DOM: the engine behaviour itself was verified in a real WebKitGTK.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { test } from 'node:test';

const source = readFileSync(new URL('./media.js', import.meta.url), 'utf8');

function harness({ origin = 'https://web.whatsapp.com', copyPlay = 'resolve', inFrame = false } = {}) {
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
  // The engine's own play()/pause(): calls are recorded, as `nativePlays`.
  const nativePlays = [];
  class HTMLAudioElement extends HTMLMediaElement {
    constructor() { super('audio'); this.loop = false; this.volume = 1; this.muted = false; }
    play() { nativePlays.push(this); return Promise.resolve(); }
    pause() {}
  }
  HTMLMediaElement.prototype.play = HTMLAudioElement.prototype.play;
  HTMLMediaElement.prototype.pause = HTMLAudioElement.prototype.pause;
  delete HTMLAudioElement.prototype.play;
  delete HTMLAudioElement.prototype.pause;
  // An <audio> made by an iframe's window: the copy media.js plays. `end()`
  // stands in for the sound finishing. `copyPlay: 'reject'` refuses playback.
  const copies = [];
  class FrameAudio {
    constructor(url) { this.url = url; this.listeners = {}; this.paused = true; copies.push(this); }
    addEventListener(type, fn) { (this.listeners[type] ||= []).push(fn); }
    play() {
      if (copyPlay === 'reject') return Promise.reject(Object.assign(new Error('refused'), { name: 'NotAllowedError' }));
      this.paused = false; return Promise.resolve();
    }
    pause() { this.paused = true; }
    end(type = 'ended') { this.paused = true; (this.listeners[type] || []).forEach(fn => fn()); }
  }
  class HTMLIFrameElement extends Element {
    constructor() { super('iframe'); this.contentWindow = { Audio: FrameAudio }; }
  }
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
  const document = { body, createElement: tag => (tag === 'video' ? new HTMLVideoElement(tag) : tag === 'audio' ? new HTMLAudioElement() : tag === 'img' ? new HTMLImageElement() : tag === 'iframe' ? new HTMLIFrameElement() : new Element(tag)) };
  let n = 0;
  const URL = { createObjectURL: () => 'blob:https://web.whatsapp.com/' + (++n), revokeObjectURL() {} };
  const timers = [];
  const window = { location: { origin }, __TAURI__: { core: { invoke: (cmd, args) => { logs.push(args.msg); return Promise.resolve(); } } } };
  window.top = inFrame ? {} : window;
  runInNewContext(source, { window, document, URL, Event, Element, HTMLMediaElement, HTMLVideoElement, HTMLAudioElement, HTMLImageElement, MutationObserver,
    WeakSet, WeakMap, Map, Object, String, Promise, setTimeout: fn => timers.push(fn) });
  const flush = () => { while (timers.length) timers.shift()(); };
  const trusted = type => { const e = new Event(type); e.isTrusted = true; return e; };
  return { document, body, URL, logs, flush, trusted, Element, HTMLMediaElement, nativePlays, copies };
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

// --- 5. alert sounds (issue #120) -----------------------------------------------
const ALERT = 'https://static.whatsapp.net/rsrc.php/send_message.mp3';
const frames = body => body.children.filter(c => c.getAttribute('data-whatrust') === 'alert-sound');
// The events media.js reports on the page's own element.
const watch = (el, types) => { const seen = []; types.forEach(t => el.addEventListener(t, () => seen.push(t))); return seen; };

test('an alert sound plays from a copy in a hidden iframe, removed when it ends', async () => {
  const { document, body, nativePlays, copies } = harness();
  const a = document.createElement('audio');
  a.src = ALERT;
  a.volume = 0.4;
  const seen = watch(a, ['play', 'playing', 'ended']);
  await a.play();
  assert.equal(nativePlays.length, 0, 'the page element itself never plays');
  assert.equal(copies.length, 1);
  assert.equal(copies[0].url, ALERT);
  assert.equal(copies[0].volume, 0.4, 'volume is carried over');
  const [frame] = frames(body);
  assert.ok(frame, 'the copy lives in an iframe');
  assert.equal(frame.style.cssText, 'display:none');
  assert.deepEqual(seen, ['play', 'playing']);
  copies[0].end();
  assert.equal(frames(body).length, 0, 'the iframe, and with it the media player, is gone');
  assert.deepEqual(seen, ['play', 'playing', 'ended']);
});

test('a second alert restarts the sound and never leaves an iframe behind', async () => {
  const { document, body, copies } = harness();
  const a = document.createElement('audio');
  a.src = ALERT;
  await a.play();
  await a.play(); // a second message before the first sound ended
  assert.equal(frames(body).length, 1, 'only the newest copy remains');
  assert.equal(copies[0].paused, true, 'the first copy was stopped');
  const seen = watch(a, ['ended']);
  copies[0].end(); // a late event from the stopped copy is ignored
  assert.deepEqual(seen, []);
  copies[1].end();
  assert.equal(frames(body).length, 0);
  assert.deepEqual(seen, ['ended']);
});

test('pause() and a load error remove the iframe too', async () => {
  const { document, body, copies } = harness();
  const a = document.createElement('audio');
  a.src = ALERT;
  a.loop = true; // a ringtone loops until WhatsApp pauses it
  const seen = watch(a, ['pause', 'error']);
  await a.play();
  assert.equal(copies[0].loop, true);
  a.pause();
  assert.equal(frames(body).length, 0);
  assert.equal(copies[0].paused, true);
  await a.play();
  copies[1].end('error');
  assert.equal(frames(body).length, 0);
  assert.deepEqual(seen, ['pause', 'error']);
});

test('voice messages and other blob media keep the engine playback', async () => {
  const { document, body, URL, nativePlays, copies } = harness();
  const voice = document.createElement('audio');
  voice.src = URL.createObjectURL({ type: 'audio/ogg' });
  await voice.play();
  const video = document.createElement('video');
  video.src = 'https://media.example/v.mp4';
  await video.play();
  assert.deepEqual(nativePlays, [voice, video]);
  assert.equal(copies.length, 0);
  assert.equal(frames(body).length, 0);
});

test('a refused copy falls back to playing the element itself', async () => {
  const { document, body, logs, nativePlays } = harness({ copyPlay: 'reject' });
  const a = document.createElement('audio');
  a.src = ALERT;
  await a.play();
  assert.deepEqual(nativePlays, [a], 'the sound is never lost');
  assert.equal(frames(body).length, 0, 'the unused iframe is removed');
  assert.ok(logs.some(m => /alert sound copy refused \(NotAllowedError\)/.test(m)));
});

test('inside an iframe nothing is copied, so a copy can never copy itself', async () => {
  const { document, body, nativePlays } = harness({ inFrame: true });
  const a = document.createElement('audio');
  a.src = ALERT;
  await a.play();
  assert.deepEqual(nativePlays, [a]);
  assert.equal(frames(body).length, 0);
});
