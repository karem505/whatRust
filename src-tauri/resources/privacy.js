(function () {
  "use strict";
  if (window.location.origin !== "https://web.whatsapp.com") return;
  if (window.__whatrustPrivacy) return;
  var KEY = "whatrust.privacy.v2", STYLE_ID = "__whatrust_privacy_style";
  // Interface language, injected by Rust (window.__WR_LANG) before page load.
  function L(pt, en) { return window.__WR_LANG === "en" ? en : pt; }

  // Selectors are best-effort against WhatsApp Web's current DOM; adjust here.
  var GROUPS = [
    { id: "chat", label: function () { return L("Todas as mensagens da conversa", "All messages in chat"); }, amt: true, sel: ["#main [data-id]"] },
    { id: "preview", label: function () { return L("Prévia das últimas mensagens", "Last message previews"); }, amt: true, sel: ["#pane-side [data-testid='cell-frame-secondary']", "#pane-side [role='listitem'] div[role='gridcell']:nth-child(2) > div:nth-child(2)"] },
    { id: "media", label: function () { return L("Prévia de mídia", "Media previews"); }, amt: true, sel: ["#main [data-id] img", "#main [data-id] video", "#main [data-id] canvas"] },
    { id: "gallery", label: function () { return L("Galeria de mídia", "Media gallery"); }, amt: true, sel: ["[data-animate-media-viewer] img", "[data-animate-media-viewer] video", "[role='dialog'] img", "[role='dialog'] video"] },
    { id: "input", label: function () { return L("Campo de texto", "Text input"); }, amt: false, sel: ["#main footer [contenteditable]"] },
    { id: "avatars", label: function () { return L("Fotos de perfil", "Profile pictures"); }, amt: true, sel: ["#pane-side img", "#main header img", "#side header img"] },
    { id: "names", label: function () { return L("Nomes de grupos e contatos", "Group and contact names"); }, amt: true, sel: ["#pane-side [data-testid='cell-frame-title']", "#main header span[dir='auto']", "#main [data-id] [data-testid='author']"] }
  ];
  var DEFAULTS = {
    // Off until the user turns it on: an update must never blur anyone's chats.
    on: false, noTransition: false, unblurHover: false,
    idle: { on: false, amt: 12, secs: 60 },
    g: {
      chat: { on: false, amt: 8 }, preview: { on: true, amt: 8 }, media: { on: true, amt: 12 },
      gallery: { on: true, amt: 12 }, input: { on: true, amt: 6 },
      avatars: { on: false, amt: 8 }, names: { on: false, amt: 8 }
    }
  };
  var cfg = load(), idleTimer = 0, styleEl, panel, panelRoot, syncs = [];

  function clone(o) { return JSON.parse(JSON.stringify(o)); }
  function merge(base, over) {
    if (!over || typeof over !== "object") return base;
    Object.keys(base).forEach(function (k) {
      if (base[k] && typeof base[k] === "object") base[k] = merge(base[k], over[k]);
      else if (typeof over[k] === typeof base[k]) base[k] = over[k];
    });
    return base;
  }
  function load() { try { return merge(clone(DEFAULTS), JSON.parse(localStorage.getItem(KEY))); } catch (e) { return clone(DEFAULTS); } }
  function save() { try { localStorage.setItem(KEY, JSON.stringify(cfg)); } catch (e) {} }
  function num(v, lo, hi) { v = +v; return isFinite(v) ? Math.min(hi, Math.max(lo, v)) : lo; }

  function css() {
    var R = "html[data-wr-privacy='on']", out = [], all = [];
    GROUPS.forEach(function (g) {
      var c = cfg.g[g.id]; if (!c.on) return;
      var sel = g.sel.map(function (s) { return R + " " + s; });
      all = all.concat(sel);
      out.push(sel.join(",") + "{filter:blur(" + (g.amt ? num(c.amt, 1, 40) : 6) + "px)!important;" +
        (cfg.noTransition ? "" : "transition:filter .15s ease;") + "}");
      out.push(sel.map(function (s) { return s + ":hover"; }).join(",") + "{filter:none!important}");
    });
    if (cfg.unblurHover && all.length)
      out.push(all.map(function (s) { return "html:hover:not([data-wr-idle='on']) " + s.slice(R.length + 1); }).join(",") + "{filter:none!important}");
    if (cfg.idle.on)
      out.push("html[data-wr-idle='on'] #app{filter:blur(" + num(cfg.idle.amt, 1, 40) + "px)!important}");
    return out.join("\n");
  }
  function apply() {
    var root = document.documentElement;
    if (!styleEl) { styleEl = document.createElement("style"); styleEl.id = STYLE_ID; (document.head || root).appendChild(styleEl); }
    styleEl.textContent = css();
    root.setAttribute("data-wr-privacy", cfg.on ? "on" : "off");
    armIdle(); save();
  }

  // Idle blur: any input resets the timer; after `secs` without input, blur #app.
  function armIdle() {
    clearTimeout(idleTimer);
    document.documentElement.removeAttribute("data-wr-idle");
    if (cfg.on && cfg.idle.on) idleTimer = setTimeout(function () {
      document.documentElement.setAttribute("data-wr-idle", "on");
    }, num(cfg.idle.secs, 5, 3600) * 1000);
  }
  ["mousemove", "mousedown", "keydown", "wheel", "touchstart"].forEach(function (ev) {
    window.addEventListener(ev, function () { if (cfg.on && cfg.idle.on) armIdle(); }, true);
  });
  document.addEventListener("visibilitychange", function () {
    if (document.hidden && cfg.on && cfg.idle.on) document.documentElement.setAttribute("data-wr-idle", "on");
  });

  // ---- Panel UI (Shadow DOM so WhatsApp's CSS cannot touch it) ----
  var PANEL_CSS =
    ":host{all:initial;font:400 13px/1.4 ui-rounded,\"SF Pro Rounded\",-apple-system,BlinkMacSystemFont,\"SF Pro Text\",sans-serif;-webkit-font-smoothing:antialiased;letter-spacing:.005em;color:#e9edefd9}" +
    ".panel{position:fixed;right:10px;bottom:10px;z-index:2147483647;width:300px;max-height:calc(100vh - 80px);overflow:auto;background:#232626;border:1px solid #8883;border-radius:10px;box-shadow:0 4px 20px #0008;padding:6px 0}.panel[hidden]{display:none}" +
    "h2{font-size:11px;margin:8px 14px 2px;font-weight:500;letter-spacing:.04em;text-transform:uppercase;opacity:.6}" +
    ".row{display:flex;align-items:center;gap:8px;padding:8px 14px;border-top:1px solid #8881}.row span.l{flex:1}" +
    ".gear{width:24px;height:24px;border-radius:50%;border:1px solid #21c063;background:transparent;color:#21c063;cursor:pointer;font-size:13px;line-height:1}" +
    ".sw{position:relative;width:44px;height:24px;border-radius:12px;background:#555;border:0;cursor:pointer;flex:none}.sw::after{content:'';position:absolute;top:3px;left:3px;width:18px;height:18px;border-radius:50%;background:#fff;transition:left .12s}.sw[aria-checked=true]{background:#1f9d3f}.sw[aria-checked=true]::after{left:23px}" +
    ".amt{display:flex;align-items:center;gap:8px;padding:2px 14px 10px;font-size:12px;opacity:.9}.amt input{flex:1;accent-color:#21c063}.amt[hidden]{display:none}";

  function el(tag, props, kids) {
    var e = document.createElement(tag);
    Object.keys(props || {}).forEach(function (k) { if (k === "text") e.textContent = props[k]; else e.setAttribute(k, props[k]); });
    (kids || []).forEach(function (k) { e.appendChild(k); });
    return e;
  }
  function sw(get, set) {
    var b = el("button", { "class": "sw", role: "switch", type: "button" });
    function sync() { b.setAttribute("aria-checked", String(!!get())); }
    syncs.push(sync);
    b.addEventListener("click", function () { set(!get()); sync(); apply(); });
    sync(); return b;
  }
  function slider(obj, key, lo, hi, unit) {
    var out = el("span", { text: obj[key] + unit });
    var r = el("input", { type: "range", min: lo, max: hi, step: 1 }); r.value = obj[key];
    r.addEventListener("input", function () { obj[key] = +r.value; out.textContent = r.value + unit; apply(); });
    return [r, out];
  }
  function row(label, toggle, withGear, detail) {
    var kids = [el("span", { "class": "l", text: label })];
    var d = detail && el("div", { "class": "amt", hidden: "" }, detail);
    if (withGear) {
      var g = el("button", { "class": "gear", type: "button", title: L("Intensidade do desfoque", "Blur amount"), "aria-label": L("Intensidade do desfoque: ", "Blur amount: ") + label, text: "⚙" });
      g.addEventListener("click", function () { d.hidden = !d.hidden; }); kids.push(g);
    }
    kids.push(toggle);
    var r = el("div", { "class": "row" }, kids);
    return d ? [r, d] : [r];
  }
  function build() {
    panelRoot = el("div"); var root = panelRoot.attachShadow({ mode: "open" });
    panel = el("div", { "class": "panel", hidden: "", role: "dialog", "aria-label": L("Desfoque de privacidade", "Privacy blur") });
    function add(parts) { parts.forEach(function (p) { panel.appendChild(p); }); }
    add(row(L("Desfoque de privacidade", "Privacy blur"), sw(function () { return cfg.on; }, function (v) { cfg.on = v; })));
    panel.appendChild(el("h2", { text: L("O que desfocar", "What to blur") }));
    GROUPS.forEach(function (g) {
      var c = cfg.g[g.id];
      add(row(g.label(), sw(function () { return c.on; }, function (v) { c.on = v; }), g.amt,
        g.amt ? [el("span", { text: L("Desfoque", "Blur") })].concat(slider(c, "amt", 1, 30, "px")) : null));
    });
    add(row(L("Sem animação", "No transition delay"), sw(function () { return cfg.noTransition; }, function (v) { cfg.noTransition = v; })));
    add(row(L("Mostrar tudo com o mouse sobre a janela", "Unblur all on app hover"), sw(function () { return cfg.unblurHover; }, function (v) { cfg.unblurHover = v; })));
    add(row(L("Desfocar tudo quando ocioso", "Blur everything when idle"), sw(function () { return cfg.idle.on; }, function (v) { cfg.idle.on = v; }), true,
      [el("span", { text: L("Desfoque", "Blur") })].concat(slider(cfg.idle, "amt", 1, 30, "px")).concat([el("span", { text: L("após", "after") })]).concat(slider(cfg.idle, "secs", 5, 600, "s"))));
    // No permanent floating button: it covered WhatsApp's send button. The
    // panel opens with Cmd/Ctrl+Shift+P (see Settings -> Shortcuts).
    root.append(el("style", { text: PANEL_CSS }), panel);
    document.documentElement.appendChild(panelRoot);
  }
  function togglePanel() { if (!panel) build(); panel.hidden = !panel.hidden; }

  window.__whatrustPrivacy = {
    toggle: function () { cfg.on = !cfg.on; apply(); syncs.forEach(function (f) { f(); }); },
    panel: togglePanel
  };
  window.addEventListener("keydown", function (e) {
    if (e.key === "Escape" && panel && !panel.hidden) { panel.hidden = true; return; }
    if (!(e.metaKey || e.ctrlKey) || !e.shiftKey || e.altKey) return;
    if (e.code === "KeyP") { e.preventDefault(); e.stopPropagation(); togglePanel(); }
  }, true);
  function init() { build(); apply(); }
  if (document.documentElement) { if (document.body) init(); else document.addEventListener("DOMContentLoaded", init, { once: true }); }
})();
