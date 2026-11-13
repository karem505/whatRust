// Run with: node --test src-tauri/resources/upsell.test.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { test } from 'node:test';

const source = readFileSync(new URL('./upsell.js', import.meta.url), 'utf8');

// A stand-in for WhatsApp's module system: __d(name, deps, factory, flags)
// registers a factory; require(name) runs it once with an exports object in
// the 7th position, as WhatsApp's ES-interop modules do.
function moduleSystem(win) {
  const factories = new Map(), cache = new Map();
  win.__d = function (name, deps, factory) { factories.set(name, factory); };
  return function require(name) {
    if (!cache.has(name)) {
      const exports = {};
      factories.get(name)(win, null, null, null, null, null, exports);
      cache.set(name, exports);
    }
    return cache.get(name);
  };
}

function utilsFactory(t, n, r, o, a, i, l) {
  l.getUserDesktopOs = () => 'WINDOWS';
  l.isAndroidAppUpsellEnabled = () => true;
  l.other = () => 42;
}

test('hook installed before WhatsApp defines __d neuters the upsell module', () => {
  const win = {};
  runInNewContext(source, { window: win });
  const require = moduleSystem(win);
  win.__d('WAWebDesktopUpsellUtils', [], utilsFactory, 98);
  const m = require('WAWebDesktopUpsellUtils');
  assert.equal(m.getUserDesktopOs(), null);
  assert.equal(m.isAndroidAppUpsellEnabled(), false);
  assert.equal(m.other(), 42);
  assert.equal(win.__wrUpsellPatched, true);
});

test('other modules are untouched', () => {
  const win = {};
  runInNewContext(source, { window: win });
  const require = moduleSystem(win);
  win.__d('WAWebSomethingElse', [], utilsFactory, 98);
  assert.equal(require('WAWebSomethingElse').getUserDesktopOs(), 'WINDOWS');
  assert.equal(win.__wrUpsellPatched, undefined);
});

test('a factory that throws still throws and nothing is patched', () => {
  const win = {};
  runInNewContext(source, { window: win });
  moduleSystem(win);
  let ran;
  win.__d = function (name, deps, factory) { ran = factory; };
  win.__d('WAWebDesktopUpsellUtils', [], () => { throw new Error('boom'); });
  assert.throws(() => ran(), /boom/);
  assert.equal(win.__wrUpsellPatched, undefined);
});

test('running twice installs one hook', () => {
  const win = {};
  runInNewContext(source, { window: win });
  runInNewContext(source, { window: win });
  const require = moduleSystem(win);
  win.__d('WAWebDesktopUpsellUtils', [], utilsFactory, 98);
  assert.equal(require('WAWebDesktopUpsellUtils').getUserDesktopOs(), null);
});
