"use strict";
// Release notes (GitHub Markdown) shown as simple, safe elements.
//
// Only a small subset is understood: #-headings, "-"/"*" lists, paragraphs,
// fenced code, **bold**, *italic* and `code`. Everything is built with
// createElement + textContent, never by parsing HTML, so HTML written in a release body
// is shown as literal text and cannot run. Links and images are not followed:
// a link keeps its text, an image (or a badge made of one) is dropped.
(function (root) {
  function inline(text) {
    // images first (a badge is a link around an image), then links keep their text
    text = text.replace(/!\[[^\]]*\]\([^)]*\)/g, "").replace(/\[([^\]]*)\]\([^)]*\)/g, "$1");
    var out = [], re = /(\*\*[^*]+\*\*|`[^`]+`|\*[^*\s][^*]*\*)/g, last = 0, m;
    while ((m = re.exec(text))) {
      if (m.index > last) out.push({ t: "text", s: text.slice(last, m.index) });
      var tok = m[0];
      if (tok.startsWith("**")) out.push({ t: "b", s: tok.slice(2, -2) });
      else if (tok.startsWith("`")) out.push({ t: "code", s: tok.slice(1, -1) });
      else out.push({ t: "i", s: tok.slice(1, -1) });
      last = m.index + tok.length;
    }
    if (last < text.length) out.push({ t: "text", s: text.slice(last) });
    return out.filter(function (x) { return x.s !== ""; });
  }

  function parse(md) {
    var blocks = [], lines = String(md || "").replace(/\r\n?/g, "\n").split("\n"), para = [], list = null, fence = null;
    function flush() {
      if (para.length) { blocks.push({ k: "p", c: inline(para.join(" ")) }); para = []; }
      if (list) { blocks.push({ k: "ul", items: list }); list = null; }
    }
    for (var i = 0; i < lines.length; i++) {
      var line = lines[i], m;
      if (fence) {
        if (/^\s*```/.test(line)) { blocks.push({ k: "pre", s: fence.join("\n") }); fence = null; }
        else fence.push(line);
        continue;
      }
      if (/^\s*```/.test(line)) { flush(); fence = []; continue; }
      if (!line.trim()) { flush(); continue; }
      if ((m = /^\s{0,3}#{1,6}\s+(.*?)\s*#*\s*$/.exec(line))) { flush(); blocks.push({ k: "h", c: inline(m[1]) }); continue; }
      if (/^\s{0,3}([-*_])(\s*\1){2,}\s*$/.test(line)) { flush(); continue; }
      if ((m = /^\s*[-*+]\s+(.*)$/.exec(line))) {
        if (para.length) { blocks.push({ k: "p", c: inline(para.join(" ")) }); para = []; }
        (list = list || []).push(inline(m[1]));
        continue;
      }
      if (list && /^\s{2,}\S/.test(line)) { // continuation of the previous bullet
        list[list.length - 1] = list[list.length - 1].concat(inline(" " + line.trim()));
        continue;
      }
      if (list) { blocks.push({ k: "ul", items: list }); list = null; }
      para.push(line.replace(/^\s*>\s?/, "").trim());
    }
    if (fence) blocks.push({ k: "pre", s: fence.join("\n") });
    flush();
    // a paragraph that was only an image or a badge is empty after inline(): drop it
    return blocks.filter(function (b) { return !(b.k === "p" && !b.c.length) && !(b.k === "h" && !b.c.length); });
  }

  function fill(doc, el, toks) {
    toks.forEach(function (x) {
      if (x.t === "text") { el.appendChild(doc.createTextNode(x.s)); return; }
      var tag = x.t === "b" ? "strong" : x.t === "i" ? "em" : "code";
      var n = doc.createElement(tag);
      n.textContent = x.s;
      el.appendChild(n);
    });
  }

  function render(doc, container, md) {
    while (container.firstChild) container.removeChild(container.firstChild);
    parse(md).forEach(function (b) {
      var el;
      if (b.k === "h") { el = doc.createElement("h4"); fill(doc, el, b.c); }
      else if (b.k === "p") { el = doc.createElement("p"); fill(doc, el, b.c); }
      else if (b.k === "pre") { el = doc.createElement("pre"); el.textContent = b.s; }
      else {
        el = doc.createElement("ul");
        b.items.forEach(function (it) { var li = doc.createElement("li"); fill(doc, li, it); el.appendChild(li); });
      }
      container.appendChild(el);
    });
  }

  var api = { parse: parse, inline: inline, render: render };
  if (typeof module !== "undefined" && module.exports) module.exports = api;
  else root.ReleaseNotes = api;
})(typeof window !== "undefined" ? window : globalThis);
