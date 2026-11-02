// Reveals, counters and the scenes that move. Everything waits until it is on screen.
(() => {
  const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;

  // ---- Counters: <b data-count="192.6" data-from="281.1" data-dec="1" data-dur="1400" data-delay="0">
  const fmt = (v, dec) => v.toLocaleString("en-US", { minimumFractionDigits: dec, maximumFractionDigits: dec });
  function runCounter(el) {
    const to = parseFloat(el.dataset.count);
    const from = parseFloat(el.dataset.from || "0");
    const dec = parseInt(el.dataset.dec || "0", 10);
    const dur = parseInt(el.dataset.dur || "1200", 10);
    const delay = parseInt(el.dataset.delay || "0", 10);
    if (reduced) { el.textContent = fmt(to, dec); return; }
    el.textContent = fmt(from, dec);
    const start = performance.now() + delay;
    const ease = t => 1 - Math.pow(1 - t, 3);
    function tick(now) {
      const t = Math.min(1, Math.max(0, (now - start) / dur));
      el.textContent = fmt(from + (to - from) * ease(t), dec);
      if (t < 1) requestAnimationFrame(tick);
    }
    requestAnimationFrame(tick);
  }
  function runCounters(root) { root.querySelectorAll("[data-count]").forEach(runCounter); }

  // ---- Reveal once: fade in, and start any counters inside.
  const revealer = new IntersectionObserver(entries => {
    for (const e of entries) {
      if (!e.isIntersecting) continue;
      e.target.classList.add("in");
      if (!e.target.hasAttribute("data-scene")) runCounters(e.target);
      revealer.unobserve(e.target);
    }
  }, { rootMargin: "0px 0px -8% 0px", threshold: 0.12 });
  document.querySelectorAll(".reveal").forEach(el => revealer.observe(el));

  // ---- Scenes replay each time they come back on screen.
  const scenes = new Map();
  const player = new IntersectionObserver(entries => {
    for (const e of entries) {
      const el = e.target;
      if (e.isIntersecting) {
        el.classList.add("in");
        runCounters(el);
        if (el.dataset.scene === "repair") startRepair(el);
      } else {
        el.classList.remove("in");
        if (el.dataset.scene === "repair") stopRepair(el);
      }
    }
  }, { threshold: 0.35 });
  document.querySelectorAll("[data-scene]").forEach(el => player.observe(el));

  // ---- The formats orbit needs its size in pixels for the chip positions.
  const orbit = document.querySelector(".orbit-wrap");
  if (orbit) {
    const size = () => orbit.style.setProperty("--size", orbit.clientWidth + "px");
    size();
    addEventListener("resize", size);
  }

  // ---- The repair scene: damage, sweep, repair, rest, again.
  const COLS = matchMedia("(max-width: 760px)").matches ? 16 : 24;
  const ROWS = 4;
  function buildRepair(el) {
    const grid = el.querySelector(".grid"), parity = el.querySelector(".parity");
    if (grid.children.length) return;
    for (let i = 0; i < COLS * ROWS; i++) grid.appendChild(document.createElement("i"));
    for (let i = 0; i < COLS; i++) parity.appendChild(document.createElement("i"));
  }
  function startRepair(el) {
    buildRepair(el);
    stopRepair(el);
    const grid = [...el.querySelector(".grid").children];
    const parity = [...el.querySelector(".parity").children];
    const timers = [];
    const at = (ms, fn) => timers.push(setTimeout(fn, ms));
    const cycle = () => {
      el.classList.remove("damaged", "fixed", "sweeping");
      grid.forEach(b => b.className = "");
      parity.forEach(p => p.className = "");
      const bad = new Set();
      while (bad.size < 7) bad.add(Math.floor(Math.random() * grid.length));
      at(700, () => { bad.forEach(i => grid[i].classList.add("bad")); el.classList.add("damaged"); });
      // The sweep crosses the grid in 1.6 s; each damaged block is rebuilt as it passes.
      at(1900, () => el.classList.add("sweeping"));
      bad.forEach(i => {
        const col = i % COLS;
        at(1900 + 80 + col * (1500 / COLS), () => { grid[i].classList.replace("bad", "ok"); parity[col].classList.add("used"); });
      });
      at(3700, () => { el.classList.remove("damaged"); el.classList.add("fixed"); });
      at(7200, cycle);
    };
    if (reduced) {
      [3, 11, 20, 37, 52, 66, 80].forEach(i => grid[i] && grid[i].classList.add("ok"));
      el.classList.add("fixed");
      return;
    }
    cycle();
    scenes.set(el, timers);
  }
  function stopRepair(el) {
    const timers = scenes.get(el);
    if (timers) { timers.forEach(clearTimeout); scenes.delete(el); }
  }
})();
