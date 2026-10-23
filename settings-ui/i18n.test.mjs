// Run: node settings-ui/i18n.test.mjs
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import assert from "node:assert/strict";

const require = createRequire(import.meta.url);
const I18n = require("./i18n.js");
const { EN, norm, t } = I18n;
const dir = new URL(".", import.meta.url);
const read = (f) => readFileSync(new URL(f, dir), "utf8");

let failures = 0;
function check(name, fn) {
  try { fn(); console.log("  ok  " + name); }
  catch (e) { failures++; console.log("  FAIL " + name + "\n       " + e.message.split("\n").join("\n       ")); }
}

// Text that is the same in both languages, or a proper name.
const SAME = new Set(["whatRust", "WhatsApp", "OK", "Português (Brasil)", "English", "⇄", "⋮", "⚙",
  "CmdOrCtrl+Shift+W", "whatrust --toggle", "Esc"]);

function visibleTexts(html) {
  html = html.replace(/<script[\s\S]*?<\/script>/g, "").replace(/<style[\s\S]*?<\/style>/g, "")
    .replace(/<!--[\s\S]*?-->/g, "").replace(/<code>[\s\S]*?<\/code>/g, "<code></code>");
  const out = [];
  for (const m of html.matchAll(/>([^<>]+)</g)) {
    const s = norm(m[1].replace(/&[a-z]+;/g, " "));
    if (s) out.push(s);
  }
  for (const m of html.matchAll(/\b(?:aria-label|title|placeholder)="([^"]+)"/g)) out.push(norm(m[1]));
  return out;
}

check("English is the identity for Portuguese and translates known keys", () => {
  assert.equal(t("Salvar"), "Salvar"); // default language is pt-BR
  assert.equal(t("Abrir {0}", ["Ana"]), "Abrir Ana");
});

check("placeholders match between Portuguese and English", () => {
  for (const [pt, en] of Object.entries(EN)) {
    for (let i = 0; i < 4; i++) {
      assert.equal(pt.includes(`{${i}}`), en.includes(`{${i}}`), `placeholder {${i}} in "${pt}"`);
    }
  }
});

check("every static string in the HTML pages has an English entry", () => {
  const missing = [];
  for (const f of ["index.html", "lock.html", "shell.html"]) {
    for (const s of visibleTexts(read(f))) {
      if (!(s in EN) && !SAME.has(s)) missing.push(`${f}: ${s}`);
    }
  }
  assert.deepEqual(missing, []);
});

check("every t(\"…\") literal in the scripts has an English entry", () => {
  const missing = [];
  for (const f of ["main.js", "panel.js", "lock.js", "shell.js", "dialog.js", "hotkey.js"]) {
    for (const m of read(f).matchAll(/\bt\(\s*(["'])((?:\\.|(?!\1).)*)\1/g)) {
      const s = norm(m[2].replace(/\\(.)/g, "$1"));
      if (!(s in EN)) missing.push(`${f}: ${s}`);
    }
  }
  assert.deepEqual(missing, []);
});

check("strings assigned to the page by the scripts have an English entry", () => {
  // textContent = "…" / Dlg.*("…", {title: "…", okLabel: "…"}) literals.
  const missing = [];
  const re = /(?:textContent\s*=\s*|title\s*:\s*|okLabel\s*:\s*|requiredMessage\s*:\s*|Dlg\.(?:alert|confirm|prompt)\(\s*)(["'])((?:\\.|(?!\1).)*[A-Za-zÀ-ú](?:\\.|(?!\1).)*)\1/g;
  for (const f of ["main.js", "lock.js", "panel.js"]) {
    for (const m of read(f).matchAll(re)) {
      const s = norm(m[2].replace(/\\(.)/g, "$1"));
      if (!/[À-ú]|\s/.test(s) && !(s in EN)) {
        // single-word ASCII strings (ids, "Mac") are not UI text unless in EN already
        continue;
      }
      if (!(s in EN) && !SAME.has(s)) missing.push(`${f}: ${s}`);
    }
  }
  assert.deepEqual(missing, []);
});

check("both language options are offered", () => {
  const html = read("index.html");
  assert.match(html, /<option value="pt-BR">/);
  assert.match(html, /<option value="en">/);
});

if (failures) { console.log(`\n${failures} i18n test(s) failed`); process.exit(1); }
console.log("\nall i18n tests passed");
