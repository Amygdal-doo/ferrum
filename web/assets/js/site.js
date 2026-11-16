// Theme toggle, copy buttons, the home desk's dock and mobile nav. Vanilla, no dependencies; the page works without it.
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

    // Home desk: dock buttons switch the one app window. It steps through the
    // Apps group every 7s until someone uses the dock, pauses on hover or
    // focus, and never moves under reduced motion. #app-<slug> opens an app.
    Array.prototype.forEach.call(document.querySelectorAll("[data-desk]"), function (desk) {
      var reduce = window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
      var title = desk.querySelector("[data-desk-title]");
      var buttons = Array.prototype.slice.call(desk.querySelectorAll("[data-desk-show]"));
      var panels = {}, captions = {};
      Array.prototype.forEach.call(desk.querySelectorAll("[data-desk-app]"), function (el) { panels[el.getAttribute("data-desk-app")] = el; });
      Array.prototype.forEach.call(desk.querySelectorAll("[data-desk-caption]"), function (el) { captions[el.getAttribute("data-desk-caption")] = el; });
      // The auto tour covers the buttons before the divider: the Apps group.
      var tour = [];
      Array.prototype.some.call(desk.querySelectorAll(".appdock > li"), function (li) {
        if (li.classList.contains("appdock__sep")) return true;
        var b = li.querySelector("[data-desk-show]");
        if (b) tour.push(b.getAttribute("data-desk-show"));
        return false;
      });
      var current = null;
      buttons.forEach(function (b) { if (b.getAttribute("aria-pressed") === "true") current = b.getAttribute("data-desk-show"); });
      var outTimer = null, outPanel = null;

      function show(slug) {
        var next = panels[slug];
        if (!next || slug === current) return;
        var prev = panels[current];
        buttons.forEach(function (b) { b.setAttribute("aria-pressed", b.getAttribute("data-desk-show") === slug ? "true" : "false"); });
        Object.keys(captions).forEach(function (k) { captions[k].hidden = k !== slug; });
        if (title) title.textContent = next.getAttribute("data-name");
        if (outPanel) { clearTimeout(outTimer); outPanel.classList.remove("is-out"); outPanel.hidden = true; outPanel = null; }
        next.classList.remove("is-out");
        next.hidden = false;
        if (prev && !reduce) {
          next.classList.remove("is-in"); void next.offsetWidth; next.classList.add("is-in");
          prev.classList.remove("is-in"); prev.classList.add("is-out");
          outPanel = prev;
          outTimer = setTimeout(function () { prev.classList.remove("is-out"); prev.hidden = true; outPanel = null; }, 260);
        } else if (prev) {
          prev.hidden = true;
        }
        current = slug;
      }

      var timer = null, stopped = reduce, held = false;
      function tick() {
        if (stopped || held || document.hidden) return;
        var i = tour.indexOf(current);
        show(tour[(i + 1) % tour.length]);
      }
      function stop() { stopped = true; clearInterval(timer); }
      if (!stopped && tour.length > 1) timer = setInterval(tick, 7000);
      desk.addEventListener("mouseenter", function () { held = true; });
      desk.addEventListener("mouseleave", function () { held = false; });
      desk.addEventListener("focusin", function () { held = true; });
      desk.addEventListener("focusout", function () { held = false; });

      buttons.forEach(function (b) {
        b.addEventListener("click", function () { stop(); show(b.getAttribute("data-desk-show")); });
      });
      var hash = location.hash.replace(/^#app-/, "");
      if (hash !== location.hash && panels[hash]) { stop(); show(hash); }
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
