// Run with: node --test src-tauri/resources/media-audio.test.mjs
// media.js sections 5 and 6 (Linux): one sound stream. A small fake DOM; the
// engine behaviour (streams per paused <audio> and per AudioContext) was
// measured in a real WebKitGTK 2.54 window.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { test } from 'node:test';

const source = readFileSync(new URL('./media.js', import.meta.url), 'utf8');

function harness() {
  const logs = [];
  const timers = [];
  class Event { constructor(type) { this.type = type; this.stopped = false; } stopImmediatePropagation() { this.stopped = true; } }
  class Element {
    constructor(tag) { this.tagName = String(tag).toUpperCase(); this.attrs = {}; this.listeners = []; this.style = {}; }
    setAttribute(n, v) { this.attrs[n] = String(v); if (n === 'src') this._src = String(v); }
    removeAttribute(n) { delete this.attrs[n]; if (n === 'src') this._src = ''; }
    addEventListener(type, fn, capture) { this.listeners.push({ type, fn, capture: !!capture }); }
    dispatchEvent(e) {
      const mine = this.listeners.filter(l => l.type === e.type);
      for (const l of [...mine.filter(l => l.capture), ...mine.filter(l => !l.capture)]) { if (e.stopped) return; l.fn.call(this, e); }
    }
  }
  // The engine: a stream is open while an element has media and has played.
  const engine = { streams: new Set(), loads: [] };
  class HTMLMediaElement extends Element {
    constructor(tag) { super(tag); this._src = ''; this._t = 0; this._paused = true; this._preload = 'auto'; this.srcObject = null; }
    get src() { return this._src; } set src(v) { this._src = String(v); this._t = 0; }
    get currentSrc() { return this._src; }
    get preload() { return this._preload; } set preload(v) { this._preload = String(v); }
    get readyState() { return this._src ? 4 : 0; }
    get currentTime() { return this._t; } set currentTime(v) { this._t = Number(v); }
    get duration() { return this._src ? 3 : NaN; }
    get paused() { return this._paused; }
    play() { if (!this._src) return Promise.reject(new Error('no src')); this._paused = false; engine.streams.add(this); this.dispatchEvent(new Event('play')); return Promise.resolve(); }
    pause() { this._paused = true; }
    load() { engine.loads.push(this._src); if (!this._src) { engine.streams.delete(this); this.dispatchEvent(new Event('emptied')); } }
  }
  class HTMLAudioElement extends HTMLMediaElement { constructor() { super('audio'); } }
  class HTMLVideoElement extends HTMLMediaElement {}
  class HTMLImageElement extends Element { constructor() { super('img'); this._src = ''; } get src() { return this._src; } set src(v) { this._src = v; } }
  let n = 0;
  const live = new Set();
  const URL = {
    createObjectURL: () => { const u = 'blob:https://web.whatsapp.com/' + (++n); live.add(u); return u; },
    revokeObjectURL: u => { live.delete(u); },
  };
  const contexts = [];
  class AudioContext {
    constructor(opts) { this.opts = opts; this.state = 'running'; this.sampleRate = (opts && opts.sampleRate) || 48000; this.destination = { ctx: this }; contexts.push(this); }
    close() { this.state = 'closed'; return Promise.resolve(); }
    suspend() { this.state = 'suspended'; return Promise.resolve(); }
    resume() { this.state = 'running'; return Promise.resolve(); }
    createGain() { return new GainNode(this); }
  }
  class GainNode { constructor(ctx) { if (!(ctx instanceof AudioContext) || contexts.indexOf(ctx) < 0) throw new TypeError('not an engine context'); this.context = ctx; } }
  const window = { location: { origin: 'https://web.whatsapp.com' }, AudioContext, GainNode,
    __TAURI__: { core: { invoke: (c, a) => { logs.push(a.msg); return Promise.resolve(); } } } };
  runInNewContext(source, { window, document: { body: new Element('body'), createElement: () => new HTMLAudioElement() }, URL, Event, Element,
    HTMLMediaElement, HTMLAudioElement, HTMLVideoElement, HTMLImageElement, MutationObserver: class { observe() {} disconnect() {} },
    WeakSet, WeakMap, Map, Set, Object, String, Number, Array, Date, Promise, Proxy, Reflect, Error, TypeError, DOMException,
    isFinite, setTimeout: fn => { timers.push(fn); return timers.length; }, clearTimeout: id => { timers[id - 1] = null; } });
  const flush = () => { const t = timers.splice(0); t.forEach(f => f && f()); };
  return { window, URL, logs, flush, engine, contexts, live, HTMLAudioElement };
}

test('a paused alert sound gives its stream back and replays from the page position', async () => {
  const { flush, engine, HTMLAudioElement, logs } = harness();
  const a = new HTMLAudioElement();
  a.src = 'https://static.whatsapp.net/notification.mp3';
  await a.play();
  a.pause();
  assert.equal(engine.streams.size, 1);
  flush();
  assert.equal(engine.streams.size, 0, 'stream closed');
  assert.equal(a.src, 'https://static.whatsapp.net/notification.mp3', 'the page still reads its src');
  assert.ok(logs.includes('media: released idle audio streams: 1'));
  a.currentTime = 0;
  await a.play();
  assert.equal(a._src, 'https://static.whatsapp.net/notification.mp3', 'media put back');
  assert.equal(engine.streams.size, 1);
});

test('a played voice message (blob:) is released, keeps its length, and resumes where it was', async () => {
  const { URL, flush, engine, HTMLAudioElement } = harness();
  const url = URL.createObjectURL({ type: 'audio/ogg; codecs=opus' });
  const a = new HTMLAudioElement();
  a.src = url;
  await a.play();
  a.currentTime = 1.25;
  a.pause();
  flush();
  assert.equal(engine.streams.size, 0);
  assert.equal(a.duration, 3, 'length kept while parked');
  assert.equal(a.currentTime, 1.25);
  assert.equal(a.readyState, 4);
  await a.play();
  assert.equal(a._src, url);
  assert.equal(a._t, 1.25);
});

test('a voice message whose URL the page revoked plays again from a URL of our own', async () => {
  const { URL, flush, HTMLAudioElement, live } = harness();
  const url = URL.createObjectURL({ type: 'audio/ogg' });
  const a = new HTMLAudioElement();
  a.src = url;
  await a.play(); a.pause(); flush();
  URL.revokeObjectURL(url);
  await a.play();
  assert.notEqual(a._src, url, 'reloaded from a fresh URL');
  assert.ok(live.has(a._src));
  assert.equal(a.src, url, 'the page still sees its own URL');
  a.pause(); flush();
  assert.equal(live.has(url), false);
  assert.equal(live.size, 0, 'our own URL is revoked again when parked');
});

test('our own unload does not reach the page: no emptied event', async () => {
  const { flush, HTMLAudioElement } = harness();
  const a = new HTMLAudioElement();
  const seen = [];
  a.addEventListener('emptied', () => seen.push('emptied'));
  a.src = 'data:audio/wav;base64,AAAA';
  await a.play(); a.pause(); flush();
  assert.deepEqual(seen, []);
});

test('a sound that ends by itself, video and call media are handled as before', async () => {
  const { flush, engine, HTMLAudioElement } = harness();
  const call = new HTMLAudioElement();
  call.srcObject = {};
  call.src = 'blob:https://web.whatsapp.com/unknown';
  await call.play(); call.pause(); flush();
  assert.equal(engine.streams.size, 1, 'srcObject / unknown blobs untouched');
});

test('every AudioContext without its own sample rate shares one engine context', async () => {
  const { window, contexts, logs } = harness();
  const a = new window.AudioContext();
  const b = new window.AudioContext();
  assert.equal(contexts.length, 1, 'one engine context');
  assert.ok(a instanceof window.AudioContext);
  const g = new window.GainNode(b); // node constructors get the engine context
  assert.equal(g.context, contexts[0]);
  assert.equal(a.sampleRate, 48000);
  const own = new window.AudioContext({ sampleRate: 8000 });
  assert.equal(contexts.length, 2, 'an explicit sample rate keeps its own context');
  await own.close();
  await a.suspend();
  assert.equal(a.state, 'suspended');
  assert.equal(b.state, 'running');
  assert.equal(contexts[0].state, 'running', 'still used by b');
  await b.suspend();
  assert.equal(contexts[0].state, 'suspended');
  await b.resume();
  await a.close(); await b.close();
  assert.equal(a.state, 'closed');
  assert.equal(contexts[0].state, 'closed', 'closed with its last handle');
  new window.AudioContext();
  assert.equal(contexts.length, 3, 'a new one after that');
  assert.ok(logs.includes('media: shared audio context installed'));
});
