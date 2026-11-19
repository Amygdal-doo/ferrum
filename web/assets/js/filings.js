// Iron filings for the home hero (<canvas data-filings>). A grid of short
// segments lines up with the field of a bar magnet that follows the pointer;
// when the pointer is idle the magnet drifts on its own. Filings near the
// magnet take on the temper colours of heated steel (tokens.css).
// Reduced motion draws one still frame. Vanilla, no dependencies.
// After an htmx page change it starts on the new page's canvas and stops the old one.
(function () {
// Starts the animation on `canvas`; returns a function that stops it.
function mount(canvas) {
  var ctx = canvas.getContext("2d");
  var root = document.documentElement;
  var still = window.matchMedia("(prefers-reduced-motion: reduce)");

  var SPACING = 15;
  var IDLE_MS = 2500;
  var filings = [];
  var palette = [];
  var width = 0, height = 0, dpr = 1;
  var magnet = { x: 0, y: 0, angle: 0.4 };
  var target = { x: 0, y: 0 };
  var pointerAt = -Infinity;
  var running = false, visible = true, frame = 0;

  function readPalette() {
    var css = getComputedStyle(root);
    function v(name) { return css.getPropertyValue(name).trim(); }
    // Weak field to strong: plain steel, then blue, violet, bronze, straw.
    palette = [v("--filing"), v("--heat-1"), v("--heat-2"), v("--heat-3"), v("--heat-4")];
  }

  function layout() {
    var rect = canvas.getBoundingClientRect();
    dpr = Math.min(window.devicePixelRatio || 1, 2);
    width = rect.width; height = rect.height;
    canvas.width = Math.round(width * dpr);
    canvas.height = Math.round(height * dpr);
    var spacing = width < 700 ? SPACING + 3 : SPACING;
    filings = [];
    for (var y = spacing / 2; y < height; y += spacing) {
      for (var x = spacing / 2; x < width; x += spacing) {
        var jx = x + (Math.random() - 0.5) * spacing * 0.6;
        var jy = y + (Math.random() - 0.5) * spacing * 0.6;
        var rest = Math.random() * Math.PI;
        filings.push({ x: jx, y: jy, rest: rest, angle: rest, strength: 0 });
      }
    }
    if (!target.x) { target.x = magnet.x = width * 0.72; target.y = magnet.y = height * 0.45; }
  }

  // Field of a bar magnet: north and south poles either side of the centre.
  function field(px, py) {
    var half = Math.min(width, height) * 0.09;
    var cx = Math.cos(magnet.angle) * half, cy = Math.sin(magnet.angle) * half;
    var bx = 0, by = 0;
    for (var i = 0; i < 2; i++) {
      var sign = i === 0 ? 1 : -1;
      var dx = px - (magnet.x + sign * cx), dy = py - (magnet.y + sign * cy);
      var r2 = dx * dx + dy * dy + 400;
      var inv = sign / (r2 * Math.sqrt(r2));
      bx += dx * inv; by += dy * inv;
    }
    return { x: bx, y: by };
  }

  function idleTarget(t) {
    // A slow figure-eight over the right half of the hero.
    target.x = width * (0.68 + 0.18 * Math.sin(t / 5200));
    target.y = height * (0.45 + 0.22 * Math.sin(t / 2600));
  }

  function step(t) {
    if (t - pointerAt > IDLE_MS) idleTarget(t);
    magnet.x += (target.x - magnet.x) * 0.06;
    magnet.y += (target.y - magnet.y) * 0.06;
    magnet.angle = 0.4 + Math.sin(t / 7000) * 0.9;
    var scale = Math.pow(Math.min(width, height) * 0.09, 2) * 1.6;
    for (var i = 0; i < filings.length; i++) {
      var f = filings[i];
      var b = field(f.x, f.y);
      var mag = Math.sqrt(b.x * b.x + b.y * b.y) * scale;
      var s = mag / (1 + mag); // 0..1
      // Blend the rest orientation into the field direction as the field grows.
      var w = Math.min(1, s * 3);
      var tx = Math.cos(f.rest) * (1 - w) + (b.x / (mag / scale || 1)) * w;
      var ty = Math.sin(f.rest) * (1 - w) + (b.y / (mag / scale || 1)) * w;
      var goal = Math.atan2(ty, tx);
      // Filings are symmetric, so turn the short way round (mod pi).
      var d = goal - f.angle;
      d = Math.atan2(Math.sin(2 * d), Math.cos(2 * d)) / 2;
      f.angle += d * 0.18;
      f.strength = s;
    }
  }

  function draw() {
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, height);
    ctx.lineCap = "round";
    var buckets = [[], [], [], [], []];
    for (var i = 0; i < filings.length; i++) {
      var s = filings[i].strength;
      var k = s < 0.18 ? 0 : s < 0.32 ? 1 : s < 0.48 ? 2 : s < 0.66 ? 3 : 4;
      buckets[k].push(filings[i]);
    }
    for (var k = 0; k < buckets.length; k++) {
      ctx.strokeStyle = palette[k];
      ctx.globalAlpha = k === 0 ? 0.7 : 0.95;
      ctx.lineWidth = k === 0 ? 1.2 : 1.6;
      ctx.beginPath();
      for (var j = 0; j < buckets[k].length; j++) {
        var f = buckets[k][j];
        var len = 3.5 + f.strength * 5;
        var dx = Math.cos(f.angle) * len, dy = Math.sin(f.angle) * len;
        ctx.moveTo(f.x - dx, f.y - dy);
        ctx.lineTo(f.x + dx, f.y + dy);
      }
      ctx.stroke();
    }
    ctx.globalAlpha = 1;
  }

  function settle() {
    // Enough steps for the filings to turn into place for a still frame.
    for (var i = 0; i < 40; i++) step(0);
    draw();
  }

  function loop(t) {
    if (!running) return;
    step(t);
    draw();
    frame = requestAnimationFrame(loop);
  }

  function start() {
    if (running || still.matches || !visible || document.hidden) return;
    running = true;
    frame = requestAnimationFrame(loop);
  }

  function stop() {
    running = false;
    cancelAnimationFrame(frame);
  }

  var hero = canvas.parentElement;
  hero.addEventListener("pointermove", function (e) {
    var rect = canvas.getBoundingClientRect();
    target.x = e.clientX - rect.left;
    target.y = e.clientY - rect.top;
    pointerAt = performance.now();
  });

  var io = null;
  if ("IntersectionObserver" in window) {
    io = new IntersectionObserver(function (entries) {
      visible = entries[0].isIntersecting;
      if (visible) start(); else stop();
    });
    io.observe(canvas);
  }
  function onVisibility() {
    if (document.hidden) stop(); else start();
  }
  document.addEventListener("visibilitychange", onVisibility);
  var resizeTimer = 0;
  function onResize() {
    clearTimeout(resizeTimer);
    resizeTimer = setTimeout(function () { layout(); if (!running) settle(); }, 120);
  }
  window.addEventListener("resize", onResize);
  // The theme toggle changes data-theme on <html>; recolour with it.
  var themeObserver = new MutationObserver(function () { readPalette(); if (!running) draw(); });
  themeObserver.observe(root, { attributes: true, attributeFilter: ["data-theme"] });
  function onMotionPref() { if (still.matches) { stop(); settle(); } else start(); }
  still.addEventListener("change", onMotionPref);

  readPalette();
  layout();
  settle();
  start();

  return function unmount() {
    stop();
    clearTimeout(resizeTimer);
    if (io) io.disconnect();
    themeObserver.disconnect();
    document.removeEventListener("visibilitychange", onVisibility);
    window.removeEventListener("resize", onResize);
    still.removeEventListener("change", onMotionPref);
  };
}

var current = null; // { canvas, unmount }
function attach() {
  var canvas = document.querySelector("canvas[data-filings]");
  if (current && current.canvas === canvas) return;
  if (current) current.unmount();
  current = canvas && canvas.getContext ? { canvas: canvas, unmount: mount(canvas) } : null;
}
attach();
document.addEventListener("htmx:afterSettle", attach);
})();
