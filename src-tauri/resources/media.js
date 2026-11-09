// Linux only (WebKitGTK): make attaching a video work.
//
// whatRust presents itself to WhatsApp Web as Chrome on Linux, so WhatsApp runs
// its Chromium code paths on WebKitGTK, where three engine behaviours differ.
// Each was reproduced in a bare WebKitGTK 2.54 WebView, outside whatRust:
//
// 1. A `blob:` video loaded with `preload=auto` (the default) is read through a
//    buffering queue that hands the demuxer corrupted bytes: bogus MP4 atoms,
//    broken H.264 and AAC frames, sometimes "format not supported". With
//    `preload=metadata` the same blob is read cleanly. So blob media is loaded
//    with `preload=metadata`.
// 2. With `preload=metadata` the element stops at HAVE_FUTURE_DATA and never
//    fires `canplaythrough`, which WhatsApp's video loader waits for (it gives
//    up after 20 s). A blob is local data, so it can play through as soon as it
//    can play: report HAVE_ENOUGH_DATA and fire `canplaythrough` after `canplay`.
// 3. WebKit produces no frames for a muted <video> that is not visible in the
//    viewport, so drawing a detached one on a canvas gives a black image, and
//    WhatsApp retries its thumbnail 100 times and fails. WhatsApp parks the
//    element in a near-invisible box on Safari; do the same here.
// 4. WhatsApp also hands the video blob to an <img>. Chromium rejects that.
//    WebKit decodes the whole video as an animated image and keeps every frame
//    in memory: 2.4 GB for a 10 s 720p clip, over 16 GB for a 35 s screen
//    recording, which froze the machine. Refuse it like Chromium does.
//
// Hardware (VA-API) decoded frames also come out black or transparent on a
// canvas; Rust turns those decoders off for the webview (see gpu.rs).
(function () {
  "use strict";
  if (window.location.origin !== "https://web.whatsapp.com") return;
  if (window.__whatrustMedia) return;
  window.__whatrustMedia = true;
  function note(m) {
    try {
      var t = window.__TAURI__;
      if (t && t.core && typeof t.core.invoke === "function") {
        t.core.invoke("dlog", { msg: "media: " + m }).catch(function () {});
      }
    } catch (e) {}
  }
  try {
    var MP = HTMLMediaElement.prototype;
    var src = Object.getOwnPropertyDescriptor(MP, "src");
    var pre = Object.getOwnPropertyDescriptor(MP, "preload");
    var rs = Object.getOwnPropertyDescriptor(MP, "readyState");
    if (!src || !pre || !rs || !src.set || !pre.set || !rs.get) { note("unsupported engine, not installed"); return; }
    var setAttr = Element.prototype.setAttribute;
    var isBlob = function (u) { return typeof u === "string" && u.slice(0, 5) === "blob:"; };

    // -- 3. park a detached blob <video> where WebKit will render it ---------
    var PARK = "position:fixed;left:0;top:0;width:64px;height:36px;overflow:hidden;opacity:0.004;pointer-events:none;z-index:2147483646";
    function park(el) {
      if (!(el instanceof HTMLVideoElement) || el.isConnected || el.parentNode || !document.body) return;
      var box = document.createElement("div");
      setAttr.call(box, "data-whatrust", "video-park");
      setAttr.call(box, "aria-hidden", "true");
      box.style.cssText = PARK;
      box.appendChild(el);
      document.body.appendChild(box);
      var observer = null, finished = false;
      // Gone when WhatsApp disposes of the element (src cleared: `emptied`) or
      // moves it into its own tree (the box loses its only child).
      var release = function () {
        if (finished) return;
        if (box.firstChild === el && isBlob(src.get.call(el))) return;
        finished = true;
        el.removeEventListener("emptied", release, true);
        if (observer) observer.disconnect();
        if (box.firstChild === el) box.removeChild(el);
        if (box.parentNode) box.parentNode.removeChild(box);
      };
      el.addEventListener("emptied", release, true);
      if (typeof MutationObserver === "function") {
        observer = new MutationObserver(release);
        observer.observe(box, { childList: true });
      }
    }

    // -- 1 + 2. blob media: preload=metadata, and "can play through" ----------
    var armed = new WeakSet();
    var savedPreload = new WeakMap(); // the page's own preload attribute, to restore
    function arm(el) {
      if (!savedPreload.has(el)) savedPreload.set(el, el.getAttribute("preload"));
      if (pre.get.call(el) !== "none") pre.set.call(el, "metadata");
      park(el);
      if (armed.has(el)) return;
      armed.add(el);
      var synthesized = false, real = false;
      el.addEventListener("emptied", function () { synthesized = false; real = false; }, true);
      el.addEventListener("canplaythrough", function (e) {
        if (!e.isTrusted) return;
        // The engine got there itself after we already told the page: once is enough.
        if (synthesized) { e.stopImmediatePropagation(); return; }
        real = true;
      }, true);
      el.addEventListener("canplay", function (e) {
        if (!e.isTrusted) return;
        setTimeout(function () {
          if (real || synthesized || !isBlob(src.get.call(el)) || rs.get.call(el) < 3) return;
          synthesized = true;
          el.dispatchEvent(new Event("canplaythrough"));
        }, 0);
      }, true);
    }
    function disarm(el) {
      if (!savedPreload.has(el)) return;
      var was = savedPreload.get(el);
      savedPreload.delete(el);
      if (was === null) el.removeAttribute("preload"); else setAttr.call(el, "preload", was);
    }
    Object.defineProperty(MP, "src", {
      configurable: true, enumerable: src.enumerable, get: src.get,
      set: function (v) {
        try { if (isBlob(String(v))) arm(this); else disarm(this); } catch (e) {}
        return src.set.call(this, v);
      },
    });
    Object.defineProperty(MP, "preload", {
      configurable: true, enumerable: pre.enumerable, get: pre.get,
      set: function (v) {
        try {
          if (isBlob(src.get.call(this))) {
            savedPreload.set(this, String(v));
            if (String(v).toLowerCase() !== "none") v = "metadata";
          }
        } catch (e) {}
        return pre.set.call(this, v);
      },
    });
    Object.defineProperty(MP, "readyState", {
      configurable: true, enumerable: rs.enumerable,
      get: function () {
        var v = rs.get.call(this);
        try { if (v === 3 && armed.has(this) && isBlob(src.get.call(this))) return 4; } catch (e) {}
        return v;
      },
    });

    // -- 4. never decode a video through <img> -------------------------------
    var videoUrls = new Map(), MAX_URLS = 512;
    var createURL = URL.createObjectURL, revokeURL = URL.revokeObjectURL;
    URL.createObjectURL = function (blob) {
      var url = createURL.apply(this, arguments);
      try {
        if (blob && /^video\//i.test(blob.type || "")) {
          if (videoUrls.size >= MAX_URLS) videoUrls.delete(videoUrls.keys().next().value);
          videoUrls.set(url, true);
        }
      } catch (e) {}
      return url;
    };
    URL.revokeObjectURL = function (url) {
      try { videoUrls.delete(url); } catch (e) {}
      return revokeURL.apply(this, arguments);
    };
    // A tiny payload that is not an image: the <img> fires `error`, as Chromium
    // does for a video URL.
    var NOT_AN_IMAGE = "data:image/png;base64,AAAA";
    var IP = HTMLImageElement.prototype;
    var isrc = Object.getOwnPropertyDescriptor(IP, "src");
    var refused = new WeakMap();
    if (isrc && isrc.set) {
      Object.defineProperty(IP, "src", {
        configurable: true, enumerable: isrc.enumerable,
        get: function () { return refused.has(this) ? refused.get(this) : isrc.get.call(this); },
        set: function (v) {
          try {
            if (videoUrls.has(String(v))) {
              refused.set(this, String(v));
              note("refused a video blob given to <img>");
              return isrc.set.call(this, NOT_AN_IMAGE);
            }
            refused.delete(this);
          } catch (e) {}
          return isrc.set.call(this, v);
        },
      });
    }

    Element.prototype.setAttribute = function (name, value) {
      try {
        var n = String(name).toLowerCase();
        if (this instanceof HTMLMediaElement) {
          if (n === "src") { if (isBlob(String(value))) arm(this); else disarm(this); }
          else if (n === "preload" && isBlob(src.get.call(this))) {
            savedPreload.set(this, String(value));
            if (String(value).toLowerCase() !== "none") value = "metadata";
          }
        } else if (n === "src" && this instanceof HTMLImageElement) {
          if (videoUrls.has(String(value))) {
            refused.set(this, String(value));
            note("refused a video blob given to <img>");
            value = NOT_AN_IMAGE;
          } else refused.delete(this);
        }
      } catch (e) {}
      return setAttr.call(this, name, value);
    };
    note("blob video handling installed");
  } catch (e) {
    note("not installed: " + e);
  }
})();
