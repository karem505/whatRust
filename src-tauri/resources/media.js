// Linux only (WebKitGTK): make attaching a video work, and keep WhatsApp's
// sounds to one audio stream (sections 5 and 6).
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

  // -- 5. one sound stream: give back the stream of an <audio> not playing --
  // WebKitGTK keeps one PulseAudio/PipeWire stream open for every <audio> that
  // was played and then paused, for as long as the page holds the element.
  // WhatsApp keeps its UI sounds and every voice message it played around
  // paused, so the system volume mixer filled with «whatrust» entries
  // (reproduced in a bare WebKitGTK 2.54 WebView: 6 sounds played and paused =
  // 5 open streams; one that ends by itself frees its own). Shortly after an
  // element stops, drop its media (that closes the stream) and put the source
  // back, at the same position, on the next play(). So a stream only exists
  // while something is actually sounding.
  // Voice messages are `blob:` URLs that the page may revoke while the element
  // is parked: keep the Blob itself and reload it from a fresh URL of ours.
  // Never srcObject (calls), never <video>. The page keeps seeing its own src,
  // position and duration, and the reload's own events never reach it.
  // (A shared audio mixer, WEBKIT_GST_ENABLE_AUDIO_MIXER, also gives one stream
  // but crashed the web process when a voice note played. Routing the elements
  // through one AudioContext does not help either: WebKit still opens a stream
  // per element.)
  try {
    var AP = HTMLMediaElement.prototype;
    var asrc = Object.getOwnPropertyDescriptor(AP, "src");
    var nativePause = AP.pause, nativePlay = AP.play, nativeLoad = AP.load;
    var removeAttr = Element.prototype.removeAttribute;
    if (typeof HTMLAudioElement !== "function" || !asrc || !asrc.get || !asrc.set || !nativePause || !nativePlay || !nativeLoad) {
      note("idle audio release: unsupported engine");
    } else {
      var IDLE_MS = window.__whatrustIdleAudioMs || 1500;
      var parked = new WeakMap(); // element -> { src, time, duration, blob }
      var alias = new WeakMap();  // element playing from our own URL -> { pageSrc, own, blob }
      var timers = new WeakMap();
      var quiet = new WeakMap();  // element -> { until, types } of our own reload
      var hooked = new WeakSet();
      var audioBlobs = new Map(), MAX_BLOBS = 256; // page blob: URL -> Blob
      var createURL5 = URL.createObjectURL, revokeURL5 = URL.revokeObjectURL;
      var released = 0;
      URL.createObjectURL = function (blob) {
        var url = createURL5.apply(this, arguments);
        try {
          if (blob && /^audio\//i.test(blob.type || "")) {
            if (audioBlobs.size >= MAX_BLOBS) audioBlobs.delete(audioBlobs.keys().next().value);
            audioBlobs.set(url, blob);
          }
        } catch (e) {}
        return url;
      };
      URL.revokeObjectURL = function (url) {
        try { audioBlobs.delete(url); } catch (e) {}
        return revokeURL5.apply(this, arguments);
      };
      // Events of our own unload/reload. The page asked for neither.
      var ON_RELEASE = ["emptied", "abort", "loadstart", "error", "suspend", "stalled", "durationchange", "timeupdate"];
      var ON_RESTORE = ["emptied", "abort", "loadstart", "suspend", "stalled", "durationchange", "loadedmetadata", "loadeddata", "canplay", "canplaythrough", "seeking", "seeked"];
      var ALL = ON_RELEASE.concat(ON_RESTORE).filter(function (t, i, a) { return a.indexOf(t) === i; });
      var hook = function (el) {
        if (hooked.has(el)) return;
        hooked.add(el);
        // At the target, capture listeners run before the page's own ones.
        ALL.forEach(function (type) {
          el.addEventListener(type, function (e) {
            var q = quiet.get(el);
            if (q && q.until > Date.now() && q.types.indexOf(e.type) >= 0) e.stopImmediatePropagation();
          }, true);
        });
        // A sound that plays to its end (and is kept for later) is released too.
        el.addEventListener("ended", function () { schedule(el); });
      };
      var hush = function (el, types, ms) { quiet.set(el, { until: Date.now() + ms, types: types }); };
      var pageSrc = function (el) { var a = alias.get(el); return a ? a.pageSrc : asrc.get.call(el); };
      var blobOf = function (el) { var a = alias.get(el); return a ? a.blob : audioBlobs.get(asrc.get.call(el)) || null; };
      var releasable = function (el) {
        if (!(el instanceof HTMLAudioElement) || !el.paused || el.srcObject) return false;
        var u = pageSrc(el);
        if (typeof u !== "string") return false;
        if (/^(https?|data):/i.test(u)) return true;
        return u.slice(0, 5) === "blob:" && !!blobOf(el);
      };
      var cancel = function (el) {
        var t = timers.get(el);
        if (t) { clearTimeout(t); timers.delete(el); }
      };
      var unalias = function (el) {
        var a = alias.get(el);
        if (!a) return;
        alias.delete(el);
        try { revokeURL5.call(URL, a.own); } catch (e) {}
      };
      var release = function (el) {
        timers.delete(el);
        if (parked.has(el) || !releasable(el)) return;
        parked.set(el, { src: pageSrc(el), time: ct.get.call(el) || 0, duration: dur ? dur.get.call(el) : NaN, blob: blobOf(el) });
        unalias(el);
        hook(el);
        hush(el, ON_RELEASE, 2000);
        removeAttr.call(el, "src");
        nativeLoad.call(el);
        released++;
        if (released === 1 || released % 50 === 0) note("released idle audio streams: " + released);
      };
      var schedule = function (el) {
        if (!(el instanceof HTMLAudioElement) || parked.has(el)) return;
        cancel(el);
        timers.set(el, setTimeout(function () { release(el); }, IDLE_MS));
      };
      var restore = function (el) {
        var p = parked.get(el);
        if (!p) return false;
        parked.delete(el);
        var target = p.src;
        if (p.blob && !audioBlobs.has(p.src)) {
          // The page revoked its URL while the element was parked.
          target = createURL5.call(URL, p.blob);
          alias.set(el, { pageSrc: p.src, own: target, blob: p.blob });
        }
        asrc.set.call(el, target);
        if (p.time > 0) { try { ct.set.call(el, p.time); } catch (e) {} }
        return true;
      };
      var forget = function (el) {
        cancel(el);
        parked.delete(el);
        quiet.delete(el);
        unalias(el);
      };
      var ct = Object.getOwnPropertyDescriptor(AP, "currentTime");
      var dur = Object.getOwnPropertyDescriptor(AP, "duration");
      if (!ct || !ct.get || !ct.set) throw new Error("no currentTime");
      AP.pause = function () {
        var r = nativePause.apply(this, arguments);
        try { schedule(this); } catch (e) {}
        return r;
      };
      AP.play = function () {
        try {
          if (this instanceof HTMLAudioElement) {
            hook(this);
            cancel(this);
            if (restore(this)) hush(this, ON_RESTORE, 1500);
          }
        } catch (e) {}
        return nativePlay.apply(this, arguments);
      };
      AP.load = function () {
        try { cancel(this); restore(this); quiet.delete(this); } catch (e) {}
        return nativeLoad.apply(this, arguments);
      };
      Object.defineProperty(AP, "src", {
        configurable: true, enumerable: asrc.enumerable,
        get: function () { var p = parked.get(this); return p ? p.src : pageSrc(this); },
        set: function (v) {
          try { forget(this); } catch (e) {}
          return asrc.set.call(this, v);
        },
      });
      Object.defineProperty(AP, "currentSrc", (function () {
        var cs = Object.getOwnPropertyDescriptor(AP, "currentSrc");
        return {
          configurable: true, enumerable: cs ? cs.enumerable : true,
          get: function () { var p = parked.get(this); if (p) return p.src; var a = alias.get(this); return a ? a.pageSrc : cs && cs.get ? cs.get.call(this) : ""; },
        };
      })());
      // While parked the element has no media: keep the page's position writes
      // (WhatsApp rewinds a sound to 0 before replaying it) for restore().
      Object.defineProperty(AP, "currentTime", {
        configurable: true, enumerable: ct.enumerable,
        get: function () { var p = parked.get(this); return p ? p.time : ct.get.call(this); },
        set: function (v) {
          var p = parked.get(this);
          if (p) { var n = Number(v); p.time = isFinite(n) && n > 0 ? n : 0; return; }
          return ct.set.call(this, v);
        },
      });
      // A parked voice message still shows its length and counts as loaded.
      if (dur && dur.get) {
        Object.defineProperty(AP, "duration", {
          configurable: true, enumerable: dur.enumerable,
          get: function () { var p = parked.get(this); return p && isFinite(p.duration) ? p.duration : dur.get.call(this); },
        });
      }
      var rs5 = Object.getOwnPropertyDescriptor(AP, "readyState");
      if (rs5 && rs5.get) {
        Object.defineProperty(AP, "readyState", {
          configurable: true, enumerable: rs5.enumerable,
          get: function () { return parked.has(this) ? 4 : rs5.get.call(this); },
        });
      }
      var setAttr5 = Element.prototype.setAttribute;
      Element.prototype.setAttribute = function (name, value) {
        try {
          if (this instanceof HTMLMediaElement && String(name).toLowerCase() === "src") forget(this);
        } catch (e) {}
        return setAttr5.call(this, name, value);
      };
      note("idle audio release installed");
    }
  } catch (e) {
    note("idle audio release not installed: " + e);
  }

  // -- 6. one AudioContext ------------------------------------------------
  // Every live AudioContext is one more open stream in WebKitGTK, silent or
  // not (bare WebKitGTK 2.54: 3 contexts = 3 «playing» streams until close()).
  // WhatsApp makes several (call tones, voice recording, ...). Hand every
  // `new AudioContext()` that does not ask for its own sample rate the same
  // engine context, behind a handle that keeps its own close/suspend state.
  // The engine context closes when the last handle does.
  try {
    var NativeAC = window.AudioContext || window.webkitAudioContext;
    if (typeof NativeAC !== "function" || typeof Proxy !== "function") {
      note("shared audio context: unsupported engine");
    } else {
      var shared = null, handles = new Set(), realOf = new WeakMap(), contexts = 0;
      var unwrap = function (c) { return (c && realOf.get(c)) || c; };
      var settle = function () {
        if (!shared) return Promise.resolve();
        var open = 0, running = 0;
        handles.forEach(function (h) { if (!h.closed) { open++; if (!h.suspended) running++; } });
        var real = shared;
        if (open === 0) { shared = null; return real.close(); }
        if (running === 0) return real.suspend();
        return real.state === "suspended" ? real.resume() : Promise.resolve();
      };
      var makeHandle = function (real) {
        var h = { closed: false, suspended: false };
        var bound = new Map();
        var proxy = new Proxy(real, {
          get: function (t, prop) {
            if (prop === "state") return h.closed ? "closed" : h.suspended ? "suspended" : t.state;
            if (prop === "close") return function () {
              if (h.closed) return Promise.resolve();
              h.closed = true; handles.delete(h);
              return settle().then(function () {});
            };
            if (prop === "suspend") return function () { if (h.closed) return Promise.reject(new DOMException("closed", "InvalidStateError")); h.suspended = true; return settle().then(function () {}); };
            if (prop === "resume") return function () { if (h.closed) return Promise.reject(new DOMException("closed", "InvalidStateError")); h.suspended = false; return settle().then(function () {}); };
            var v = Reflect.get(t, prop);
            if (typeof v === "function") {
              if (!bound.has(prop)) bound.set(prop, v.bind(t));
              return bound.get(prop);
            }
            return v;
          },
          set: function (t, prop, value) { return Reflect.set(t, prop, value); },
        });
        handles.add(h);
        realOf.set(proxy, real);
        return proxy;
      };
      var SharedAC = function AudioContext(options) {
        if (!new.target) throw new TypeError("Constructor AudioContext requires 'new'");
        var own = options && options.sampleRate != null;
        if (own || new.target !== SharedAC) return Reflect.construct(NativeAC, arguments, new.target === SharedAC ? NativeAC : new.target);
        if (!shared || shared.state === "closed") {
          shared = new NativeAC(options);
          contexts++;
          if (contexts === 1 || contexts % 20 === 0) note("shared audio context opened: " + contexts);
        }
        return makeHandle(shared);
      };
      SharedAC.prototype = NativeAC.prototype;
      Object.setPrototypeOf(SharedAC, NativeAC);
      window.AudioContext = SharedAC;
      if (window.webkitAudioContext) window.webkitAudioContext = SharedAC;
      // Node constructors (new GainNode(ctx), ...) need the engine context.
      ["AnalyserNode", "AudioBufferSourceNode", "AudioWorkletNode", "BiquadFilterNode", "ChannelMergerNode", "ChannelSplitterNode",
        "ConstantSourceNode", "ConvolverNode", "DelayNode", "DynamicsCompressorNode", "GainNode", "IIRFilterNode",
        "MediaElementAudioSourceNode", "MediaStreamAudioDestinationNode", "MediaStreamAudioSourceNode", "OscillatorNode",
        "PannerNode", "StereoPannerNode", "WaveShaperNode"].forEach(function (name) {
        var Orig = window[name];
        if (typeof Orig !== "function") return;
        var Wrapped = function () {
          var args = Array.prototype.slice.call(arguments);
          args[0] = unwrap(args[0]);
          return Reflect.construct(Orig, args, !new.target || new.target === Wrapped ? Orig : new.target);
        };
        Wrapped.prototype = Orig.prototype;
        Object.setPrototypeOf(Wrapped, Orig);
        try { Object.defineProperty(Wrapped, "name", { value: name }); } catch (e) {}
        window[name] = Wrapped;
      });
      note("shared audio context installed");
    }
  } catch (e) {
    note("shared audio context not installed: " + e);
  }
})();
