// Pure helpers: turn a captured key combo into a Tauri accelerator string.
// Assigned to globalThis.HotkeyFmt so the page (window.HotkeyFmt, since
// window === globalThis in a browser) and the node test share ONE source.
// MUST NOT reference `window` (undefined under node).
(function () {
  const FN_KEY = /^F([1-9]|1[0-9]|2[0-4])$/;

  function keyToken(code) {
    if (/^Key[A-Z]$/.test(code)) return code.slice(3);   // KeyW -> W
    if (/^Digit[0-9]$/.test(code)) return code.slice(5); // Digit1 -> 1
    if (FN_KEY.test(code)) return code;                  // F8 -> F8
    switch (code) {
      case "ArrowUp": return "Up";
      case "ArrowDown": return "Down";
      case "ArrowLeft": return "Left";
      case "ArrowRight": return "Right";
      case "Space": return "Space";
      default: return null;                              // unmapped
    }
  }

  // mods: {ctrl, alt, shift, meta}; code: KeyboardEvent.code.
  // Returns the accelerator string, or null if the key is unmapped.
  function comboToAccelerator(mods, code) {
    const key = keyToken(code);
    if (key === null) return null;
    const parts = [];
    if (mods.ctrl) parts.push("CmdOrCtrl");
    if (mods.alt) parts.push("Alt");
    if (mods.shift) parts.push("Shift");
    if (mods.meta) parts.push("Super");
    parts.push(key);
    return parts.join("+");
  }

  // Accept only if there is a modifier, or the key itself is a function key.
  function isValidCombo(mods, code) {
    if (mods.ctrl || mods.alt || mods.shift || mods.meta) return true;
    return FN_KEY.test(code);
  }

  // The stored accelerator says CmdOrCtrl (Tauri syntax); the fields show the
  // friendlier Cmd/Ctrl and convert back on save.
  function toDisplay(accel) {
    return String(accel).replace(/CmdOrCtrl/g, "Cmd/Ctrl");
  }
  function fromDisplay(text) {
    return String(text).replace(/Cmd\s*\/\s*Ctrl/gi, "CmdOrCtrl");
  }

  globalThis.HotkeyFmt = { comboToAccelerator, isValidCombo, toDisplay, fromDisplay, FN_KEY };
})();
