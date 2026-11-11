// Zero-dependency assertion script for the pure hotkey formatter.
// Run: node settings-ui/comboToAccelerator.test.mjs   (exit non-zero on failure)
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
// hotkey.js assigns globalThis.HotkeyFmt; run it in this global scope.
(0, eval)(readFileSync(join(here, "hotkey.js"), "utf8"));
const { comboToAccelerator, isValidCombo, toDisplay, fromDisplay } = globalThis.HotkeyFmt;

let failures = 0;
function eq(actual, expected, label) {
  const a = JSON.stringify(actual), e = JSON.stringify(expected);
  if (a !== e) { console.error(`FAIL ${label}: got ${a}, want ${e}`); failures++; }
  else { console.log(`ok   ${label}`); }
}
const M = (o = {}) => ({ ctrl: false, alt: false, shift: false, meta: false, ...o });

eq(comboToAccelerator(M({ ctrl: true, shift: true }), "KeyW"), "CmdOrCtrl+Shift+W", "Ctrl+Shift+W");
eq(comboToAccelerator(M(), "F8"), "F8", "bare F8");
eq(comboToAccelerator(M({ shift: true }), "Digit1"), "Shift+1", "Shift+Digit1 stays 1 (code-based)");
eq(comboToAccelerator(M({ meta: true }), "KeyK"), "Super+K", "Meta+K -> Super+K");
eq(comboToAccelerator(M({ ctrl: true, alt: true, shift: true }), "KeyP"), "CmdOrCtrl+Alt+Shift+P", "modifier order");
eq(comboToAccelerator(M({ ctrl: true }), "ArrowUp"), "CmdOrCtrl+Up", "Ctrl+ArrowUp -> CmdOrCtrl+Up");
eq(comboToAccelerator(M(), "Comma"), null, "unmapped key -> null");

eq(toDisplay("CmdOrCtrl+Shift+W"), "Cmd/Ctrl+Shift+W", "display shows Cmd/Ctrl");
eq(fromDisplay("Cmd/Ctrl+Shift+W"), "CmdOrCtrl+Shift+W", "save converts back to CmdOrCtrl");
eq(fromDisplay(toDisplay("CmdOrCtrl+Alt+F8")), "CmdOrCtrl+Alt+F8", "display round-trips");
eq(fromDisplay("cmd / ctrl+K"), "CmdOrCtrl+K", "typed variants are accepted");
eq(fromDisplay("CmdOrCtrl+K"), "CmdOrCtrl+K", "stored form passes through");

eq(isValidCombo(M(), "KeyW"), false, "bare letter invalid");
eq(isValidCombo(M({ shift: true }), "KeyW"), true, "Shift+letter valid (Shift counts)");
eq(isValidCombo(M(), "F8"), true, "bare F-key valid");
eq(isValidCombo(M({ ctrl: true }), "KeyW"), true, "Ctrl+letter valid");

if (failures) { console.error(`\n${failures} failing assertion(s)`); process.exit(1); }
console.log("\nall hotkey tests passed");
