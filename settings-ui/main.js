const invoke = window.__TAURI__.core.invoke;
const BOOLS = ["close_to_tray", "start_minimized", "autostart", "notifications", "hotkey_enabled", "menu_bar_icon", "notify_when_focused", "notify_active_chat", "respect_muted"];
// The menu-bar icon can only be hidden on macOS, where the Dock icon still
// brings the window back (issue #44).
const IS_MAC = /Mac/.test(navigator.platform || navigator.userAgent);
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
  document.getElementById("menu_bar_icon_row").hidden = !IS_MAC;
  if (/Win/.test(navigator.platform || navigator.userAgent)) {
    const startupRow = document.getElementById("start_minimized").closest("label");
    startupRow.querySelector(".row-title").textContent = "Iniciar minimizado com o Windows";
    startupRow.querySelector(".row-subtitle").textContent = "Vale apenas ao entrar no Windows. Ao abrir pelo atalho, a janela sempre aparece.";
  }
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
      note.textContent = "Salvo com observação: " + warn;
      setTimeout(() => (note.textContent = ""), 6000);
    } else {
      note.textContent = "Alterações salvas ✓";
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
  note.textContent = 'Aguarde…';
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

function accountMenu(a, accounts, anchor) {
  const previous=document.getElementById("account-actions");if(previous){if(previous.dataset.open==="true")previous.hidePopover();previous.remove();}
  const menu = document.createElement("div"); menu.id="account-actions";menu.className="account-actions";
  const nativePopover=typeof menu.showPopover==="function";
  if(nativePopover)menu.setAttribute("popover", "auto");
  else {menu.style.position="fixed";menu.style.zIndex="9999";menu.showPopover=()=>{menu.style.display="block";menu.dataset.open="true";};menu.hidePopover=()=>{menu.style.display="none";menu.dataset.open="false";anchor.focus({preventScroll:true});};}menu.setAttribute("role", "menu");menu.setAttribute("aria-label", "Ações de " + a.name);
  const actions = [
    ["Abrir conta", () => accountAction(() => invoke("open_account", {id:a.id}))],
    ...( /Win/.test(navigator.platform || navigator.userAgent) ? [[a.detached ? "Reintegrar à janela" : "Abrir em janela separada", () => accountAction(() => invoke(a.detached?"attach_account":"detach_account",{id:a.id}))]] : []),
    ["Usar ao iniciar",()=>accountAction(()=>invoke("set_default_account",{id:a.id}))],
    ["Renomear",async()=>{const name=await Dlg.prompt("",{title:"Renomear conta",value:a.name,okLabel:"Renomear",requiredMessage:"Informe um nome."});if(name!==null&&name!==a.name)await accountAction(()=>invoke("rename_account",{id:a.id,name}));}],
  ];
  const index=accounts.findIndex(x=>x.id===a.id);
  for(const [delta,label] of [[-1,"Mover para cima"],[1,"Mover para baixo"]]) {
    if(index+delta<0||index+delta>=accounts.length)continue;
    actions.push([label,()=>accountAction(()=>{const ids=accounts.map(x=>x.id);[ids[index],ids[index+delta]]=[ids[index+delta],ids[index]];return invoke("reorder_accounts",{ids});})]);
  }
  if(accounts.length>1)actions.push(["Remover conta",async()=>{if(await Dlg.confirm("A sessão local desta conta será apagada. Para usá-la novamente, você precisará ler o QR code.",{title:`Remover "${a.name}"?`,okLabel:"Remover",danger:true}))await accountAction(()=>invoke("remove_account",{id:a.id}));}]);
  for(const [label,action] of actions){const button=document.createElement("button");button.type="button";button.textContent=label;button.setAttribute("role","menuitem");if(label==="Remover conta")button.className="danger";button.onclick=()=>{menu.hidePopover();menu.dataset.open="false";action();};menu.appendChild(button);}
  const lifecycle=new AbortController();
  const originalHide=menu.hidePopover.bind(menu);
  function afterClose(){menu.dataset.open="false";anchor.setAttribute("aria-expanded","false");lifecycle.abort();if(anchor.isConnected&&!document.querySelector('.dlg-backdrop,.account-actions[data-open="true"]'))anchor.focus({preventScroll:true});}
  menu.hidePopover=()=>{originalHide();afterClose();};
  function place(){
    const r=anchor.getBoundingClientRect(),m=menu.getBoundingClientRect(),padding=12,gap=6;
    const pane=document.querySelector('.prefs').getBoundingClientRect();
    if(!anchor.isConnected||r.bottom<pane.top||r.top>pane.bottom){menu.hidePopover();return;}
    const below=Math.max(0,innerHeight-padding-r.bottom-gap),above=Math.max(0,r.top-gap-padding),natural=menu.scrollHeight+2;
    const down=below>=natural||below>=above;
    menu.style.maxHeight=Math.max(40,Math.min(innerHeight-24,down?below:above))+"px";
    const height=menu.getBoundingClientRect().height;
    const top=down?r.bottom+gap:r.top-height-gap;
    menu.style.left=Math.max(padding,Math.min(r.right-m.width,innerWidth-m.width-padding))+"px";
    menu.style.top=Math.max(padding,Math.min(top,innerHeight-height-padding))+"px";
  }
  menu.addEventListener("toggle",e=>{menu.dataset.open=String(e.newState==="open");anchor.setAttribute("aria-expanded",String(e.newState==="open"));if(e.newState==="closed")afterClose();});
  menu.addEventListener("keydown",e=>{
    if(e.key==="Escape"){e.preventDefault();menu.hidePopover();return;}
    const buttons=[...menu.querySelectorAll("button")],at=buttons.indexOf(document.activeElement);
    if(["ArrowDown","ArrowUp","Home","End"].includes(e.key)){e.preventDefault();const next=e.key==="Home"?0:e.key==="End"?buttons.length-1:(at+(e.key==="ArrowDown"?1:-1)+buttons.length)%buttons.length;buttons[next].focus();}
  });
  window.addEventListener("resize",place,{signal:lifecycle.signal});
  document.addEventListener("scroll",place,{capture:true,signal:lifecycle.signal});
  if(!nativePopover)document.addEventListener("pointerdown",e=>{if(!menu.contains(e.target)&&!anchor.contains(e.target))menu.hidePopover();},{signal:lifecycle.signal});
  document.body.appendChild(menu);menu.showPopover();menu.dataset.open="true";anchor.setAttribute("aria-expanded","true");place();menu.querySelector("button").focus();

}
function renderAccounts(accounts) {
  const list=document.getElementById("accounts-list"),startup=document.getElementById("startup_account");
  if(document.getElementById("account-actions")?.dataset.open==="true")return;
  list.replaceChildren();startup.replaceChildren();
  for(const a of accounts){
    const option=document.createElement("option");option.value=a.id;option.textContent=a.name;option.selected=a.is_default;startup.appendChild(option);
    const row=document.createElement("div");row.className="account-row";
    const avatar=document.createElement("span");avatar.className="account-avatar";avatar.setAttribute("aria-hidden","true");avatar.textContent=a.name.trim().split(/\s+/).slice(0,2).map(x=>x[0]).join("").toUpperCase();
    const open=document.createElement("button");open.type="button";open.className="account-open";open.setAttribute("aria-label","Abrir "+a.name);
    const name=document.createElement("span");name.className="acct-name";name.textContent=a.name;
    const info=document.createElement("span");info.className="acct-subtitle";info.textContent=a.detached?"Em janela separada":a.is_default?"Conta ao iniciar":"WhatsApp";open.append(name,info);open.onclick=()=>accountAction(()=>invoke("open_account",{id:a.id}));
    row.append(avatar,open);
    if(a.unread){const badge=document.createElement("span");badge.className="acct-unread";badge.textContent=a.unread;badge.title=a.unread+" conversas não lidas";row.appendChild(badge);}
    const more=document.createElement("button");more.type="button";more.className="account-more";more.textContent="⋮";more.setAttribute("aria-label","Opções de "+a.name);more.setAttribute("aria-haspopup","menu");more.onclick=()=>accountMenu(a,accounts,more);row.appendChild(more);list.appendChild(row);
  }
}

async function loadAccounts() {
  try {
    const accounts = await invoke("list_accounts");
    renderAccounts(accounts);
  } catch (e) {
    document.getElementById('accounts-note').textContent = 'Não foi possível carregar as contas: ' + String(e);
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
    document.getElementById("biometric_label").textContent = "Usar " + s.biometric_label;
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
    } catch (e) { await Dlg.alert(String(e), { title: "Não foi possível ativar o bloqueio" }); }
  });

  document.getElementById("lock_now").addEventListener("click", async () => {
    try { await invoke("lock_app"); } catch (e) { await Dlg.alert(String(e), { title: "Não foi possível bloquear" }); }
  });

  document.getElementById("change_pw").addEventListener("click", async () => {
    const current = await Dlg.prompt("", {
      title: "Senha atual",
      password: true,
      okLabel: "Continuar",
      requiredMessage: "Informe sua senha atual.",
    });
    if (current === null) return;
    const next = await Dlg.prompt("Pelo menos 4 caracteres.", {
      title: "Nova senha",
      password: true,
      okLabel: "Alterar",
      requiredMessage: "Informe uma nova senha.",
    });
    if (next === null) return;
    try {
      await invoke("change_app_lock_password", { current, new: next, confirm: next });
      await Dlg.alert("Sua senha de bloqueio foi alterada.", { title: "Senha alterada" });
    } catch (e) { await Dlg.alert(String(e), { title: "Não foi possível alterar a senha" }); }
  });

  document.getElementById("disable_lock").addEventListener("click", async () => {
    const current = await Dlg.prompt("Informe sua senha atual para desativar o bloqueio.", {
      title: "Desativar bloqueio",
      password: true,
      okLabel: "Desativar",
      danger: true,
      requiredMessage: "Informe sua senha atual.",
    });
    if (current === null) return;
    try {
      await invoke("disable_app_lock", { current });
      await loadLock();
    } catch (e) { await Dlg.alert(String(e), { title: "Não foi possível desativar o bloqueio" }); }
  });

  document.getElementById("biometric_enabled").addEventListener("change", async (e) => {
    try {
      await invoke("set_biometric_enabled", { enabled: e.target.checked });
    } catch (err) {
      e.target.checked = !e.target.checked; // revert on failure
      await Dlg.alert(String(err), { title: "Não foi possível alterar a biometria" });
    }
    await loadLock();
  });

  const reportLockOptionFailure = (e) => Dlg.alert(String(e), { title: "Não foi possível salvar" });
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
  btn.textContent = "Gravar";
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
  if (accel === null) { setHotkeyHint("Tecla não suportada. Tente outra."); return; }
  if (!window.HotkeyFmt.isValidCombo(mods, e.code)) {
    setHotkeyHint("Inclua Ctrl, Alt ou Shift.");
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
  btn.textContent = "Pressione as teclas… (Esc cancela)";
  btn.classList.add("recording");
  setHotkeyHint("");
  window.addEventListener("keydown", onRecordKeydown, true);
  window.addEventListener("blur", onRecordBlur, true);
}
