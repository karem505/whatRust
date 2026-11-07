"use strict";
// Settings -> Updates. The backend (update_service.rs) owns all state; this page
// renders the status it returns and pushes through "whatrust-update-status".
(function () {
  const invoke = window.__TAURI__.core.invoke;
  const $ = (id) => document.getElementById(id);
  let st = null;

  function mb(n) {
    return (n / 1048576).toFixed(1);
  }

  function show(id, on) {
    $(id).hidden = !on;
  }

  function when(ms) {
    return ms > 0 ? new Date(ms).toLocaleString(I18n.locale()) : t("Nunca");
  }

  // What the "Install updates automatically" switch does for this install type.
  function autoInstallNote(m) {
    if (m.id === "appimage" || m.id === "macos") {
      return t("Baixa, confere e substitui o aplicativo em segundo plano. A nova versão começa na próxima vez que você abrir o whatRust.");
    }
    if (m.id === "windows-nsis") {
      return t("Baixa e confere em segundo plano. O instalador roda quando você clica em Reiniciar e instalar, porque ele reinicia o whatRust.");
    }
    return t("Não se aplica a este tipo de instalação: ela precisa de senha ou é atualizada por outra ferramenta. Você instala com um clique.");
  }

  function render(s) {
    st = s;
    const m = s.method;
    $("upd_current").textContent = s.current;
    $("upd_method").textContent = m.label;
    $("upd_last").textContent = when(s.lastCheck);
    $("auto_update_install_subtitle").textContent = autoInstallNote(m);
    $("upd_check").disabled = ["checking", "downloading", "installing", "handoff"].includes(s.state);

    const v = s.latest ? s.latest.version : "";
    let title = "";
    let message = "";
    let progress = null;
    let install = false, apply = false, restart = false, page = false;
    const busy = s.state === "downloading" || s.state === "installing";
    switch (s.state) {
      case "checking": title = t("Procurando atualizações…"); break;
      case "current": title = t("Você está na versão mais recente"); break;
      case "available":
      case "error":
        if (s.newer) {
          title = t("A versão {0} está disponível", [v]);
          page = true;
          if (!m.canInstall) message = m.howTo;
          else if (!m.assetAvailable) message = t("Esta versão não tem um arquivo para o seu tipo de instalação.");
          else install = true;
        } else if (s.state === "error") {
          title = t("Não foi possível procurar atualizações");
        }
        break;
      case "downloading":
        title = t("Baixando a versão {0}…", [v]);
        progress = s.total > 0 ? Math.min(100, Math.round((s.received / s.total) * 100)) : -1;
        message = s.total > 0 ? t("{0} MB de {1} MB", [mb(s.received), mb(s.total)]) : t("{0} MB", [mb(s.received)]);
        break;
      case "installing":
        title = t("Instalando a versão {0}…", [v]);
        progress = -1;
        if (m.id === "deb") message = t("Digite sua senha de administrador quando o sistema pedir.");
        break;
      case "staged":
        title = t("A versão {0} está pronta para instalar", [v]);
        message = t("O instalador já foi baixado e conferido. O whatRust vai fechar e abrir de novo ao instalar.");
        apply = true;
        break;
      case "ready":
        title = t("A versão {0} foi instalada", [v]);
        message = t("Reinicie o whatRust para usar a nova versão.");
        restart = true;
        break;
      case "handoff":
        title = t("Instalando a versão {0}…", [v]);
        message = t("O instalador assumiu. O whatRust vai fechar e abrir de novo sozinho.");
        break;
      case "opened":
        title = t("Arquivo baixado e conferido");
        message = s.note || (m.id === "flatpak"
          ? t("O pacote foi aberto na central de programas do sistema; confirme a instalação lá. Se nada abrir, execute o comando abaixo.")
          : t("O arquivo foi aberto. Termine a instalação por lá."));
        break;
      default: break;
    }
    // A message from the backend for a type that cannot install (and the opened path).
    if (s.state === "opened" && s.opened) message += " " + s.opened;
    if (m.needsPassword && install) message = t("Pede sua senha de administrador ao instalar.");

    show("upd_panel", !!title);
    $("upd_title").textContent = title;
    $("upd_message").textContent = message;
    show("upd_message", !!message);
    const bar = $("upd_progress");
    show("upd_progress", progress !== null);
    if (progress === -1 || progress === null) bar.removeAttribute("value");
    else bar.value = progress;
    $("upd_error").textContent = s.error || "";
    show("upd_error", !!s.error && s.state !== "checking");
    show("upd_install", install && !busy);
    $("upd_install").textContent = m.id === "flatpak" ? t("Baixar e abrir") : t("Baixar e instalar");
    show("upd_apply", apply);
    show("upd_restart", restart);
    show("upd_page", page);
    const fp = m.id === "flatpak" && (s.newer || s.state === "opened");
    show("upd_flatpak", fp);
    $("upd_command").textContent = fp ? m.flatpakCommand : "";

    const notes = s.latest && s.latest.notes ? s.latest.notes : "";
    show("upd_notes_box", !!notes);
    if (notes !== render.notes) {
      render.notes = notes;
      ReleaseNotes.render(document, $("upd_notes"), notes);
    }
  }

  async function refresh() {
    try {
      render(await invoke("get_update_status"));
    } catch (e) {
      $("upd_error").textContent = String(e);
      show("upd_error", true);
      show("upd_panel", true);
    }
  }

  // Clipboard: the async API when the webview allows it, else a selection copy.
  async function copy(text) {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch (e) {
      const ta = document.createElement("textarea");
      ta.value = text;
      ta.style.position = "fixed";
      ta.style.opacity = "0";
      document.body.appendChild(ta);
      ta.select();
      let ok = false;
      try { ok = document.execCommand("copy"); } catch (err) { ok = false; }
      ta.remove();
      return ok;
    }
  }

  async function act(cmd) {
    try {
      await invoke(cmd);
    } catch (e) {
      $("upd_error").textContent = String(e);
      show("upd_error", true);
    }
    // the backend also pushes events; this just covers the instant before the first one
    setTimeout(refresh, 150);
  }

  window.addEventListener("DOMContentLoaded", () => {
    $("upd_check").addEventListener("click", () => act("check_for_updates"));
    $("upd_install").addEventListener("click", () => act("install_update"));
    $("upd_apply").addEventListener("click", () => act("install_update"));
    $("upd_restart").addEventListener("click", () => act("restart_to_update"));
    $("upd_page").addEventListener("click", () => act("open_release_page"));
    $("upd_copy").addEventListener("click", async () => {
      const ok = await copy($("upd_command").textContent);
      $("upd_copy").textContent = ok ? t("Copiado ✓") : t("Selecione e copie");
      setTimeout(() => ($("upd_copy").textContent = t("Copiar")), 2000);
    });
    const ev = window.__TAURI__ && window.__TAURI__.event;
    if (ev) ev.listen("whatrust-update-status", (e) => render(e.payload)).catch(() => {});
    window.addEventListener("focus", refresh);
    window.addEventListener("i18n", () => st && render(st));
    refresh();
  });
})();
