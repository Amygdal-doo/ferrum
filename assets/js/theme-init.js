// Applies the saved theme before first paint. Dark is the default.
document.documentElement.classList.add("js");
(function () {
  try {
    var t = localStorage.getItem("ferrum-theme");
    if (t === "light" || t === "dark") document.documentElement.setAttribute("data-theme", t);
  } catch (e) { /* storage blocked: keep dark */ }
})();
