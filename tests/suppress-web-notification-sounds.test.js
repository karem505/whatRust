"use strict";
const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const script = fs.readFileSync(path.join(__dirname, "../src-tauri/resources/suppress-web-notification-sounds.js"), "utf8");
const CHIME = "https://static.whatsapp.net/rsrc.php/yW/r/BS_BUUXbKq5.mp3";

function fixture(origin = "https://web.whatsapp.com") {
  class Media {
    constructor(src, muted = false) {
      this.src = src;
      this.currentSrc = src;
      this.muted = muted;
      this.plays = 0;
    }
    play() {
      this.plays++;
      return Promise.resolve("played");
    }
  }
  class Audio extends Media {}
  class Video extends Media {}
  const navigator = {userActivation: {isActive: true}};
  const ctx = vm.createContext({
    location: {origin, href: origin + "/"}, HTMLMediaElement: Media,
    HTMLAudioElement: Audio, URL, Promise, Reflect, navigator,
  });
  const originalPlay = Media.prototype.play;
  vm.runInContext(script, ctx);
  return {Audio, Video, navigator, Media, originalPlay};
}

test("blocks the identified chime on its very first play even with active interaction", async () => {
  const {Audio} = fixture();
  const a = new Audio(CHIME);
  assert.equal(await a.play(), undefined);
  assert.equal(a.plays, 0);
  assert.equal(a.muted, false);
});

test("blocks chime repeatedly regardless of userActivation", async () => {
  const {Audio, navigator} = fixture();
  const a = new Audio(CHIME);
  for (const active of [true, false, true, false]) {
    navigator.userActivation.isActive = active;
    await a.play();
  }
  assert.equal(a.plays, 0);
});

test("blocks the same URL after fresh page context (no learning)", async () => {
  const first = fixture();
  const next = fixture();
  await new first.Audio(CHIME).play();
  const nextAudio = new next.Audio(CHIME);
  await nextAudio.play();
  assert.equal(nextAudio.plays, 0);
});

test("ignores query string but requires exact path and HTTPS origin", async () => {
  const {Audio} = fixture();
  const matched = new Audio(CHIME + "?cache=123");
  const alteredName = new Audio("https://static.whatsapp.net/rsrc.php/yW/r/OTHER.mp3");
  const alteredHost = new Audio("https://elsewhere.net/rsrc.php/yW/r/BS_BUUXbKq5.mp3");
  const insecure = new Audio("http://static.whatsapp.net/rsrc.php/yW/r/BS_BUUXbKq5.mp3");
  await Promise.all([matched.play(), alteredName.play(), alteredHost.play(), insecure.play()]);
  assert.equal(matched.plays, 0);
  assert.equal(alteredName.plays, 1);
  assert.equal(alteredHost.plays, 1);
  assert.equal(insecure.plays, 1);
});

test("does not block a different auto-playing sound from the same host", async () => {
  const {Audio, navigator} = fixture();
  navigator.userActivation.isActive = false;
  const a = new Audio("https://static.whatsapp.net/rsrc.php/yW/r/another.mp3");
  assert.equal(await a.play(), "played");
  assert.equal(a.plays, 1);
});

test("does not affect blob voice messages, video, or voice calls on another URL", async () => {
  const {Audio, Video} = fixture();
  const voice = new Audio("blob:https://web.whatsapp.com/voice123");
  const video = new Video("https://static.whatsapp.net/movie.mp4");
  const call = new Audio("https://static.whatsapp.net/call-ringtone.mp3");
  await Promise.all([voice.play(), video.play(), call.play()]);
  assert.equal(voice.plays, 1);
  assert.equal(video.plays, 1);
  assert.equal(call.plays, 1);
});

test("allows an audio element reused with a different source", async () => {
  const {Audio} = fixture();
  const audio = new Audio(CHIME);
  await audio.play();
  audio.src = "blob:https://web.whatsapp.com/voice123";
  audio.currentSrc = audio.src;
  assert.equal(await audio.play(), "played");
  assert.equal(audio.plays, 1);
});

test("does not modify existing mute state", async () => {
  const {Audio} = fixture();
  const audio = new Audio(CHIME, true);
  await audio.play();
  assert.equal(audio.muted, true);
});

test("does not patch pages other than WhatsApp Web", async () => {
  const {Audio, Media, originalPlay} = fixture("https://example.org");
  assert.equal(Media.prototype.play, originalPlay);
  const a = new Audio(CHIME);
  assert.equal(await a.play(), "played");
});

test("preserves original play() for malformed and empty URLs", async () => {
  const {Audio} = fixture();
  const broken = new Audio("://broken");
  const empty = new Audio("");
  await broken.play();
  await empty.play();
  assert.equal(broken.plays, 1);
  assert.equal(empty.plays, 1);
});
