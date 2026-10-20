// In-page alert / confirm / prompt.
//
// The native ones cannot be used here. WKWebView only shows a JavaScript dialog
// if the host implements the matching WKUIDelegate panel method, and wry
// implements none of them, so on macOS `alert()` does nothing, `confirm()`
// returns false, and `prompt()` returns null — all without ever drawing
// anything. Rename, Remove, Change password, Disable lock and the reset link
// therefore looked like dead buttons on macOS while working on Windows and
// Linux, whose webviews answer these calls natively.
//
// These replacements are page elements, so they behave the same everywhere.
// They are async: `await Dlg.confirm(...)`, not `if (confirm(...))`.
(function () {
  "use strict";

  var open = null; // { resolve, cancel } for the dialog currently on screen

  function el(tag, className, text) {
    var e = document.createElement(tag);
    if (className) e.className = className;
    if (text !== undefined) e.textContent = text;
    return e;
  }

  function close(result) {
    if (!open) return;
    var o = open;
    open = null;
    document.removeEventListener("keydown", onKeydown, true);
    o.backdrop.remove();
    if (o.restoreFocus && document.contains(o.restoreFocus)) o.restoreFocus.focus();
    o.resolve(result);
  }

  function onKeydown(e) {
    if (!open) return;
    if (e.key === "Escape") {
      e.preventDefault();
      close(open.cancelValue);
    } else if (e.key === "Enter" && e.target.tagName !== "BUTTON") {
      e.preventDefault();
      open.accept();
    } else if (e.key === "Tab") {
      // Keep focus inside the dialog: it is modal in intent, and a Tab that
      // escapes it lands on controls the dialog is covering.
      var f = open.focusable;
      if (f.length === 1) { e.preventDefault(); f[0].focus(); return; }
      if (f.length < 2) return;
      var first = f[0];
      var last = f[f.length - 1];
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    }
  }

  // kind: "alert" | "confirm" | "prompt"
  function show(kind, message, opts) {
    opts = opts || {};
    if (open) close(open.cancelValue); // one at a time; the newer call wins

    var backdrop = el("div", "dlg-backdrop");
    var box = el("div", "dlg");
    box.setAttribute("role", "dialog");
    box.setAttribute("aria-modal", "true");

    if (opts.title) {
      var h = el("h2", null, opts.title);
      box.appendChild(h);
    }
    if (message) box.appendChild(el("p", "dlg-message", message));

    var input = null;
    if (kind === "prompt") {
      input = el("input", "dlg-input");
      input.type = opts.password ? "password" : "text";
      input.value = opts.value || "";
      if (opts.password) input.autocomplete = "off";
      if (opts.placeholder) input.placeholder = opts.placeholder;
      box.appendChild(input);
    }

    var error = el("p", "dlg-error");
    error.hidden = true;
    box.appendChild(error);

    var actions = el("div", "dlg-actions");
    var cancel = null;
    if (kind !== "alert") {
      cancel = el("button", "dlg-secondary", opts.cancelLabel || "Cancelar");
      cancel.type = "button";
      actions.appendChild(cancel);
    }
    var ok = el("button", opts.danger ? "dlg-danger" : null, opts.okLabel || "OK");
    ok.type = "button";
    actions.appendChild(ok);
    box.appendChild(actions);
    backdrop.appendChild(box);

    var cancelValue = kind === "confirm" ? false : kind === "prompt" ? null : undefined;

    return new Promise(function (resolve) {
      function accept() {
        if (kind !== "prompt") return close(kind === "confirm" ? true : undefined);
        // Never trim a password: leading or trailing whitespace is part of it.
        var v = opts.password ? input.value : input.value.trim();
        if (opts.required !== false && !v) {
          error.textContent = opts.requiredMessage || "Este campo não pode ficar vazio.";
          error.hidden = false;
          input.focus();
          return;
        }
        close(v);
      }

      open = {
        resolve: resolve,
        backdrop: backdrop,
        accept: accept,
        cancelValue: cancelValue,
        restoreFocus: document.activeElement,
        focusable: cancel ? [input, cancel, ok].filter(Boolean) : [ok],
      };

      ok.addEventListener("click", accept);
      if (cancel) cancel.addEventListener("click", function () { close(cancelValue); });
      backdrop.addEventListener("mousedown", function (e) {
        if (e.target === backdrop) close(cancelValue);
      });
      document.addEventListener("keydown", onKeydown, true);

      document.body.appendChild(backdrop);
      (input || ok).focus();
      if (input) input.select();
    });
  }

  window.Dlg = {
    /** Resolves when dismissed. */
    alert: function (message, opts) { return show("alert", message, opts); },
    /** Resolves true only if confirmed. */
    confirm: function (message, opts) { return show("confirm", message, opts); },
    /** Resolves to the entered text (trimmed unless `password`), or null if cancelled. */
    prompt: function (message, opts) { return show("prompt", message, opts); },
  };
})();
