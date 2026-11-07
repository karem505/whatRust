// Run: node settings-ui/notes.test.mjs
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import assert from "node:assert/strict";

const require = createRequire(import.meta.url);
const N = require("./notes.js");

let failures = 0;
function check(name, fn) {
  try { fn(); console.log("  ok  " + name); }
  catch (e) { failures++; console.log("  FAIL " + name + "\n       " + e.message.split("\n").join("\n       ")); }
}

// A DOM that only offers createElement/createTextNode/textContent: anything else
// (innerHTML, insertAdjacentHTML, ...) would be undefined and fail the test.
function fakeDoc() {
  const mk = (tag) => ({
    tag, children: [], _text: "",
    get firstChild() { return this.children[0] || null; },
    appendChild(c) { this.children.push(c); return c; },
    removeChild(c) { this.children.splice(this.children.indexOf(c), 1); },
    set textContent(v) { this._text = String(v); },
    get textContent() { return this._text + this.children.map((c) => c.textContent ?? c.data ?? "").join(""); },
  });
  return { createElement: mk, createTextNode: (data) => ({ data, textContent: data }) };
}
const tags = (el, acc = new Set()) => { for (const c of el.children) if (c.tag) { acc.add(c.tag); tags(c, acc); } return acc; };

const REAL = `## whatRust v0.6.11

Drag-and-drop works again on Linux, plus a **privacy blur**.

### Fixed

- **Linux: drag-and-drop works again.** In 0.6.9 and 0.6.10, files never reached the chat. Thanks to @t43riz (#62).
- **Thin scrollbars.** The chat list no longer shows a thick white scrollbar.

### Support whatRust

[![Sponsor whatRust](https://img.shields.io/badge/Sponsor-ea4aaa)](https://github.com/sponsors/karem505)

\`\`\`bash
curl -fsSL https://example.com/install.sh | sh
\`\`\`

**Full changelog:** https://github.com/karem505/whatRust/compare/v0.6.10...v0.6.11
`;

check("headings, paragraphs, lists, code blocks and bold are understood", () => {
  const b = N.parse(REAL);
  assert.deepEqual(b.map((x) => x.k), ["h", "p", "h", "ul", "h", "pre", "p"]);
  assert.equal(b[3].items.length, 2);
  assert.deepEqual(b[1].c.map((x) => x.t), ["text", "b", "text"]);
  assert.equal(b[5].s, "curl -fsSL https://example.com/install.sh | sh");
});

check("a badge (link around an image) and plain images disappear; links keep their text", () => {
  assert.deepEqual(N.parse("[![Sponsor](https://img/x.svg)](https://github.com/sponsors/x)"), []);
  assert.deepEqual(N.inline("see [the docs](https://example.com/a?b=c) now"), [{ t: "text", s: "see the docs now" }]);
  assert.deepEqual(N.inline("![alt](http://x/y.png) text"), [{ t: "text", s: " text" }]);
});

check("HTML in a release body is shown as text, never as elements", () => {
  const doc = fakeDoc();
  const box = doc.createElement("div");
  N.render(doc, box, '<img src=x onerror="alert(1)">\n\n<script>alert(2)</script>\n\n- <b onclick="x()">bold?</b>\n\n`<i>code</i>`');
  const all = [...tags(box)];
  assert.deepEqual(all.filter((t) => !["p", "ul", "li", "code"].includes(t)), []);
  assert.match(box.textContent, /<img src=x onerror="alert\(1\)">/);
  assert.match(box.textContent, /<script>alert\(2\)<\/script>/);
  assert.match(box.textContent, /<b onclick="x\(\)">bold\?<\/b>/);
});

check("only a small set of element names is ever created", () => {
  const doc = fakeDoc();
  const box = doc.createElement("div");
  N.render(doc, box, REAL + "\n*italic* and `code`\n");
  const all = [...tags(box)].sort();
  assert.deepEqual(all.filter((t) => !["h4", "p", "ul", "li", "pre", "strong", "em", "code"].includes(t)), []);
  assert.ok(all.includes("h4") && all.includes("ul") && all.includes("pre"));
});

check("rendering twice replaces the previous content", () => {
  const doc = fakeDoc();
  const box = doc.createElement("div");
  N.render(doc, box, "one");
  N.render(doc, box, "two");
  assert.equal(box.textContent, "two");
});

check("empty, nullish and CRLF input are fine; an unclosed fence does not swallow the page", () => {
  assert.deepEqual(N.parse(""), []);
  assert.deepEqual(N.parse(null), []);
  assert.equal(N.parse("a\r\n\r\nb").length, 2);
  assert.equal(N.parse("```\nx\ny").at(-1).k, "pre");
});

check("the notes renderer never touches innerHTML", () => {
  for (const f of ["notes.js", "updates.js"]) {
    const src = readFileSync(new URL(f, import.meta.url), "utf8");
    assert.ok(!/innerHTML|outerHTML|insertAdjacentHTML|document\.write/.test(src), f);
  }
});

if (failures) { console.log(`\n${failures} notes test(s) failed`); process.exit(1); }
console.log("\nall notes tests passed");
