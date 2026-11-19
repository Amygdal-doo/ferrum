// Theme toggle, copy buttons, mobile nav. Vanilla, no dependencies; the page works without it.
// Listeners sit on the document, so they keep working after htmx swaps the body.
(function () {
  var root = document.documentElement;
  root.classList.add("js");

  function syncThemeLabel() {
    var btn = document.querySelector("[data-theme-toggle]");
    if (!btn) return;
    var dark = root.getAttribute("data-theme") !== "light";
    btn.setAttribute("aria-label", dark ? "Switch to light theme" : "Switch to dark theme");
  }

  // Mobile nav: toggles .is-open on the header; closes on Escape and link click.
  function setNavOpen(open, restoreFocus) {
    var header = document.querySelector(".site-header");
    var navBtn = document.querySelector("[data-nav-toggle]");
    if (!header || !navBtn) return;
    header.classList.toggle("is-open", open);
    navBtn.setAttribute("aria-expanded", open ? "true" : "false");
    navBtn.setAttribute("aria-label", open ? "Close menu" : "Menu");
    if (!open && restoreFocus) navBtn.focus();
  }

  document.addEventListener("click", function (e) {
    var el = e.target && e.target.closest ? e.target : null;
    if (!el) return;

    // Theme toggle: dark <-> light, persisted to localStorage "ferrum-theme".
    if (el.closest("[data-theme-toggle]")) {
      var next = root.getAttribute("data-theme") === "light" ? "dark" : "light";
      root.setAttribute("data-theme", next);
      try { localStorage.setItem("ferrum-theme", next); } catch (err) { /* storage blocked */ }
      syncThemeLabel();
      return;
    }

    // Copy buttons: <button data-copy="text">. The label confirms, then resets.
    var copy = el.closest("[data-copy]");
    if (copy) {
      if (!navigator.clipboard || copy.hasAttribute("data-copied")) return;
      var label = copy.textContent;
      navigator.clipboard.writeText(copy.getAttribute("data-copy")).then(function () {
        copy.setAttribute("data-copied", "");
        copy.textContent = "Copied";
        setTimeout(function () { copy.textContent = label; copy.removeAttribute("data-copied"); }, 1600);
      });
      return;
    }

    var navBtn = el.closest("[data-nav-toggle]");
    if (navBtn) {
      setNavOpen(navBtn.getAttribute("aria-expanded") !== "true", false);
      return;
    }
    var toggle = document.querySelector("[data-nav-toggle]");
    var nav = toggle && document.getElementById(toggle.getAttribute("aria-controls") || "");
    if (nav && nav.contains(el) && el.closest("a")) setNavOpen(false, false);
  });

  // Button menus (<details data-menu>): one open at a time, closed by a
  // click outside or Escape.
  function closeMenus(except) {
    Array.prototype.forEach.call(document.querySelectorAll("details[data-menu][open]"), function (m) {
      if (m !== except) m.removeAttribute("open");
    });
  }
  document.addEventListener("click", function (e) {
    var menu = e.target && e.target.closest ? e.target.closest("details[data-menu]") : null;
    closeMenus(menu);
    // A chosen link starts a download and the page stays: close the menu.
    if (menu && e.target.closest("a")) menu.removeAttribute("open");
  });

  document.addEventListener("keydown", function (e) {
    if (e.key === "Escape") {
      var open = document.querySelector("details[data-menu][open]");
      if (open) {
        open.removeAttribute("open");
        open.querySelector("summary").focus();
        return;
      }
    }
    var header = document.querySelector(".site-header");
    if (e.key === "Escape" && header && header.classList.contains("is-open")) setNavOpen(false, true);
  });

  // htmx page changes (hx-boost on <body>).
  // The body's page class (e.g. "page-home") comes with the new page.
  document.addEventListener("htmx:beforeSwap", function (e) {
    var d = e.detail;
    if (!d.boosted) return;
    // A missing page or a server error: load it normally, so the address bar
    // and the status code match what is shown.
    if (d.xhr.status >= 400) {
      d.shouldSwap = false;
      location.href = d.pathInfo.requestPath;
      return;
    }
    var m = /<body[^>]*\sclass="([^"]*)"/i.exec(d.serverResponse);
    document.body.className = m ? m[1] : "";
  });
  // The network failed: let the browser try, and show its own error page.
  document.addEventListener("htmx:sendError", function (e) {
    var path = e.detail.pathInfo && e.detail.pathInfo.requestPath;
    if (e.detail.boosted && path) location.href = path;
  });
  // Moves focus to the new content, as a full page load would for screen readers.
  document.addEventListener("htmx:afterSettle", function (e) {
    if (!e.detail.boosted) return;
    syncThemeLabel();
    var main = document.getElementById("main");
    if (main && location.hash === "") {
      main.setAttribute("tabindex", "-1");
      main.focus({ preventScroll: true });
    }
  });

  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", syncThemeLabel);
  else syncThemeLabel();
})();
