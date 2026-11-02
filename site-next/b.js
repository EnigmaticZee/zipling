// Variant B: the motion system in plain JavaScript. Fluid hover (one
// highlight per list, gliding under the cursor), tabs with a hover pill and
// a selected pill, weight that changes without reflow, reveals, counters,
// and the scenes that loop while they are on screen.
(() => {
  const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;

  // ---- Fluid hover: one overlay per .fh container, moved by transform.
  function fluidHover(container, { nearest = false } = {}) {
    const items = [...container.querySelectorAll("[data-fh]")];
    if (!items.length) return;
    const hl = document.createElement("span");
    hl.className = "fh-hl";
    container.prepend(hl);
    let lit = null, fresh = true;
    const place = (el) => {
      const r = el.getBoundingClientRect(), c = container.getBoundingClientRect();
      hl.style.width = r.width + "px";
      hl.style.height = r.height + "px";
      hl.style.transform = `translate(${r.left - c.left + container.scrollLeft}px, ${r.top - c.top + container.scrollTop}px)`;
    };
    const light = (el) => {
      if (el === lit) return;
      if (fresh || reduced) { hl.style.transition = "none"; place(el); void hl.offsetWidth; hl.style.transition = ""; fresh = false; }
      else place(el);
      hl.classList.remove("off");
      hl.style.opacity = "1";
      lit = el;
    };
    const out = () => { hl.classList.add("off"); hl.style.opacity = "0"; lit = null; };
    container.addEventListener("pointerenter", () => { fresh = true; });
    container.addEventListener("pointermove", (e) => {
      let target = items.find(el => el.contains(e.target));
      if (!target && nearest) {
        let best = Infinity;
        for (const el of items) {
          const r = el.getBoundingClientRect(), dx = Math.max(r.left - e.clientX, 0, e.clientX - r.right), dy = Math.max(r.top - e.clientY, 0, e.clientY - r.bottom);
          const d = dx * dx + dy * dy; if (d < best) { best = d; target = el; }
        }
      }
      target ? light(target) : out();
    });
    container.addEventListener("pointerleave", out);
  }
  document.querySelectorAll(".fh").forEach(el => fluidHover(el, { nearest: el.hasAttribute("data-nearest") }));

  // ---- Tabs: the hover pill springs on fast, the selected pill on moderate.
  document.querySelectorAll("[data-tabs]").forEach(tabs => {
    const buttons = [...tabs.querySelectorAll(".tab")];
    const panels = [...document.querySelectorAll(`[data-panels="${tabs.dataset.tabs}"] .panel`)];
    const sel = document.createElement("span"); sel.className = "pill sel";
    const hov = document.createElement("span"); hov.className = "pill hov";
    tabs.prepend(hov); tabs.prepend(sel);
    const place = (pill, el) => { pill.style.width = el.offsetWidth + "px"; pill.style.height = el.offsetHeight + "px"; pill.style.transform = `translate(${el.offsetLeft}px, ${el.offsetTop}px)`; };
    let current = buttons.findIndex(b => b.classList.contains("on"));
    if (current < 0) current = 0;
    const select = (i) => {
      current = i;
      buttons.forEach((b, j) => { b.classList.toggle("on", j === i); b.setAttribute("aria-selected", j === i); b.tabIndex = j === i ? 0 : -1; });
      panels.forEach((p, j) => { p.classList.toggle("on", j === i); if (j === i) p.classList.add("in"); });
      place(sel, buttons[i]);
    };
    buttons.forEach((b, i) => {
      b.addEventListener("click", () => select(i));
      b.addEventListener("pointerenter", () => { place(hov, b); hov.style.opacity = i === current ? "0" : ".6"; sel.style.opacity = i === current ? "1" : ".7"; });
      b.addEventListener("keydown", (e) => {
        const n = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
        if (!n) return;
        e.preventDefault();
        const j = (current + n + buttons.length) % buttons.length;
        select(j); buttons[j].focus();
      });
    });
    tabs.addEventListener("pointerleave", () => { place(hov, buttons[current]); hov.style.opacity = "0"; sel.style.opacity = "1"; });
    sel.style.transition = "none"; select(current); void sel.offsetWidth; sel.style.transition = "";
    addEventListener("resize", () => place(sel, buttons[current]));
  });

  // ---- Counters.
  const fmt = (v, dec) => v.toLocaleString("en-US", { minimumFractionDigits: dec, maximumFractionDigits: dec });
  function runCounter(el) {
    const to = parseFloat(el.dataset.count), from = parseFloat(el.dataset.from || "0");
    const dec = parseInt(el.dataset.dec || "0", 10), dur = parseInt(el.dataset.dur || "900", 10), delay = parseInt(el.dataset.delay || "0", 10);
    if (reduced) { el.textContent = fmt(to, dec); return; }
    el.textContent = fmt(from, dec);
    const start = performance.now() + delay, ease = t => 1 - Math.pow(1 - t, 3);
    const tick = now => { const t = Math.min(1, Math.max(0, (now - start) / dur)); el.textContent = fmt(from + (to - from) * ease(t), dec); if (t < 1) requestAnimationFrame(tick); };
    requestAnimationFrame(tick);
  }
  const runCounters = root => root.querySelectorAll("[data-count]").forEach(runCounter);

  // ---- Reveals: groups, not elements; each plays once.
  const revealer = new IntersectionObserver(entries => {
    for (const e of entries) {
      if (!e.isIntersecting) continue;
      e.target.classList.add("in");
      if (!e.target.hasAttribute("data-scene")) runCounters(e.target);
      revealer.unobserve(e.target);
    }
  }, { rootMargin: "0px 0px -10% 0px", threshold: 0.1 });
  document.querySelectorAll(".rv, .rv-fade").forEach(el => revealer.observe(el));

  // ---- Scenes replay when they come back.
  const timers = new Map();
  const player = new IntersectionObserver(entries => {
    for (const e of entries) {
      const el = e.target;
      if (e.isIntersecting) { el.classList.add("in"); runCounters(el); if (el.dataset.scene === "repair") startRepair(el); }
      else { el.classList.remove("in"); if (el.dataset.scene === "repair") stopRepair(el); }
    }
  }, { threshold: 0.3 });
  document.querySelectorAll("[data-scene]").forEach(el => player.observe(el));

  // ---- The repair scene.
  const COLS = matchMedia("(max-width: 760px)").matches ? 16 : 24, ROWS = 4;
  function startRepair(el) {
    const grid = el.querySelector(".grid"), parity = el.querySelector(".parity");
    if (!grid.children.length) {
      for (let i = 0; i < COLS * ROWS; i++) grid.appendChild(document.createElement("i"));
      for (let i = 0; i < COLS; i++) parity.appendChild(document.createElement("i"));
    }
    stopRepair(el);
    const blocks = [...grid.children], par = [...parity.children], list = [];
    const at = (ms, fn) => list.push(setTimeout(fn, ms));
    if (reduced) { [3, 11, 20, 37, 52, 66, 80].forEach(i => blocks[i] && blocks[i].classList.add("ok")); el.classList.add("fixed"); return; }
    const cycle = () => {
      el.classList.remove("damaged", "fixed", "sweeping");
      blocks.forEach(b => b.className = ""); par.forEach(p => p.className = "");
      const bad = new Set(); while (bad.size < 7) bad.add(Math.floor(Math.random() * blocks.length));
      at(700, () => { bad.forEach(i => blocks[i].classList.add("bad")); el.classList.add("damaged"); });
      at(1900, () => el.classList.add("sweeping"));
      bad.forEach(i => { const col = i % COLS; at(1980 + col * (1500 / COLS), () => { blocks[i].classList.replace("bad", "ok"); par[col].classList.add("used"); }); });
      at(3700, () => { el.classList.remove("damaged"); el.classList.add("fixed"); });
      at(7200, cycle);
    };
    cycle();
    timers.set(el, list);
  }
  function stopRepair(el) { const l = timers.get(el); if (l) { l.forEach(clearTimeout); timers.delete(el); } }

  // ---- The current section in the navigation.
  const links = [...document.querySelectorAll(".nav-links a[href^='#']")];
  const spy = new IntersectionObserver(entries => {
    for (const e of entries) if (e.isIntersecting) links.forEach(a => a.setAttribute("aria-current", a.getAttribute("href") === "#" + e.target.id));
  }, { rootMargin: "-40% 0px -55% 0px" });
  links.forEach(a => { const s = document.querySelector(a.getAttribute("href")); if (s) spy.observe(s); });
})();
