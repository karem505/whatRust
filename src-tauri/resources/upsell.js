// whatRust: hide WhatsApp's "get the desktop app" upsells (issue #83).
// WhatsApp shows them only when it believes the browser runs on Windows or
// macOS (WAWebDesktopUpsellUtils.getUserDesktopOs) or Android. Answering "no
// such OS" makes it behave as it does on Linux, where none are shown. The
// module system's define function (window.__d) is wrapped before WhatsApp's
// scripts run; if WhatsApp renames the module the patch simply does nothing.
(function () {
  "use strict";
  if (window.__wrUpsellHook) return;
  window.__wrUpsellHook = true;
  var MODULE = "WAWebDesktopUpsellUtils";
  function neuter(exports) {
    if (!exports || typeof exports.getUserDesktopOs !== "function") return false;
    exports.getUserDesktopOs = function () { return null; };
    if (typeof exports.isAndroidAppUpsellEnabled === "function") {
      exports.isAndroidAppUpsellEnabled = function () { return false; };
    }
    window.__wrUpsellPatched = true;
    return true;
  }
  function wrapDefine(define) {
    if (typeof define !== "function" || define.__wrWrapped) return define;
    var wrapped = function (name, deps, factory) {
      if (name === MODULE && typeof factory === "function") {
        var original = factory;
        arguments[2] = function () {
          var result = original.apply(this, arguments);
          try {
            for (var i = 0; i < arguments.length; i++) {
              if (neuter(arguments[i])) break;
            }
          } catch (e) { /* leave WhatsApp untouched */ }
          return result;
        };
      }
      return define.apply(this, arguments);
    };
    wrapped.__wrWrapped = true;
    return wrapped;
  }
  var current = window.__d;
  try {
    Object.defineProperty(window, "__d", {
      configurable: true,
      enumerable: true,
      get: function () { return current; },
      set: function (v) { current = wrapDefine(v); },
    });
    if (current) current = wrapDefine(current);
  } catch (e) { /* not patchable: leave it */ }
})();
