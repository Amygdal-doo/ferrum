// Cookie banner for Google Analytics. Google's tag loads only after Accept;
// the choice is kept in localStorage "ferrum-consent" ("granted" / "denied").
// [data-consent-open] (footer "Cookie settings") shows the banner again.
// After an htmx page change the banner comes back if no choice was made.
// Google's tag counts those page changes itself (they change the history).
(function () {
  var meta = document.querySelector('meta[name="ga-measurement-id"]');
  if (!meta) return;
  var id = meta.getAttribute("content");
  var KEY = "ferrum-consent";
  var loaded = false;

  function read() {
    try { return localStorage.getItem(KEY); } catch (e) { return null; }
  }
  function save(value) {
    try { localStorage.setItem(KEY, value); } catch (e) { /* storage blocked */ }
  }

  function load() {
    if (loaded) return;
    loaded = true;
    window.dataLayer = window.dataLayer || [];
    window.gtag = function () { window.dataLayer.push(arguments); };
    window.gtag("js", new Date());
    window.gtag("config", id);
    var s = document.createElement("script");
    s.async = true;
    s.src = "https://www.googletagmanager.com/gtag/js?id=" + encodeURIComponent(id);
    document.head.appendChild(s);
  }

  // Declining after accepting removes Google's cookies for this site.
  function clearCookies() {
    document.cookie.split(";").forEach(function (c) {
      var name = c.split("=")[0].trim();
      if (name === "_ga" || name.indexOf("_ga_") === 0) {
        var host = location.hostname;
        ["", host, "." + host].forEach(function (domain) {
          document.cookie = name + "=; Max-Age=0; path=/" + (domain ? "; domain=" + domain : "");
        });
      }
    });
  }

  var banner;
  function hide() {
    if (banner) banner.hidden = true;
  }
  function show() {
    if (!banner) {
      banner = document.createElement("section");
      banner.className = "consent";
      banner.setAttribute("aria-label", "Cookies");
      banner.innerHTML =
        '<p class="consent__text">May we use Google Analytics cookies to count visits? ' +
        'The apps never track you; this is only about this website. ' +
        '<a href="/privacy#analytics">Details</a></p>' +
        '<div class="consent__actions">' +
        '<button type="button" class="btn btn--secondary" data-consent="denied">Decline</button>' +
        '<button type="button" class="btn btn--primary" data-consent="granted">Accept</button>' +
        "</div>";
      banner.addEventListener("click", function (e) {
        var choice = e.target.getAttribute && e.target.getAttribute("data-consent");
        if (!choice) return;
        save(choice);
        hide();
        if (choice === "granted") {
          load();
        } else {
          clearCookies();
          if (loaded) location.reload();
        }
      });
    }
    // htmx replaces the body's content on a page change, banner included.
    if (!document.body.contains(banner)) document.body.appendChild(banner);
    banner.hidden = false;
  }

  function init() {
    var choice = read();
    if (choice === "granted") load();
    else if (choice !== "denied") show();
  }

  document.addEventListener("click", function (e) {
    if (e.target && e.target.closest && e.target.closest("[data-consent-open]")) show();
  });
  document.addEventListener("htmx:afterSettle", function (e) {
    if (!e.detail.boosted) return;
    var choice = read();
    if (choice !== "granted" && choice !== "denied") show();
  });

  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", init);
  else init();
})();
