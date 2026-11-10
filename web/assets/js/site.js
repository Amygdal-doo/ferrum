// Theme toggle, copy buttons, home showcase tabs and mobile nav. Vanilla, no dependencies; the page works without it.
(function () {
  var root = document.documentElement;
  root.classList.add("js");

  function init() {
    // Theme toggle: dark <-> light, persisted to localStorage "ferrum-theme".
    var themeBtn = document.querySelector("[data-theme-toggle]");
    function syncThemeLabel() {
      if (!themeBtn) return;
      var dark = root.getAttribute("data-theme") !== "light";
      themeBtn.setAttribute("aria-label", dark ? "Switch to light theme" : "Switch to dark theme");
    }
    if (themeBtn) {
      syncThemeLabel();
      themeBtn.addEventListener("click", function () {
        var next = root.getAttribute("data-theme") === "light" ? "dark" : "light";
        root.setAttribute("data-theme", next);
        try { localStorage.setItem("ferrum-theme", next); } catch (e) { /* storage blocked */ }
        syncThemeLabel();
      });
    }

    // Copy buttons: <button data-copy="text">. The label confirms, then resets.
    Array.prototype.forEach.call(document.querySelectorAll("[data-copy]"), function (btn) {
      var label = btn.textContent;
      btn.addEventListener("click", function () {
        if (!navigator.clipboard) return;
        navigator.clipboard.writeText(btn.getAttribute("data-copy")).then(function () {
          btn.textContent = "Copied";
          setTimeout(function () { btn.textContent = label; }, 1600);
        });
      });
    });

    // Home showcase: ARIA tabs. Click, or arrow keys / Home / End when focused.
    Array.prototype.forEach.call(document.querySelectorAll("[data-showcase]"), function (box) {
      var tabs = Array.prototype.slice.call(box.querySelectorAll('[role="tab"]'));
      function select(tab, focus) {
        tabs.forEach(function (t) {
          var on = t === tab;
          t.setAttribute("aria-selected", on ? "true" : "false");
          t.tabIndex = on ? 0 : -1;
          var panel = document.getElementById(t.getAttribute("aria-controls"));
          if (panel) panel.hidden = !on;
        });
        if (focus) tab.focus();
      }
      tabs.forEach(function (tab, i) {
        tab.addEventListener("click", function () { select(tab, false); });
        tab.addEventListener("keydown", function (e) {
          var next = { ArrowDown: i + 1, ArrowRight: i + 1, ArrowUp: i - 1, ArrowLeft: i - 1, Home: 0, End: tabs.length - 1 }[e.key];
          if (next === undefined) return;
          e.preventDefault();
          select(tabs[(next + tabs.length) % tabs.length], true);
        });
      });
    });

    // Mobile nav: toggles .is-open on the header; closes on Escape and link click.
    var header = document.querySelector(".site-header");
    var navBtn = document.querySelector("[data-nav-toggle]");
    if (!header || !navBtn) return;
    var nav = document.getElementById(navBtn.getAttribute("aria-controls") || "");
    function setOpen(open, restoreFocus) {
      header.classList.toggle("is-open", open);
      navBtn.setAttribute("aria-expanded", open ? "true" : "false");
      navBtn.setAttribute("aria-label", open ? "Close menu" : "Menu");
      if (!open && restoreFocus) navBtn.focus();
    }
    navBtn.addEventListener("click", function () {
      setOpen(navBtn.getAttribute("aria-expanded") !== "true", false);
    });
    document.addEventListener("keydown", function (e) {
      if (e.key === "Escape" && header.classList.contains("is-open")) setOpen(false, true);
    });
    if (nav) {
      nav.addEventListener("click", function (e) {
        if (e.target && e.target.closest && e.target.closest("a")) setOpen(false, false);
      });
    }
  }

  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", init);
  else init();
})();
