"use strict";
// Pure helpers for the profile picker cards on the shell page (issue #102).
// Kept free of the DOM so Node can test them (profiles.test.mjs).
(function (root) {
  var api = {
    // First letter of the account name, upper-cased; "?" for an empty name.
    initial: function (name) {
      var chars = Array.from(String(name == null ? "" : name).trim());
      return chars.length ? chars[0].toUpperCase() : "?";
    },
    // Text of the unread badge: empty for none, "99+" above 99.
    badgeText: function (unread) {
      var n = Number(unread) || 0;
      return n <= 0 ? "" : n > 99 ? "99+" : String(n);
    },
    // Accounts in display order: the saved order, ties by name.
    sorted: function (list) {
      return (list || []).slice().sort(function (a, b) {
        return (a.order - b.order) || String(a.name).localeCompare(String(b.name));
      });
    },
  };
  if (typeof module !== "undefined" && module.exports) module.exports = api;
  else root.Profiles = api;
})(typeof window !== "undefined" ? window : globalThis);
