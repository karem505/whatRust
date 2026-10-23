// Run with: node --test src-tauri/resources/notifications.test.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { test } from 'node:test';

const source = readFileSync(new URL('./bridge.js', import.meta.url), 'utf8');

function harness({ reject = false } = {}) {
  const calls = [], errors = [], intervals = [];
  const document = {
    title: '(3) WhatsApp', readyState: 'complete',
    querySelector: () => null, querySelectorAll: () => [],
    addEventListener() {},
  };
  const window = {
    location: { origin: 'https://web.whatsapp.com' }, addEventListener() {},
    ServiceWorkerRegistration: class {
      showNotification() {}
      getNotifications() {}
    },
    __TAURI__: { core: { invoke(cmd, args) {
      calls.push({ cmd, args });
      return reject ? Promise.reject('private message payload') : Promise.resolve();
    } } },
  };
  runInNewContext(source, {
    window, document, navigator: {},
    console: { warn: message => errors.push(message), error: message => errors.push(message), log() {} },
    MutationObserver: class { observe() {} },
    setInterval: (callback, delay) => { intervals.push({ callback, delay }); return 0; },
    setTimeout: () => 0, clearTimeout() {},
  });
  return { window, document, calls, errors, intervals };
}

test('page and service-worker registration notifications reach the custom Rust command', async () => {
  const { window, calls } = harness();
  new window.Notification('test A', { body: 'body A' });
  await new window.ServiceWorkerRegistration().showNotification('test B', { body: 'body B' });
  const notifications = calls.filter(c => c.cmd === 'notify');
  assert.equal(notifications.length, 2);
  assert.equal(notifications[0].args.title, 'test A');
  assert.equal(notifications[1].args.body, 'body B');
});

test('the two notification paths do not duplicate the same alert', async () => {
  const { window, calls } = harness();
  new window.Notification('same', { body: 'message' });
  await new window.ServiceWorkerRegistration().showNotification('same', { body: 'message' });
  assert.equal(calls.filter(c => c.cmd === 'notify').length, 1);
});

test('unread initialization, changes and clearing reach Rust without repeating unchanged titles', () => {
  const { document, calls, intervals } = harness();
  const poll = intervals.find(timer => timer.delay === 2000).callback;
  poll();
  document.title = '(4) WhatsApp'; poll();
  document.title = 'WhatsApp'; poll(); poll();
  assert.deepEqual(calls.filter(c => c.cmd === 'set_unread').map(c => c.args.title),
    ['(3) WhatsApp', '(4) WhatsApp', 'WhatsApp']);
});

test('IPC rejection is visible without logging private notification content', async () => {
  const { window, errors } = harness({ reject: true });
  new window.Notification('private title', { body: 'private body' });
  await new Promise(resolve => setImmediate(resolve));
  assert.ok(errors.includes('[whatRust] notify was refused'));
  assert.ok(errors.includes('[whatRust] set_unread was refused'));
  assert.ok(errors.every(message => !message.includes('private')));
});
