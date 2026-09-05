const invoke = window.__TAURI__.core.invoke;
const BOOLS = ["close_to_tray", "start_minimized", "autostart", "notifications", "hotkey_enabled"];
const ZOOM_DEFAULT = 1;

// settings.json holds a zoom factor, not a preset name, so a stored value need
// not be one of the four options offered here — it can come from a hand edit or
// from a build with a different set of presets. Show the closest one.
function selectNearestZoom(value) {
  const sel = document.getElementById("zoom");
  const distance = (o) => Math.abs(parseFloat(o.value) - value);
  sel.value = [...sel.options].reduce((a, b) => (distance(b) < distance(a) ? b : a)).value;
}

async function load() {
  const s = await invoke("get_settings");
  for (const f of BOOLS) {
    const el = document.getElementById(f);
    if (el) el.checked = !!s[f];
  }
  document.getElementById("hotkey").value = s.hotkey || "CmdOrCtrl+Shift+W";
  selectNearestZoom(Number.isFinite(s.zoom) ? s.zoom : ZOOM_DEFAULT);
}

async function save() {
  const s = await invoke("get_settings");
  for (const f of BOOLS) {
    const el = document.getElementById(f);
    if (el) s[f] = el.checked;
  }
  const hk = document.getElementById("hotkey").value.trim();
  s.hotkey = hk || "CmdOrCtrl+Shift+W";
  s.zoom = parseFloat(document.getElementById("zoom").value) || ZOOM_DEFAULT;
  const note = document.getElementById("note");
  try {
    const warn = await invoke("set_settings", { settings: s });
    if (warn) {
      note.textContent = "Saved with warning: " + warn;
      setTimeout(() => (note.textContent = ""), 6000);
    } else {
      note.textContent = "Saved ✓";
      setTimeout(() => (note.textContent = ""), 1500);
    }
  } catch (e) {
    note.textContent = String(e);
  }
}

// --- Accounts ---
let accountsBusy = false;

async function accountAction(action) {
  if (accountsBusy) return;
  accountsBusy = true;
  document.querySelectorAll('#accounts-list button, .add-row input, .add-row button, #startup_account')
    .forEach(el => { el.disabled = true; });
  const note = document.getElementById('accounts-note');
  note.textContent = 'Working…';
  try {
    await action();
    note.textContent = '';
  } catch (error) {
    note.textContent = String(error);
  } finally {
    accountsBusy = false;
    document.getElementById('new_account_name').disabled = false;
    document.getElementById('add_account').disabled = false;
    document.getElementById('startup_account').disabled = false;
    await loadAccounts();
  }
}

function renderAccounts(accounts) {
  const list = document.getElementById("accounts-list");
  list.textContent = "";
  const canRemove = accounts.length > 1;
  const startup = document.getElementById('startup_account');
  startup.textContent = '';

  for (const a of accounts) {
    const row = document.createElement("div");
    row.className = "account-row";

    const name = document.createElement("span");
    name.className = "acct-name";
    name.textContent = a.name;
    row.appendChild(name);
    const option = document.createElement('option');
    option.value = a.id;
    option.textContent = a.name;
    option.selected = a.is_default;
    startup.appendChild(option);

    if (a.unread > 0) {
      const badge = document.createElement("span");
      badge.className = "acct-unread";
      badge.textContent = String(a.unread);
      row.appendChild(badge);
    }

    const openBtn = document.createElement("button");
    openBtn.className = "secondary";
    openBtn.textContent = "Open";
    openBtn.addEventListener("click", async () => {
      await accountAction(() => invoke("open_account", { id: a.id }));
    });
    row.appendChild(openBtn);

    const renameBtn = document.createElement("button");
    renameBtn.className = "secondary";
    renameBtn.textContent = "Rename";
    renameBtn.addEventListener("click", async () => {
      const next = await Dlg.prompt("", {
        title: "Rename account",
        value: a.name,
        okLabel: "Rename",
        requiredMessage: "An account needs a name.",
      });
      if (next === null || next === a.name) return;
      await accountAction(() => invoke("rename_account", { id: a.id, name: next }));
    });
    row.appendChild(renameBtn);

    const removeBtn = document.createElement("button");
    removeBtn.className = "secondary";
    removeBtn.textContent = "Remove";
    removeBtn.disabled = !canRemove;
    removeBtn.addEventListener("click", async () => {
      const ok = await Dlg.confirm(`Its local session will be deleted, and you will need to scan the QR code again to use this number.`, {
        title: `Remove "${a.name}"?`,
        okLabel: "Remove",
        danger: true,
      });
      if (!ok) return;
      await accountAction(() => invoke("remove_account", { id: a.id }));
    });
    row.appendChild(removeBtn);

    list.appendChild(row);
  }
}

async function loadAccounts() {
  try {
    const accounts = await invoke("list_accounts");
    renderAccounts(accounts);
  } catch (e) {
    document.getElementById('accounts-note').textContent = 'Could not load accounts: ' + String(e);
  }
}

async function addAccount() {
  const input = document.getElementById("new_account_name");
  const name = input.value.trim();
  if (!name || accountsBusy) return;
  await accountAction(async () => {
    await invoke("add_account", { name });
    input.value = "";
  });
}

window.addEventListener("DOMContentLoaded", () => {
  load();
  loadAccounts();
  loadLock();
  wireLock();
  document.getElementById("save").addEventListener("click", save);
  document.getElementById("add_account").addEventListener("click", addAccount);
  document.getElementById('startup_account').addEventListener('change', event => {
    const id = event.target.value;
    accountAction(() => invoke('set_default_account', { id }));
  });
  document.getElementById("new_account_name").addEventListener("keydown", (e) => {
    if (e.key === "Enter") addAccount();
  });
  document.getElementById("record_hotkey").addEventListener("click", toggleRecording);
  // Zoom saves on change rather than on Save: picking a size you cannot see the
  // effect of is guesswork, and the webview applies it immediately.
  document.getElementById("zoom").addEventListener("change", save);
});

// --- App lock ---

async function loadLock() {
  let s;
  try {
    s = await invoke("get_lock_status");
  } catch (e) {
    return;
  }
  const disabled = document.getElementById("lock-disabled");
  const enabled = document.getElementById("lock-enabled");
  disabled.hidden = s.enabled;
  enabled.hidden = !s.enabled;

  if (s.enabled) {
    const row = document.getElementById("biometric_row");
    row.hidden = !s.biometric_available;
    document.getElementById("biometric_label").textContent = "Use " + s.biometric_label;
    document.getElementById("biometric_enabled").checked = s.biometric_enabled;
    document.getElementById("lock_on_launch").checked = s.lock_on_launch;
    document.getElementById("lock_on_hide").checked = s.lock_on_hide;
    document.getElementById("idle_min").value = String(Math.round(s.idle_secs / 60));
  }
}

async function saveLockOptions() {
  const idleMin = parseInt(document.getElementById("idle_min").value, 10) || 0;
  await invoke("set_app_lock_options", {
    lockOnLaunch: document.getElementById("lock_on_launch").checked,
    lockOnHide: document.getElementById("lock_on_hide").checked,
    idleSecs: Math.max(0, idleMin) * 60,
  });
}

function wireLock() {
  document.getElementById("enable_lock").addEventListener("click", async () => {
    const a = document.getElementById("lock_pw1").value;
    const b = document.getElementById("lock_pw2").value;
    try {
      await invoke("set_app_lock_password", { new: a, confirm: b });
      document.getElementById("lock_pw1").value = "";
      document.getElementById("lock_pw2").value = "";
      await loadLock();
    } catch (e) { await Dlg.alert(String(e), { title: "Could not enable the lock" }); }
  });

  document.getElementById("lock_now").addEventListener("click", async () => {
    try { await invoke("lock_app"); } catch (e) { await Dlg.alert(String(e), { title: "Could not lock" }); }
  });

  document.getElementById("change_pw").addEventListener("click", async () => {
    const current = await Dlg.prompt("", {
      title: "Current password",
      password: true,
      okLabel: "Continue",
      requiredMessage: "Enter your current password.",
    });
    if (current === null) return;
    const next = await Dlg.prompt("At least 4 characters.", {
      title: "New password",
      password: true,
      okLabel: "Change",
      requiredMessage: "Enter a new password.",
    });
    if (next === null) return;
    try {
      await invoke("change_app_lock_password", { current, new: next, confirm: next });
      await Dlg.alert("Your app-lock password has been changed.", { title: "Password changed" });
    } catch (e) { await Dlg.alert(String(e), { title: "Could not change the password" }); }
  });

  document.getElementById("disable_lock").addEventListener("click", async () => {
    const current = await Dlg.prompt("Enter your current password to turn the app lock off.", {
      title: "Disable app lock",
      password: true,
      okLabel: "Disable",
      danger: true,
      requiredMessage: "Enter your current password.",
    });
    if (current === null) return;
    try {
      await invoke("disable_app_lock", { current });
      await loadLock();
    } catch (e) { await Dlg.alert(String(e), { title: "Could not disable the lock" }); }
  });

  document.getElementById("biometric_enabled").addEventListener("change", async (e) => {
    try {
      await invoke("set_biometric_enabled", { enabled: e.target.checked });
    } catch (err) {
      e.target.checked = !e.target.checked; // revert on failure
      await Dlg.alert(String(err), { title: "Could not change biometric unlock" });
    }
    await loadLock();
  });

  const reportLockOptionFailure = (e) => Dlg.alert(String(e), { title: "Could not save" });
  for (const id of ["lock_on_launch", "lock_on_hide"]) {
    document.getElementById(id).addEventListener("change", () => saveLockOptions().catch(reportLockOptionFailure));
  }
  document.getElementById("idle_min").addEventListener("change", () => saveLockOptions().catch(reportLockOptionFailure));
}

// --- Shortcut recorder ---

let recordingHotkey = false;
let hotkeyPrev = "";
const MODIFIER_KEYS = new Set(["Control", "Shift", "Alt", "Meta"]);

function setHotkeyHint(msg) {
  const hint = document.getElementById("hotkey_hint");
  if (!msg) { hint.hidden = true; hint.textContent = ""; }
  else { hint.textContent = msg; hint.hidden = false; }
}

function stopRecording(restore) {
  if (!recordingHotkey) return;
  recordingHotkey = false;
  window.removeEventListener("keydown", onRecordKeydown, true);
  window.removeEventListener("blur", onRecordBlur, true);
  const btn = document.getElementById("record_hotkey");
  btn.textContent = "Record";
  btn.classList.remove("recording");
  if (restore) document.getElementById("hotkey").value = hotkeyPrev;
}

function onRecordBlur() { setHotkeyHint(""); stopRecording(true); }

function onRecordKeydown(e) {
  e.preventDefault();
  e.stopPropagation();
  if (MODIFIER_KEYS.has(e.key)) return;            // ignore bare modifiers
  if (e.key === "Escape") { setHotkeyHint(""); stopRecording(true); return; }
  const mods = { ctrl: e.ctrlKey, alt: e.altKey, shift: e.shiftKey, meta: e.metaKey };
  const accel = window.HotkeyFmt.comboToAccelerator(mods, e.code);
  if (accel === null) { setHotkeyHint("Unsupported key — try another."); return; }
  if (!window.HotkeyFmt.isValidCombo(mods, e.code)) {
    setHotkeyHint("Add a modifier (Ctrl / Alt / Shift).");
    return;
  }
  document.getElementById("hotkey").value = accel;
  setHotkeyHint("");
  stopRecording(false);
}

function toggleRecording() {
  if (recordingHotkey) { setHotkeyHint(""); stopRecording(true); return; }
  recordingHotkey = true;
  hotkeyPrev = document.getElementById("hotkey").value;
  const btn = document.getElementById("record_hotkey");
  btn.textContent = "Press keys… (Esc to cancel)";
  btn.classList.add("recording");
  setHotkeyHint("");
  window.addEventListener("keydown", onRecordKeydown, true);
  window.addEventListener("blur", onRecordBlur, true);
}
