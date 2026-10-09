// Zero-dependency behavioral tests for bridge.js's drag-drop injector
// (the chunked __whatrustDropFeed protocol). Run: node src-tauri/resources/bridge.test.mjs
// Exits nonzero on failure. Same standalone pattern as settings-ui/comboToAccelerator.test.mjs.
//
// The harness stubs the minimal DOM surface bridge.js touches, evals the real
// script, then drives the feed exactly as window.rs does (begin/chunk/end/commit)
// and asserts which fake <input type=file> received which File objects.

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const bridgeSrc = readFileSync(join(here, "bridge.js"), "utf8");

let failures = 0;
function assert(cond, msg) {
  if (cond) {
    console.log("  ok  " + msg);
  } else {
    failures++;
    console.error("FAIL  " + msg);
  }
}
const b64 = (s) => Buffer.from(s).toString("base64");
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// --- minimal DOM stubs -------------------------------------------------------
class FakeFile {
  constructor(parts, name, opts) {
    this.name = name;
    this.type = (opts && opts.type) || "";
    let size = 0;
    for (const p of parts) size += p.length ?? 0;
    this.size = size;
    this.parts = parts;
  }
  bytes() {
    const out = Buffer.concat(this.parts.map((p) => Buffer.from(p)));
    return out;
  }
}
class FakeDataTransfer {
  constructor() {
    this._files = [];
    this.items = { add: (f) => this._files.push(f) };
  }
  get files() {
    return this._files.slice();
  }
}
class FakeInput {
  constructor(accept) {
    this.accept = accept;
    this.isConnected = true;
    this.files = [];
    this.batches = []; // one entry per change event: [FakeFile, ...]
  }
  dispatchEvent(ev) {
    if (ev.type === "change") {
      this.batches.push(this.files.slice());
      if (this.onchange) this.onchange();
    }
    return true;
  }
}

class FakeElement {
  constructor(tag) {
    this.tagName = String(tag).toUpperCase();
    this.children = [];
    this.parentNode = null;
    this.attrs = {};
    this.listeners = {};
    this.textContent = "";
    this.className = "";
    this.title = "";
  }
  get isConnected() {
    let n = this;
    while (n.parentNode) n = n.parentNode;
    return n.isRoot === true;
  }
  get firstChild() {
    return this.children[0] || null;
  }
  setAttribute(k, v) {
    this.attrs[k] = String(v);
  }
  attachShadow() {
    const root = new FakeElement("#shadow-root");
    root.parentNode = this;
    this.shadow = root;
    return root;
  }
  appendChild(c) {
    c.parentNode = this;
    this.children.push(c);
    return c;
  }
  removeChild(c) {
    this.children = this.children.filter((x) => x !== c);
    c.parentNode = null;
    return c;
  }
  addEventListener(type, fn) {
    (this.listeners[type] ||= []).push(fn);
  }
  click() {
    for (const fn of this.listeners.click || []) fn({ isTrusted: true });
  }
  // Every element below this one, depth-first.
  all() {
    return this.children.flatMap((c) => [c, ...c.all()]);
  }
}

function makeHarness(options = {}) {
  const mediaInput = new FakeInput("image/*,video/mp4,video/3gpp,video/quicktime");
  const docInput = new FakeInput("*");
  const logs = [];
  const warns = [];
  const invocations = [];
  // Simulate WhatsApp's composer lifecycle: it opens right after an input change
  // and "the user sends" ~80ms later. This exercises injectBatch's real waits
  // (composer detected, then queue held until it closes) without long timeouts.
  const state = { composerUntil: 0 };
  const onChange = () => {
    state.composerUntil = Date.now() + 80;
  };
  mediaInput.onchange = onChange;
  docInput.onchange = onChange;
  const body = new FakeElement("body");
  body.isRoot = true;
  const document = {
    body,
    createElement: (tag) => new FakeElement(tag),
    title: "WhatsApp",
    readyState: "complete",
    addEventListener() {},
    querySelectorAll(sel) {
      return sel === 'input[type="file"]' ? [mediaInput, docInput] : [];
    },
    querySelector(sel) {
      if (sel === "title") return null;
      if (sel.includes("media-caption-input-container")) {
        return Date.now() < state.composerUntil ? {} : null;
      }
      return null;
    },
  };
  const window = {
    location: { origin: "https://web.whatsapp.com" },
    addEventListener() {},
    __TAURI__: {
      core: {
        invoke(cmd, args) {
          invocations.push({ cmd, args });
          return options.invokeRejects
            ? Promise.reject(new Error("Command " + cmd + " not allowed by ACL"))
            : Promise.resolve();
        },
      },
    },
  };
  const sandboxGlobals = {
    window,
    document,
    navigator: options.navigator || {},
    console: { log: (m) => logs.push(String(m)), warn: (m) => warns.push(String(m)), error() {} },
    File: FakeFile,
    DataTransfer: FakeDataTransfer,
    Uint8Array: options.Uint8Array || Uint8Array,
    Event: class {
      constructor(type) {
        this.type = type;
      }
    },
    MutationObserver: class {
      observe() {}
    },
    atob: options.atob || ((s) => Buffer.from(s, "base64").toString("binary")),
    // unref'd, so bridge.js's 60s stream-GC timer cannot hold node open after the
    // last assertion. The timers still fire — the tests themselves await ref'd
    // sleeps, which keep the loop alive for as long as anything is being checked.
    setTimeout: (fn, ms) => {
      const t = setTimeout(fn, ms);
      if (t && typeof t.unref === "function") t.unref();
      return t;
    },
    clearTimeout,
    setInterval: () => 0,
  };
  // Evaluate bridge.js with our stubs shadowing the real globals.
  const params = Object.keys(sandboxGlobals);
  const fn = new Function(...params, `"use strict";\n${bridgeSrc}`);
  fn(...params.map((k) => sandboxGlobals[k]));
  return { window, document, mediaInput, docInput, logs, warns, invocations, navigator: sandboxGlobals.navigator };
}

// Drive the feed the way window.rs stream_drop does.
function feedFile(w, dropId, idx, name, type, content, { chunks = 1 } = {}) {
  const buf = Buffer.from(content);
  w.__whatrustDropFeed({ op: "begin", drop: dropId, file: idx, name, type, size: buf.length });
  const per = Math.max(1, Math.ceil(buf.length / chunks));
  for (let off = 0; off < buf.length || (buf.length === 0 && off === 0); off += per) {
    const slice = buf.subarray(off, Math.min(off + per, buf.length));
    w.__whatrustDropFeed({ op: "chunk", drop: dropId, file: idx, b64: slice.toString("base64") });
    if (buf.length === 0) break;
  }
  w.__whatrustDropFeed({ op: "end", drop: dropId, file: idx });
}

// --- tests -------------------------------------------------------------------
async function testMixedDropLosesNothing() {
  console.log("mixed drop routes everything as documents (no silent loss)");
  const { window: w, mediaInput, docInput } = makeHarness();
  feedFile(w, 1, 0, "photo.jpg", "image/jpeg", "jpegbytes");
  feedFile(w, 1, 1, "report.pdf", "application/pdf", "pdfbytes");
  const ack = w.__whatrustDropFeed({ op: "commit", drop: 1, files: 2 });
  assert(ack === "QUEUED:2", `commit acks both files (got ${ack})`);
  await sleep(30);
  assert(mediaInput.batches.length === 0, "media input untouched for a mixed drop");
  assert(docInput.batches.length === 1, "document input received one batch");
  const names = (docInput.batches[0] || []).map((f) => f.name).sort();
  assert(
    JSON.stringify(names) === JSON.stringify(["photo.jpg", "report.pdf"]),
    `both files attached (got ${names})`
  );
}

async function testPureMediaGoesToMediaInput() {
  console.log("pure media drop routes to the Photos & Videos input");
  const { window: w, mediaInput, docInput } = makeHarness();
  feedFile(w, 2, 0, "clip.mp4", "video/mp4", "mp4bytes");
  w.__whatrustDropFeed({ op: "commit", drop: 2, files: 1 });
  await sleep(30);
  assert(mediaInput.batches.length === 1, "media input received the video");
  assert(docInput.batches.length === 0, "document input untouched");
}

async function testDistinctSameNamedFilesBothAttach() {
  console.log("distinct files sharing a name + header prefix both attach");
  const { window: w, mediaInput } = makeHarness();
  const header = "A".repeat(64); // identical first 18+ bytes — old dedupe dropped one
  feedFile(w, 3, 0, "same.mp4", "video/mp4", header + "-first");
  feedFile(w, 3, 1, "same.mp4", "video/mp4", header + "-second");
  const ack = w.__whatrustDropFeed({ op: "commit", drop: 3, files: 2 });
  assert(ack === "QUEUED:2", `both distinct files queued (got ${ack})`);
  await sleep(30);
  assert(
    mediaInput.batches.length === 1 && mediaInput.batches[0].length === 2,
    "one change event carrying both files"
  );
}

async function testChunkReassemblyByteExact() {
  console.log("multi-chunk payload reassembles byte-for-byte");
  const { window: w, mediaInput } = makeHarness();
  const content = Buffer.alloc(10_000);
  for (let i = 0; i < content.length; i++) content[i] = (i * 31 + 7) & 0xff;
  feedFile(w, 4, 0, "big.mp4", "video/mp4", content, { chunks: 7 });
  w.__whatrustDropFeed({ op: "commit", drop: 4, files: 1 });
  await sleep(30);
  const file = mediaInput.batches[0] && mediaInput.batches[0][0];
  assert(!!file, "file arrived");
  assert(file && file.size === content.length, "size preserved across chunks");
  assert(file && file.bytes().equals(content), "bytes identical after reassembly");
}

async function testFromBase64FastPath() {
  console.log("modern WebViews use Uint8Array.fromBase64 without the atob fallback");
  let fastDecodes = 0;
  class FastUint8Array extends Uint8Array {}
  FastUint8Array.fromBase64 = (s) => {
    fastDecodes++;
    return Uint8Array.from(Buffer.from(s, "base64"));
  };
  const { window: w, mediaInput } = makeHarness({
    Uint8Array: FastUint8Array,
    atob: () => { throw new Error("atob fallback must not run"); },
  });
  const content = Buffer.from("native-fast-path");
  feedFile(w, 13, 0, "fast.mp4", "video/mp4", content, { chunks: 3 });
  const ack = w.__whatrustDropFeed({ op: "commit", drop: 13, files: 1 });
  assert(ack === "QUEUED:1", `fast-path stream queued (got ${ack})`);
  await sleep(30);
  const file = mediaInput.batches[0]?.[0];
  assert(fastDecodes === 3, `fromBase64 decoded all three stanzas (got ${fastDecodes})`);
  assert(file?.bytes().equals(content), "fast-path bytes reassemble exactly");
}

async function testChunksDecodeBeforeCommit() {
  console.log("base64 stanzas decode incrementally before commit");
  let decodes = 0;
  const { window: w, mediaInput } = makeHarness({
    atob: (s) => {
      decodes++;
      return Buffer.from(s, "base64").toString("binary");
    },
  });
  feedFile(w, 11, 0, "streaming.mp4", "video/mp4", "twelve-bytes", { chunks: 3 });
  await sleep(30);
  assert(decodes === 3, `all three stanzas decoded before commit (got ${decodes})`);
  assert(mediaInput.batches.length === 0, "decoding alone does not inject a file");
  const ack = w.__whatrustDropFeed({ op: "commit", drop: 11, files: 1 });
  assert(ack === "QUEUED:1", `decoded stream still validates at commit (got ${ack})`);
  await sleep(30);
  assert(mediaInput.batches[0]?.[0]?.name === "streaming.mp4", "decoded file injects after commit");
}

async function testIncompleteFileSkippedAbortHonored() {
  console.log("incomplete/aborted streams never produce a corrupt File");
  const { window: w, mediaInput, docInput } = makeHarness();
  // File 0: declared 100 bytes but only 10 sent (Rust died mid-stream) — no end op.
  w.__whatrustDropFeed({ op: "begin", drop: 5, file: 0, name: "trunc.mp4", type: "video/mp4", size: 100 });
  w.__whatrustDropFeed({ op: "chunk", drop: 5, file: 0, b64: b64("tenbytes!!") });
  // File 1: explicitly aborted by the sender.
  feedFile(w, 5, 1, "aborted.pdf", "application/pdf", "partial");
  w.__whatrustDropFeed({ op: "abort", drop: 5, file: 1 });
  // File 2: healthy.
  feedFile(w, 5, 2, "good.pdf", "application/pdf", "gooddata");
  const ack = w.__whatrustDropFeed({ op: "commit", drop: 5, files: 3 });
  assert(ack === "QUEUED:1", `only the complete file is queued (got ${ack})`);
  await sleep(30);
  const all = [...mediaInput.batches.flat(), ...docInput.batches.flat()].map((f) => f.name);
  assert(JSON.stringify(all) === JSON.stringify(["good.pdf"]), `only good.pdf attached (got ${all})`);
}

async function testCommittedDropsInjectSequentially() {
  console.log("committed drops use one composer in queue order");
  const slowPart = b64("one");
  let delayed = false;
  const { window: w, mediaInput } = makeHarness({
    atob: (s) => {
      // Make the first stanza exceed the 12 ms pump budget while another stanza
      // remains, forcing its decode to yield. The second committed job then decodes
      // quickly; the composer queue must still preserve commit order.
      if (!delayed && s === slowPart) {
        delayed = true;
        const until = Date.now() + 20;
        while (Date.now() < until) {}
      }
      return Buffer.from(s, "base64").toString("binary");
    },
  });
  w.__whatrustDropFeed({ op: "begin", drop: 6, file: 0, name: "first.mp4", type: "video/mp4", size: 4 });
  w.__whatrustDropFeed({ op: "chunk", drop: 6, file: 0, parts: [slowPart, b64("!")] });
  w.__whatrustDropFeed({ op: "end", drop: 6, file: 0 });
  w.__whatrustDropFeed({ op: "commit", drop: 6, files: 1 });
  feedFile(w, 7, 0, "second.mp4", "video/mp4", "two");
  w.__whatrustDropFeed({ op: "commit", drop: 7, files: 1 });
  // First drop's composer stays "open" ~80ms; the queue must hold drop #7 until
  // it closes, then inject — so wait past both cycles.
  await sleep(400);
  const seen = mediaInput.batches.map((b) => b.map((f) => f.name).join(","));
  assert(
    JSON.stringify(seen) === JSON.stringify(["first.mp4", "second.mp4"]),
    `both drops injected in order (got ${JSON.stringify(seen)})`
  );
}

async function testCommitWithoutDataAcksEmpty() {
  console.log("commit with no completed files acks EMPTY (Rust can toast)");
  const { window: w } = makeHarness();
  const ack = w.__whatrustDropFeed({ op: "commit", drop: 8, files: 0 });
  assert(ack === "EMPTY", `empty commit acks EMPTY (got ${ack})`);
}

async function testDecodeFailureBeforeCommitAcksErr() {
  console.log("a pre-commit decode failure is delegated to Rust exactly once");
  const { window: w, mediaInput, invocations } = makeHarness({
    atob: () => { throw new Error("forced early decode failure"); },
  });
  feedFile(w, 12, 0, "broken-early.mp4", "video/mp4", "bytes");
  await sleep(30);
  const ack = w.__whatrustDropFeed({ op: "commit", drop: 12, files: 1 });
  assert(ack === "ERR", `pre-commit failure acks ERR for Rust (got ${ack})`);
  assert(mediaInput.batches.length === 0, "failed decode is never injected");
  assert(
    !invocations.some((x) => x.cmd === "notify"),
    "JavaScript does not duplicate the Rust-side ERR notification"
  );
}

async function testDecodeFailureNotifies() {
  console.log("an asynchronous decode failure is visible to the user");
  const { window: w, mediaInput, invocations } = makeHarness({
    atob: () => { throw new Error("forced decode failure"); },
  });
  feedFile(w, 10, 0, "broken.mp4", "video/mp4", "bytes");
  const ack = w.__whatrustDropFeed({ op: "commit", drop: 10, files: 1 });
  assert(ack === "QUEUED:1", `valid stream is accepted into the queue (got ${ack})`);
  await sleep(30);
  assert(mediaInput.batches.length === 0, "failed decode is never injected");
  assert(
    invocations.some((x) => x.cmd === "notify" && /Could not attach/.test(x.args.body)),
    "native failure notification is sent"
  );
}

async function testMalformedMessagesAreRejected() {
  console.log("malformed feed messages are rejected without throwing");
  const { window: w } = makeHarness();
  assert(w.__whatrustDropFeed(null) === "BADMSG", "null message");
  assert(w.__whatrustDropFeed({ op: "chunk" }) === "BADMSG", "missing drop id");
  assert(
    w.__whatrustDropFeed({ op: "chunk", drop: 9, file: 0, b64: "AA==" }) === "NOFILE",
    "chunk before begin"
  );
  assert(w.__whatrustDropFeed({ op: "wat", drop: 9 }) === "BADOP", "unknown op");
}

async function testNotificationsReachTheNativeNotifyCommand() {
  console.log("page notifications (both APIs) reach the native notify command once");
  const { window: w, invocations } = makeHarness();
  new w.Notification("Alice", { body: "hello" });
  const notifies = invocations.filter((x) => x.cmd === "notify");
  assert(notifies.length === 1, "new Notification() forwards to notify");
  assert(
    notifies[0] && notifies[0].args.title === "Alice" && notifies[0].args.body === "hello",
    "title and body are forwarded"
  );
  // The same alert again within the burst window is a duplicate, not a second toast.
  new w.Notification("Alice", { body: "hello" });
  assert(invocations.filter((x) => x.cmd === "notify").length === 1, "burst duplicate suppressed");
  assert(w.Notification.permission === "granted", "Notification.permission reads granted");
}

async function testPermissionsApiAgreesNotificationsAreGranted() {
  console.log("navigator.permissions reports notifications granted, passes others through");
  const asked = [];
  const navigator = {
    permissions: {
      query(desc) {
        asked.push(desc.name);
        return Promise.resolve({ name: desc.name, state: "prompt" });
      },
    },
  };
  makeHarness({ navigator });
  const n = await navigator.permissions.query({ name: "notifications" });
  assert(n.state === "granted", `notifications state is granted (got ${n.state})`);
  assert(asked.length === 0, "the engine is not asked about notifications");
  const mic = await navigator.permissions.query({ name: "microphone" });
  assert(mic.state === "prompt" && asked[0] === "microphone", "other permissions reach the engine");
}

async function testRefusedCommandsWarnOnceAndNeverThrow() {
  console.log("a refused command is swallowed but leaves one console warning");
  const { window: w, warns } = makeHarness({ invokeRejects: true });
  new w.Notification("A", { body: "1" });
  new w.Notification("B", { body: "2" });
  await sleep(10);
  const notifyWarns = warns.filter((m) => m.includes("notify was refused"));
  assert(notifyWarns.length === 1, `one warning for notify (got ${notifyWarns.length})`);
}

async function testChromeShimsOnlyForAChromeUserAgent() {
  console.log("Chrome client hints only when the UA claims Chrome (Safari on macOS stays Safari)");
  const chromeNav = {
    userAgent: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36",
  };
  const chrome = makeHarness({ navigator: chromeNav });
  assert(chromeNav.userAgentData && chromeNav.userAgentData.platform === "Linux", "Chrome UA gets userAgentData");
  assert(!!chrome.window.chrome, "Chrome UA gets window.chrome");
  const safariNav = {
    userAgent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15",
  };
  const safari = makeHarness({ navigator: safariNav });
  assert(safariNav.userAgentData === undefined, "Safari UA gets no userAgentData");
  assert(safari.window.chrome === undefined, "Safari UA gets no window.chrome");
}

function toastsIn(document) {
  const host = document.body.children.find((c) => c.attrs["data-whatrust"] === "downloads");
  if (!host) return [];
  return host.shadow.all().filter((e) => /\btoast\b/.test(e.className));
}
const buttonsOf = (toast) => toast.all().filter((e) => e.tagName === "BUTTON");

async function testDownloadToastOffersOpenAndShowInFolder() {
  console.log("a finished download shows a toast whose buttons send only its id");
  const { window: w, document, invocations } = makeHarness();
  const name = '<img src=x onerror="alert(1)">.pdf';
  assert(w.__whatrustDownloadDone({ ok: true, id: 4, name, canOpen: true }) === true, "toast shown");
  const [toast] = toastsIn(document);
  assert(!!toast, "one toast in the page");
  const texts = toast.all().map((e) => e.textContent);
  assert(texts.includes("Downloaded") && texts.includes(name), "title and file name, as plain text");
  const labels = buttonsOf(toast).map((b) => b.textContent);
  assert(labels.includes("Open") && labels.includes("Show in folder"), `buttons: ${labels}`);
  buttonsOf(toast).find((b) => b.textContent === "Open").click();
  const call = invocations.find((x) => x.cmd === "open_download");
  assert(call && call.args.id === 4 && Object.keys(call.args).length === 1, "Open sends just the id");
  assert(toastsIn(document).length === 0, "the toast closes after a choice");
}

async function testRiskyDownloadsAndFailures() {
  console.log("programs only offer Show in folder; failures say so");
  const { window: w, document, invocations } = makeHarness();
  w.__whatrustDownloadDone({ ok: true, id: 9, name: "setup.exe", canOpen: false });
  const [toast] = toastsIn(document);
  const labels = buttonsOf(toast).map((b) => b.textContent);
  assert(!labels.includes("Open"), "no Open button for a program");
  buttonsOf(toast).find((b) => b.textContent === "Show in folder").click();
  assert(invocations.some((x) => x.cmd === "reveal_download" && x.args.id === 9), "Show in folder sends the id");
  w.__whatrustDownloadDone({ ok: false });
  const [failed] = toastsIn(document);
  assert(failed && /failed/.test(failed.className), "failure toast");
  assert(failed.all().some((e) => e.textContent === "Download failed"), "failure says so");
  assert(buttonsOf(failed).every((b) => b.textContent !== "Open"), "nothing to open after a failure");
  for (let i = 0; i < 5; i++) w.__whatrustDownloadDone({ ok: true, id: 20 + i, name: i + ".jpg", canOpen: true });
  assert(toastsIn(document).length === 3, "at most three toasts stack up");
}

const tests = [
  testDownloadToastOffersOpenAndShowInFolder,
  testRiskyDownloadsAndFailures,
  testChromeShimsOnlyForAChromeUserAgent,
  testNotificationsReachTheNativeNotifyCommand,
  testPermissionsApiAgreesNotificationsAreGranted,
  testRefusedCommandsWarnOnceAndNeverThrow,
  testMixedDropLosesNothing,
  testPureMediaGoesToMediaInput,
  testDistinctSameNamedFilesBothAttach,
  testChunkReassemblyByteExact,
  testFromBase64FastPath,
  testChunksDecodeBeforeCommit,
  testIncompleteFileSkippedAbortHonored,
  testCommittedDropsInjectSequentially,
  testCommitWithoutDataAcksEmpty,
  testDecodeFailureBeforeCommitAcksErr,
  testDecodeFailureNotifies,
  testMalformedMessagesAreRejected,
];

for (const t of tests) {
  await t();
}
if (failures > 0) {
  console.error(`\n${failures} assertion(s) FAILED`);
  process.exit(1);
}
console.log("\nall bridge tests passed");
