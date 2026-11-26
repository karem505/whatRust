// Run: node settings-ui/profiles.test.mjs
import { createRequire } from "node:module";
import assert from "node:assert/strict";

const P = createRequire(import.meta.url)("./profiles.js");
let failures = 0;
function check(name, fn) {
  try { fn(); console.log("  ok  " + name); }
  catch (e) { failures++; console.log("  FAIL " + name + "\n       " + e.message); }
}

check("initial is the upper-cased first letter", () => {
  assert.equal(P.initial("brian"), "B");
  assert.equal(P.initial("  work"), "W");
  assert.equal(P.initial("éva"), "É");
});
check("initial keeps a whole emoji and survives empty names", () => {
  assert.equal(P.initial("😀 fun"), "😀");
  assert.equal(P.initial(""), "?");
  assert.equal(P.initial(null), "?");
});
check("badge text hides zero and caps at 99+", () => {
  assert.equal(P.badgeText(0), "");
  assert.equal(P.badgeText(undefined), "");
  assert.equal(P.badgeText(7), "7");
  assert.equal(P.badgeText(99), "99");
  assert.equal(P.badgeText(100), "99+");
});
check("sorted follows order, then name, without mutating the input", () => {
  const input = [{ name: "b", order: 2 }, { name: "z", order: 1 }, { name: "a", order: 1 }];
  assert.deepEqual(P.sorted(input).map((a) => a.name), ["a", "z", "b"]);
  assert.equal(input[0].name, "b");
});
if (failures) process.exit(1);
