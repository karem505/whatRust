"use strict";
// Interface language for the local pages (settings, lock screen, shell).
//
// Portuguese (Brazil) is the source language: the HTML and scripts are written
// in Portuguese and EN maps each Portuguese string to its English text. With
// English selected, text nodes and a few attributes whose (whitespace-collapsed)
// text is a key are rewritten, including nodes added later by scripts, so code
// that does `el.textContent = "Salvar"` needs no change. Strings with variable
// parts go through t("Abrir {0}", [name]). A string without an entry is shown
// as written. The start language is injected by Rust as window.__WR_LANG; the
// settings page switches it live with I18n.setLang().
(function (root) {
  var EN = {
    // titles
    "whatRust — Configurações": "whatRust — Settings",
    "whatRust — Bloqueado": "whatRust — Locked",
    // header and navigation
    "Configurações": "Settings",
    "Voltar ao WhatsApp": "Back to WhatsApp",
    "Contas": "Accounts",
    "Notificações": "Notifications",
    "Aparência": "Appearance",
    "Segurança": "Security",
    "Inicialização": "Startup",
    "Atalhos": "Shortcuts",
    "Diagnóstico": "Diagnostics",
    // accounts
    "Conta ao iniciar": "Account on startup",
    "Nome da nova conta": "New account name",
    "Adicionar": "Add",
    "Múltiplas contas exigem macOS 14 ou superior.": "Multiple accounts require macOS 14 or later.",
    "Aguarde…": "Please wait…",
    "Ações de {0}": "Actions for {0}",
    "Abrir conta": "Open account",
    "Reintegrar à janela": "Return to the main window",
    "Abrir em janela separada": "Open in a separate window",
    "Usar ao iniciar": "Use on startup",
    "Renomear": "Rename",
    "Renomear conta": "Rename account",
    "Informe um nome.": "Enter a name.",
    "Mover para cima": "Move up",
    "Mover para baixo": "Move down",
    "Remover conta": "Remove account",
    "A sessão local desta conta será apagada. Para usá-la novamente, você precisará ler o QR code.": "This account's local session will be deleted. To use it again, you will need to scan the QR code.",
    "Remover \"{0}\"?": "Remove \"{0}\"?",
    "Remover": "Remove",
    "Abrir {0}": "Open {0}",
    "Em janela separada": "In a separate window",
    "{0} conversas não lidas": "{0} unread chats",
    "Opções de {0}": "Options for {0}",
    "Não foi possível carregar as contas: {0}": "Could not load the accounts: {0}",
    // notifications
    "Receber notificações": "Receive notifications",
    "Ativa os avisos de mensagens do whatRust.": "Turns on whatRust message alerts.",
    "Avisar com o aplicativo aberto": "Notify while the app is open",
    "Receber avisos mesmo com a janela em primeiro plano.": "Get alerts even when the window is in the foreground.",
    "Avisar na conversa que estou lendo": "Notify in the chat I am reading",
    "Também avisar mensagens novas na conversa aberta.": "Also alert for new messages in the open chat.",
    "Respeitar conversas silenciadas": "Respect muted chats",
    "Quando o silenciamento não puder ser confirmado, o detector adicional permanece silencioso.": "When muting cannot be confirmed, the additional detector stays silent.",
    "O detector adicional só usa mensagens novas identificáveis. Conversas silenciadas, histórico e estados desconhecidos não geram avisos adicionais.": "The additional detector only uses identifiable new messages. Muted chats, history and unknown states do not produce additional alerts.",
    // behavior
    "Comportamento": "Behavior",
    "Continuar em segundo plano ao fechar": "Keep running in the background when closed",
    "Mantém suas contas conectadas ao fechar a janela": "Keeps your accounts connected when the window is closed",
    "Iniciar minimizado": "Start minimized",
    "Iniciar sem abrir a janela principal": "Start without opening the main window",
    "Iniciar com o sistema": "Start with the system",
    "Mostrar ícone na barra de menus": "Show icon in the menu bar",
    "Quando desativado, clique no ícone do Dock para abrir o whatRust": "When off, click the Dock icon to open whatRust",
    "Iniciar minimizado com o Windows": "Start minimized with Windows",
    "Vale apenas ao entrar no Windows. Ao abrir pelo atalho, a janela sempre aparece.": "Applies only at Windows sign-in. When opened from a shortcut, the window always appears.",
    // appearance and language
    "Tamanho da interface": "Interface size",
    "Ajusta o tamanho do WhatsApp sem alterar as outras contas.": "Adjusts the size of WhatsApp without changing the other accounts.",
    "Muito pequeno": "Very small",
    "Pequeno": "Small",
    "Padrão": "Default",
    "Grande": "Large",
    "Somente na barra de menus": "Menu bar only",
    "Sem ícone no Dock nem janela principal — todas as contas continuam conectadas para notificações e cada uma abre como painel pela bandeja": "No Dock icon or main window — every account stays connected for notifications, and each opens as its own panel from the tray",
    "O ícone da bandeja é o único ponto de entrada no modo somente barra de menus, por isso permanece visível": "The tray icon is the only entry point in menu-bar-only mode, so it stays visible",
    "Requer macOS 14 (Sonoma) ou superior — este Mac não consegue manter um painel oculto conectado": "Requires macOS 14 (Sonoma) or later — this Mac cannot keep a hidden panel connected",
    "Idioma": "Language",
    "Idioma da interface do whatRust.": "Language of the whatRust interface.",
    // shortcuts
    "Atalho para mostrar ou ocultar": "Shortcut to show or hide",
    "Uma combinação de teclas para mostrar ou ocultar o aplicativo": "A key combination to show or hide the app",
    "Atalho": "Shortcut",
    "Gravar": "Record",
    "No Linux/Wayland, configure": "On Linux/Wayland, set",
    "nos atalhos do sistema.": "in the system shortcuts.",
    "Pressione as teclas… (Esc cancela)": "Press the keys… (Esc cancels)",
    "Tecla não suportada. Tente outra.": "Unsupported key. Try another.",
    "Inclua Ctrl, Alt ou Shift.": "Include Ctrl, Alt or Shift.",
    // security
    "Senha": "Password",
    "Pelo menos 6 caracteres": "At least 6 characters",
    "Pelo menos 6 caracteres.": "At least 6 characters.",
    "Confirmar": "Confirm",
    "Ativar bloqueio": "Turn on lock",
    "Usar biometria": "Use biometrics",
    "Usar {0}": "Use {0}",
    "Bloquear ao iniciar": "Lock on launch",
    "Bloquear ao ocultar": "Lock when hidden",
    "Bloquear por inatividade": "Lock when idle",
    "Minutos; 0 desativa": "Minutes; 0 turns it off",
    "Bloquear agora": "Lock now",
    "Alterar senha…": "Change password…",
    "Desativar…": "Turn off…",
    "O bloqueio protege o acesso às janelas do whatRust. Ele": "The lock protects access to the whatRust windows. It",
    "não": "does not",
    "criptografa os dados no disco.": "encrypt the data on disk.",
    "Não foi possível ativar o bloqueio": "Could not turn on the lock",
    "Não foi possível bloquear": "Could not lock",
    "Senha atual": "Current password",
    "Continuar": "Continue",
    "Informe sua senha atual.": "Enter your current password.",
    "Nova senha": "New password",
    "Alterar": "Change",
    "Informe uma nova senha.": "Enter a new password.",
    "Sua senha de bloqueio foi alterada.": "Your lock password was changed.",
    "Senha alterada": "Password changed",
    "Não foi possível alterar a senha": "Could not change the password",
    "Informe sua senha atual para desativar o bloqueio.": "Enter your current password to turn off the lock.",
    "Desativar bloqueio": "Turn off lock",
    "Desativar": "Turn off",
    "Não foi possível desativar o bloqueio": "Could not turn off the lock",
    "Não foi possível alterar a biometria": "Could not change biometrics",
    "Não foi possível salvar": "Could not save",
    // diagnostics
    "Diagnóstico de notificações": "Notification diagnostics",
    "Verificar avisos do Windows": "Check Windows alerts",
    "O teste confirma o envio; confira se o Windows exibe o banner.": "The test confirms sending; check that Windows shows the banner.",
    "Testar": "Test",
    "Permissões e Não perturbe": "Permissions and Do Not Disturb",
    "Abrir no Windows": "Open in Windows",
    "Atividade desta sessão": "Activity this session",
    "Somente horário, conta e resultado. Não armazena o texto das mensagens.": "Time, account and result only. Message text is not stored.",
    "Aguardando envio": "Waiting to send",
    "Enviada ao Windows; exibição não confirmada": "Sent to Windows; display not confirmed",
    "Falha no envio": "Send failed",
    "Não enviada": "Not sent",
    "Aviso aberto": "Alert opened",
    "Aviso encerrado": "Alert dismissed",
    "Aplicativo bloqueado": "App locked",
    "Notificações desativadas": "Notifications off",
    "Avisos em primeiro plano desativados": "Foreground alerts off",
    "Conversa ativa": "Active chat",
    "Conversa silenciada": "Muted chat",
    "Silenciamento não confirmado": "Mute state not confirmed",
    "Windows recusou o envio": "Windows rejected the send",
    "Captura nativa ativa": "Native capture active",
    "Captura nativa indisponível; ponte alternativa": "Native capture unavailable; fallback bridge",
    "Controles integrados": "Integrated controls",
    "Acesso pelo menu da janela": "Access from the window menu",
    "Detector adicional disponível quando houver sinais confiáveis": "Additional detector available when reliable signals exist",
    "Aguardando conversa compatível": "Waiting for a compatible chat",
    "Nenhum aviso registrado": "No alerts recorded",
    "Os próximos eventos aparecerão aqui durante esta execução.": "Upcoming events will appear here during this run.",
    // footer and panel
    "Salvar": "Save",
    "Alterações salvas ✓": "Changes saved ✓",
    "Alterações salvas": "Changes saved",
    "Salvo com observação: {0}": "Saved with a note: {0}",
    "Voltar sem salvar?": "Go back without saving?",
    "As alterações ainda não foram salvas.": "Your changes have not been saved.",
    "Voltar": "Back",
    // shell
    "Controles do whatRust": "whatRust controls",
    "Configurações do whatRust": "whatRust settings",
    "Trocar conta do WhatsApp": "Switch WhatsApp account",
    "Escolha uma conta para continuar": "Choose an account to continue",
    "Use o seletor acima para abrir um WhatsApp ou as configurações para gerenciar suas contas.": "Use the selector above to open a WhatsApp, or settings to manage your accounts.",
    // lock screen
    "Digite sua senha para acessar suas contas do WhatsApp.": "Enter your password to access your WhatsApp accounts.",
    "Digite sua senha": "Enter your password",
    "Digite sua senha.": "Enter your password.",
    "Desbloquear": "Unlock",
    "Esqueceu a senha? Redefinir…": "Forgot your password? Reset…",
    "Senha incorreta. Tente novamente.": "Wrong password. Try again.",
    "Não foi possível desbloquear. Tente novamente.": "Could not unlock. Try again.",
    "Não foi possível confirmar a biometria. Digite sua senha.": "Could not confirm biometrics. Enter your password.",
    "Muitas tentativas. Tente de novo em {0}s.": "Too many attempts. Try again in {0}s.",
    "Isso desconecta todas as contas deste aplicativo e remove o bloqueio. Você precisará vincular o WhatsApp novamente pelo QR Code. Suas conversas continuam no celular.": "This signs out every account in this app and removes the lock. You will need to link WhatsApp again with the QR code. Your chats stay on your phone.",
    "Redefinir o whatRust?": "Reset whatRust?",
    "Redefinir": "Reset",
    "Não foi possível redefinir o aplicativo. Tente novamente.": "Could not reset the app. Try again.",
    // dialogs
    "Cancelar": "Cancel",
    "Este campo não pode ficar vazio.": "This field cannot be empty."
  };

  var lang = root.__WR_LANG === "en" ? "en" : "pt-BR";

  function norm(s) { return String(s).replace(/\s+/g, " ").trim(); }
  function has(k) { return Object.prototype.hasOwnProperty.call(EN, k); }
  function fill(text, args) {
    if (!args) return text;
    for (var i = 0; i < args.length; i++) text = text.split("{" + i + "}").join(String(args[i]));
    return text;
  }
  // Translate a source string (exact key) and fill {0}, {1}...
  function t(s, args) {
    var key = norm(s);
    var out = lang === "en" && has(key) ? EN[key] : s;
    return fill(out, args);
  }

  var api = {
    EN: EN,
    norm: norm,
    t: t,
    lang: function () { return lang; },
    locale: function () { return lang === "en" ? "en-US" : "pt-BR"; },
    setLang: function () {},
  };
  root.I18n = api;
  root.t = t;
  if (typeof module !== "undefined" && module.exports) module.exports = api;
  if (typeof document === "undefined" || !document.documentElement) return;

  // ---- DOM translation -------------------------------------------------
  var ATTRS = ["aria-label", "title", "placeholder"];
  var text = new WeakMap();   // text node -> { src, out }
  var attrs = new WeakMap();  // element  -> { name: { src, out } }
  var SKIP = { SCRIPT: 1, STYLE: 1, CODE: 1 };

  function translateText(node) {
    var rec = text.get(node);
    if (!rec || node.nodeValue !== rec.out) rec = { src: node.nodeValue, out: node.nodeValue };
    var key = norm(rec.src), out = rec.src;
    if (lang === "en" && has(key)) {
      var m = /^(\s*)[\s\S]*?(\s*)$/.exec(rec.src);
      out = m[1] + EN[key] + m[2];
    }
    rec.out = out;
    text.set(node, rec);
    if (node.nodeValue !== out) node.nodeValue = out;
  }

  function translateAttrs(el) {
    var recs = attrs.get(el) || {};
    ATTRS.forEach(function (name) {
      if (!el.hasAttribute || !el.hasAttribute(name)) return;
      var value = el.getAttribute(name), rec = recs[name];
      if (!rec || value !== rec.out) rec = { src: value, out: value };
      var key = norm(rec.src), out = lang === "en" && has(key) ? EN[key] : rec.src;
      rec.out = out;
      recs[name] = rec;
      if (value !== out) el.setAttribute(name, out);
    });
    attrs.set(el, recs);
  }

  function walk(node) {
    if (node.nodeType === 3) { translateText(node); return; }
    if (node.nodeType !== 1 || SKIP[node.nodeName]) return;
    translateAttrs(node);
    for (var c = node.firstChild; c; c = c.nextSibling) walk(c);
  }

  var observer;
  function translateAll() {
    if (observer) observer.disconnect();
    walk(document.documentElement);
    observe();
  }

  function observe() {
    if (!observer) {
      observer = new MutationObserver(function (records) {
        observer.disconnect();
        records.forEach(function (r) {
          if (r.type === "characterData") { translateText(r.target); }
          else if (r.type === "attributes") { translateAttrs(r.target); }
          else { r.addedNodes.forEach(function (n) { walk(n); }); }
        });
        observe();
      });
    }
    observer.observe(document.documentElement, {
      subtree: true, childList: true, characterData: true,
      attributes: true, attributeFilter: ATTRS,
    });
  }

  api.setLang = function (next) {
    lang = next === "en" ? "en" : "pt-BR";
    root.__WR_LANG = lang;
    document.documentElement.lang = lang;
    translateAll();
    root.dispatchEvent(new Event("i18n"));
  };

  function start() { document.documentElement.lang = lang; translateAll(); }
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", start);
  else start();
})(typeof window !== "undefined" ? window : globalThis);
