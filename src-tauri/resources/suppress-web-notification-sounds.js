// Optional workaround for the duplicate WhatsApp Web notification chime.
// Identified from a real WebView2 trace; avoid guessing based on userActivation.
(() => {
  "use strict";

  if (location.origin !== "https://web.whatsapp.com") return;

  const CHIME_ORIGIN = "https://static.whatsapp.net";
  const CHIME_PATH = "/rsrc.php/yW/r/BS_BUUXbKq5.mp3";
  const media = HTMLMediaElement.prototype;
  const originalPlay = media.play;

  function isKnownChime(element) {
    if (!(element instanceof HTMLAudioElement)) return false;
    const source = element.currentSrc || element.src;
    if (!source) return false;

    try {
      const url = new URL(source, location.href);
      return url.origin === CHIME_ORIGIN && url.pathname === CHIME_PATH;
    } catch (_) {
      return false;
    }
  }

  media.play = function (...args) {
    if (isKnownChime(this)) {
      // Do not start playback, even during typing or after clicking a toast.
      // Preserve the play() Promise shape, without modifying other sounds.
      return Promise.resolve();
    }
    return Reflect.apply(originalPlay, this, args);
  };
})();
