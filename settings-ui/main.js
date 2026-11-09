const invoke = window.__TAURI__.core.invoke;
const LANGS = ["pt-BR", "en"];
const BOOLS = ["close_to_tray", "start_minimized", "autostart", "notifications", "hotkey_enabled", "blur_hotkey_enabled", "menu_bar_icon", "menubar_only", "hide_dock_icon", "notify_when_focused", "notify_active_chat", "respect_muted", "single_window", "auto_update_check", "auto_update_install"];
const ZOOM_DEFAULT = 1;
const SINGLE_WINDOW_SUBTITLE = "Uma janela com um seletor de contas; as contas ocultas continuam conectadas. Requer reiniciar o whatRust.";

// The menu-bar rows are macOS-only. get_settings reports the OS this build targets
// in `platform`; the rows start hidden and appear here once it is known.
function setMacRows(visible) {
  document.querySelectorAll(".mac-only").forEach((el) => (el.hidden = !visible));
}

// The "Show menu bar icon" subtitle as authored in the markup; kept so it can
// be restored when menu-bar-only is turned back off.
const MENU_ICON_SUBTITLE = "Quando desativado, clique no ícone do Dock para abrir o whatRust";

// In menu-bar-only mode the tray icon is the app's only entry point and the
// backend keeps it visible, so the switch is locked on.
let menuBarIconChecked = true;

function syncMenuBarIconRow() {
  const only = document.getElementById("menubar_only").checked || document.getElementById("hide_dock_icon").checked;
  const icon = document.getElementById("menu_bar_icon");
  icon.disabled = only;
  icon.checked = only ? true : menuBarIconChecked;
  document.getElementById("menu_bar_icon_subtitle").textContent = only
    ? "O ícone da bandeja é o único ponto de entrada no modo somente barra de menus, por isso permanece visível"
    : MENU_ICON_SUBTITLE;
}

// settings.json holds a zoom factor, not a preset name, and the zoom shortcuts
// (Ctrl +/-/wheel) land on levels the four presets do not offer. Show an exact
// match when there is one; otherwise add a temporary "NN%" option so the control
// reflects the real value instead of pretending it is the nearest preset.
function selectNearestZoom(value) {
  const sel = document.getElementById("zoom");
  for (const o of [...sel.options]) if (o.dataset.custom) o.remove();
  const exact = [...sel.options].find((o) => Math.abs(parseFloat(o.value) - value) < 0.005);
  if (exact) {
    sel.value = exact.value;
    return;
  }
  const opt = document.createElement("option");
  opt.dataset.custom = "1";
  opt.value = String(value);
  opt.textContent = Math.round(value * 100) + "%";
  const after = [...sel.options].find((o) => parseFloat(o.value) > value);
  sel.insertBefore(opt, after || null);
  sel.value = opt.value;
}

// The chosen download folder (null = the system Downloads folder). Kept here
// because save() rebuilds the whole settings object from the page.
let downloadDir = null;

function showDownloadDir() {
  const sub = document.getElementById("download_dir_subtitle");
  sub.textContent = downloadDir || "Pasta Downloads do sistema";
  sub.title = downloadDir || "";
  document.getElementById("reset_download_dir").disabled = !downloadDir;
}

function showIcon(custom) {
  document.getElementById("icon_subtitle").textContent = custom ? "Personalizado" : "Padrão";
  document.getElementById("reset_icon").disabled = !custom;
}

function flash(message, ms = 4000) {
  const note = document.getElementById("note");
  note.textContent = message;
  setTimeout(() => { if (note.textContent === message) note.textContent = ""; }, ms);
}

async function chooseDownloadDir() {
  try {
    const dir = await invoke("pick_download_dir");
    if (!dir) return;
    const previous = downloadDir;
    downloadDir = dir;
    showDownloadDir();
    // save() shows a refusal in the note; then go back to what is stored.
    if (!(await save())) {
      downloadDir = previous;
      showDownloadDir();
    }
  } catch (e) { flash(String(e)); }
}

async function resetDownloadDir() {
  downloadDir = null;
  showDownloadDir();
  await save();
}

async function chooseIcon() {
  try {
    if (await invoke("choose_app_icon")) { showIcon(true); flash("Alterações salvas ✓", 1500); }
  } catch (e) { flash(String(e), 6000); }
}

async function resetIcon() {
  try {
    await invoke("reset_app_icon");
    showIcon(false);
    flash("Alterações salvas ✓", 1500);
  } catch (e) { flash(String(e), 6000); }
}

async function load() {
  const s = await invoke("get_settings");
  setMacRows(s.platform === "macos");
  // Title bar (Linux): auto hides it on tiling window managers like Hyprland.
  document.getElementById("title_bar_row").hidden = s.platform !== "linux";
  document.getElementById("title_bar").value = ["show", "hide"].includes(s.title_bar) ? s.title_bar : "auto";
  document.getElementById("title_bar_subtitle").textContent = s.tiling_wm
    ? t("Detectado: {0}. No modo automático a barra fica oculta.", [s.tiling_wm])
    : "Gerenciadores de janelas lado a lado, como o Hyprland, não usam barra de título.";
  if (/Win/.test(navigator.platform || navigator.userAgent)) {
    const startupRow = document.getElementById("start_minimized").closest("label");
    startupRow.querySelector(".row-title").textContent = "Iniciar minimizado com o Windows";
    startupRow.querySelector(".row-subtitle").textContent = "Vale apenas ao entrar no Windows. Ao abrir pelo atalho, a janela sempre aparece.";
  }
  for (const f of BOOLS) {
    const el = document.getElementById(f);
    if (el) el.checked = !!s[f];
  }
  // Menu-bar-only needs WKWebView's inactive-scheduling policy (macOS 14+).
  // Below that the backend ignores the setting, so don't offer the switch.
  const menubarOnly = document.getElementById("menubar_only");
  menubarOnly.disabled = !s.menubar_supported;
  if (s.platform === "macos" && !s.menubar_supported) {
    menubarOnly.checked = false;
    document.getElementById("menubar_only_subtitle").textContent =
      "Requer macOS 14 (Sonoma) ou superior — este Mac não consegue manter um painel oculto conectado";
  }
  // Single-window mode (Linux/macOS): applies after a restart; menu-bar-only wins.
  const swRow = document.getElementById("single_window_row");
  swRow.hidden = !s.single_window_supported;
  const swNote = document.getElementById("single_window_subtitle");
  if (s.single_window_supported && !!s.single_window !== !!s.single_window_active) {
    swNote.textContent = s.single_window && s.menubar_only
      ? "Reinicie o whatRust para aplicar. O modo somente barra de menus tem prioridade sobre esta opção."
      : "Reinicie o whatRust para aplicar a mudança.";
  } else {
    swNote.textContent = SINGLE_WINDOW_SUBTITLE;
  }
  document.getElementById("hotkey").value = s.hotkey || "CmdOrCtrl+Shift+W";
  document.getElementById("blur_hotkey").value = s.blur_hotkey || "CmdOrCtrl+Shift+B";
  selectNearestZoom(Number.isFinite(s.zoom) ? s.zoom : ZOOM_DEFAULT);
  const lang = LANGS.includes(s.language) ? s.language : "en";
  document.getElementById("language").value = lang;
  if (I18n.lang() !== lang) I18n.setLang(lang);
  menuBarIconChecked = document.getElementById("menu_bar_icon").checked;
  syncMenuBarIconRow();
  downloadDir = s.download_dir || null;
  showDownloadDir();
  showIcon(!!s.custom_icon);
  loadProxy(s);
}

// --- Proxy (issue #52) ---
let proxyInUse = "";
let proxySupported = true;
let proxySaved = { proxy_mode: "off", proxy_host: "", proxy_port: 0 };

function proxyUrl(mode, host, port) {
  if (mode === "off") return "";
  const h = host.includes(":") && !host.startsWith("[") ? "[" + host + "]" : host;
  return mode + "://" + h.toLowerCase() + ":" + port;
}

function syncProxyRows() {
  const off = document.getElementById("proxy_mode").value === "off";
  document.querySelectorAll(".proxy-field").forEach((el) => (el.hidden = off));
}

// The restart hint shows while the saved proxy differs from the one the open
// webviews were created with.
function syncProxyRestart() {
  const saved = proxyUrl(proxySaved.proxy_mode, proxySaved.proxy_host || "", proxySaved.proxy_port || 0);
  const running = proxyInUse.replace(/\/$/, "");
  document.getElementById("proxy_restart").hidden = saved.replace(/\/$/, "") === running;
}

function loadProxy(s) {
  proxyInUse = s.proxy_in_use || "";
  proxySupported = s.proxy_supported !== false;
  proxySaved = { proxy_mode: s.proxy_mode || "off", proxy_host: s.proxy_host || "", proxy_port: s.proxy_port || 0 };
  const mode = document.getElementById("proxy_mode");
  mode.value = proxySaved.proxy_mode;
  if (!proxySupported) {
    mode.disabled = true;
    mode.value = "off";
  }
  document.getElementById("proxy_host").value = proxySaved.proxy_host;
  document.getElementById("proxy_port").value = proxySaved.proxy_port ? String(proxySaved.proxy_port) : "";
  syncProxyRows();
  syncProxyRestart();
}

async function save() {
  const s = await invoke("get_settings");
  for (const f of BOOLS) {
    const el = document.getElementById(f);
    if (el) s[f] = el.checked;
  }
  const hk = document.getElementById("hotkey").value.trim();
  s.hotkey = hk || "CmdOrCtrl+Shift+W";
  s.blur_hotkey = document.getElementById("blur_hotkey").value.trim() || "CmdOrCtrl+Shift+B";
  s.zoom = parseFloat(document.getElementById("zoom").value) || ZOOM_DEFAULT;
  s.title_bar = document.getElementById("title_bar").value || "auto";
  s.language = document.getElementById("language").value;
  s.download_dir = downloadDir;
  if (proxySupported) {
    s.proxy_mode = document.getElementById("proxy_mode").value;
    s.proxy_host = document.getElementById("proxy_host").value.trim();
    const port = document.getElementById("proxy_port").value.trim();
    // Anything but plain digits becomes 0, which the backend rejects.
    s.proxy_port = /^[0-9]{1,5}$/.test(port) ? parseInt(port, 10) : 0;
  }
  const note = document.getElementById("note");
  try {
    const warn = await invoke("set_settings", { settings: s });
    const saved = await invoke("get_settings");
    proxySaved = { proxy_mode: saved.proxy_mode, proxy_host: saved.proxy_host, proxy_port: saved.proxy_port };
    document.getElementById("proxy_host").value = saved.proxy_host;
    syncProxyRestart();
    if (warn) {
      note.textContent = t("Salvo com observação: {0}", [warn]);
      setTimeout(() => (note.textContent = ""), 6000);
    } else {
      note.textContent = "Alterações salvas ✓";
      setTimeout(() => (note.textContent = ""), 1500);
    }
    return true;
  } catch (e) {
    note.textContent = String(e);
    return false;
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
  else {menu.style.position="fixed";menu.style.zIndex="9999";menu.showPopover=()=>{menu.style.display="block";menu.dataset.open="true";};menu.hidePopover=()=>{menu.style.display="none";menu.dataset.open="false";anchor.focus({preventScroll:true});};}menu.setAttribute("role", "menu");menu.setAttribute("aria-label", t("Ações de {0}", [a.name]));
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
  if(accounts.length>1)actions.push(["Remover conta",async()=>{if(await Dlg.confirm("A sessão local desta conta será apagada. Para usá-la novamente, você precisará ler o QR code.",{title:t('Remover "{0}"?',[a.name]),okLabel:"Remover",danger:true}))await accountAction(()=>invoke("remove_account",{id:a.id}));}]);
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
    const open=document.createElement("button");open.type="button";open.className="account-open";open.setAttribute("aria-label",t("Abrir {0}",[a.name]));
    const name=document.createElement("span");name.className="acct-name";name.textContent=a.name;
    const info=document.createElement("span");info.className="acct-subtitle";info.textContent=a.detached?"Em janela separada":a.is_default?"Conta ao iniciar":"WhatsApp";open.append(name,info);open.onclick=()=>accountAction(()=>invoke("open_account",{id:a.id}));
    row.append(avatar,open);
    if(a.unread){const badge=document.createElement("span");badge.className="acct-unread";badge.textContent=a.unread;badge.title=t("{0} conversas não lidas",[a.unread]);row.appendChild(badge);}
    const more=document.createElement("button");more.type="button";more.className="account-more";more.textContent="⋮";more.setAttribute("aria-label",t("Opções de {0}",[a.name]));more.setAttribute("aria-haspopup","menu");more.onclick=()=>accountMenu(a,accounts,more);row.appendChild(more);list.appendChild(row);
  }
}

async function loadAccounts() {
  try {
    const accounts = await invoke("list_accounts");
    renderAccounts(accounts);
  } catch (e) {
    document.getElementById('accounts-note').textContent = t('Não foi possível carregar as contas: {0}', [String(e)]);
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
  // The macOS rows stay hidden until load() knows the platform.
  document.getElementById("menubar_only").addEventListener("change", syncMenuBarIconRow);
  document.getElementById("hide_dock_icon").addEventListener("change", syncMenuBarIconRow);
  load();
  loadAccounts();
  loadLock();
  wireLock();
  document.getElementById("save").addEventListener("click", save);
  // The update switches apply at once, like zoom and language.
  for (const id of ["auto_update_check", "auto_update_install"]) {
    document.getElementById(id).addEventListener("change", save);
  }
  document.getElementById("proxy_mode").addEventListener("change", syncProxyRows);
  document.getElementById("proxy_restart_btn").addEventListener("click", async () => {
    try {
      await invoke("restart_app");
    } catch (e) {
      document.getElementById("note").textContent = String(e);
    }
  });
  document.getElementById("add_account").addEventListener("click", addAccount);
  document.getElementById('startup_account').addEventListener('change', event => {
    const id = event.target.value;
    accountAction(() => invoke('set_default_account', { id }));
  });
  document.getElementById("new_account_name").addEventListener("keydown", (e) => {
    if (e.key === "Enter") addAccount();
  });
  document.getElementById("record_hotkey").addEventListener("click", () => toggleRecording("hotkey", "record_hotkey"));
  document.getElementById("record_blur_hotkey").addEventListener("click", () => toggleRecording("blur_hotkey", "record_blur_hotkey"));
  document.getElementById("choose_download_dir").addEventListener("click", chooseDownloadDir);
  document.getElementById("reset_download_dir").addEventListener("click", resetDownloadDir);
  document.getElementById("choose_icon").addEventListener("click", chooseIcon);
  document.getElementById("reset_icon").addEventListener("click", resetIcon);
  // Zoom saves on change rather than on Save: picking a size you cannot see the
  // effect of is guesswork, and the webview applies it immediately.
  document.getElementById("zoom").addEventListener("change", save);
  document.getElementById("title_bar").addEventListener("change", save);
  // The zoom shortcuts change it from the WhatsApp window; catch up on return.
  window.addEventListener("focus", () => {
    invoke("get_settings").then((s) => {
      if (Number.isFinite(s.zoom)) selectNearestZoom(s.zoom);
    }).catch(() => {});
  });
  // Language applies at once, like zoom.
  document.getElementById("language").addEventListener("change", async () => {
    I18n.setLang(document.getElementById("language").value);
    await save();
  });
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
    document.getElementById("biometric_label").textContent = t("Usar {0}", [s.biometric_label]);
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
    const next = await Dlg.prompt("Pelo menos 6 caracteres.", {
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
let recordInputId = "hotkey";
let recordBtnId = "record_hotkey";
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
  const btn = document.getElementById(recordBtnId);
  btn.textContent = "Gravar";
  btn.classList.remove("recording");
  if (restore) document.getElementById(recordInputId).value = hotkeyPrev;
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
  document.getElementById(recordInputId).value = accel;
  setHotkeyHint("");
  stopRecording(false);
}

function toggleRecording(inputId, btnId) {
  if (recordingHotkey) { setHotkeyHint(""); stopRecording(true); return; }
  recordingHotkey = true;
  recordInputId = inputId;
  recordBtnId = btnId;
  hotkeyPrev = document.getElementById(inputId).value;
  const btn = document.getElementById(btnId);
  btn.textContent = "Pressione as teclas… (Esc cancela)";
  btn.classList.add("recording");
  setHotkeyHint("");
  window.addEventListener("keydown", onRecordKeydown, true);
  window.addEventListener("blur", onRecordBlur, true);
}
