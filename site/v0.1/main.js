// Zipling site: charts, comparison table, tooltips and micro-animations.
// Data lives in data.js (window.ZIPLING).

const Z = window.ZIPLING;
const TOOL = Object.fromEntries(Z.tools.map((t) => [t.id, t]));
const METRICS = [
  { key: 0, label: "Archive size", unit: "MB" },
  { key: 1, label: "Time to compress", unit: "s" },
  { key: 2, label: "Time to extract", unit: "s" },
];
const reduceMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;

function fmt(v, unit) {
  if (unit === "MB") return `${v.toFixed(1)} MB`;
  return `${v < 10 ? v.toFixed(v < 1 ? 2 : 1) : Math.round(v)} s`;
}
const pct = (size, mb) => `${((size / mb) * 100).toFixed(1)}%`;

// ---------------------------------------------------------------- tooltip

const tip = document.createElement("div");
tip.className = "tooltip";
tip.setAttribute("role", "status");
tip.hidden = true;
document.body.appendChild(tip);

function showTip(html, x, y) {
  tip.innerHTML = html;
  tip.hidden = false;
  const w = tip.offsetWidth, h = tip.offsetHeight;
  tip.style.left = `${Math.min(Math.max(8, x + 14), innerWidth - w - 8)}px`;
  tip.style.top = `${Math.min(Math.max(8, y - h - 12), innerHeight - h - 8)}px`;
}
const hideTip = () => { tip.hidden = true; };

function bindTip(el, html) {
  el.addEventListener("pointermove", (e) => showTip(html(), e.clientX, e.clientY));
  el.addEventListener("pointerleave", hideTip);
  el.addEventListener("focus", () => { const b = el.getBoundingClientRect(); showTip(html(), b.left + 40, b.top); });
  el.addEventListener("blur", hideTip);
}

// ---------------------------------------------------------------- charts

function toolTip(t, ds) {
  const [s, p, u] = ds.r[t.id];
  return `<b>${t.name}</b><span>${t.detail}</span><br>${fmt(s, "MB")} · ${pct(s, ds.mb)} of original<br>` +
    `Compress ${fmt(p, "s")} · Extract ${fmt(u, "s")}` +
    (t.sep ? `<br><span>† Measured in a separate session</span>` : "");
}

// Three bar charts (size, compress, extract) for one dataset.
function renderCharts(host, ds, ids) {
  host.innerHTML = "";
  for (const m of METRICS) {
    const fig = document.createElement("figure");
    fig.className = "chart";
    const max = Math.max(...ids.map((id) => ds.r[id][m.key]));
    fig.innerHTML = `<figcaption><strong>${m.label}</strong><span>${m.unit === "MB" ? "MB" : "seconds"}</span></figcaption>`;
    const bars = document.createElement("div");
    bars.className = "bars";
    bars.setAttribute("role", "list");
    ids.forEach((id, i) => {
      const t = TOOL[id], v = ds.r[id][m.key];
      const row = document.createElement("div");
      row.className = `bar-row${t.us ? " us" : ""}`;
      row.style.setProperty("--i", i);
      row.setAttribute("role", "listitem");
      row.tabIndex = 0;
      row.setAttribute("aria-label", `${t.name}: ${m.label} ${fmt(v, m.unit)}`);
      row.innerHTML = `<span class="bar-name">${t.name}${t.sep && m.key ? "<sup>†</sup>" : ""}</span>` +
        `<span class="bar-track"><span class="bar" style="--w:${v / max}"></span>` +
        `<span class="bar-val">${fmt(v, m.unit)}</span></span>`;
      bindTip(row, () => toolTip(t, ds));
      bars.appendChild(row);
    });
    fig.appendChild(bars);
    host.appendChild(fig);
  }
  grow(host);
}

// Bars grow when their chart scrolls into view.
const growObserver = "IntersectionObserver" in window && new IntersectionObserver((entries) => {
  for (const e of entries) if (e.isIntersecting) { e.target.classList.add("grown"); growObserver.unobserve(e.target); }
}, { threshold: 0.25 });
function grow(host) {
  host.querySelectorAll(".chart").forEach((c) => (growObserver ? growObserver.observe(c) : c.classList.add("grown")));
}

function renderTable(tbody, ds, ids) {
  tbody.innerHTML = ids.map((id) => {
    const t = TOOL[id], [s, p, u] = ds.r[id];
    return `<tr class="${t.us ? "us" : ""}"><td>${t.name}${t.sep ? " †" : ""}</td><td>${fmt(s, "MB")}</td>` +
      `<td>${pct(s, ds.mb)}</td><td>${fmt(p, "s")}</td><td>${fmt(u, "s")}</td></tr>`;
  }).join("");
}

// ---------------------------------------------------------------- comparison

const MARK = {
  y: ["yes", "Yes", '<svg viewBox="0 0 20 20"><path d="M5.5 10.5l3 3 6-7"/></svg>'],
  p: ["part", "Partial", '<svg viewBox="0 0 20 20"><path d="M6 10h8"/></svg>'],
  n: ["no", "No", ""],
  plan: ["plan", "Planned for 1.0", '<svg viewBox="0 0 20 20"><path d="M10 6v4.5l3 1.8"/></svg>'],
};

function mark(code, label) {
  const [cls, text, svg] = MARK[code];
  return `<span class="mk mk-${cls}" title="${label ? label + ": " : ""}${text}"><span class="sr">${label ? label + ": " : ""}${text}</span>${svg}</span>`;
}

function renderCompare(host, { groups, limit } = {}) {
  const apps = Z.apps;
  let html = `<table class="cmp"><thead><tr><th scope="col"><span class="sr">Feature</span></th>` +
    apps.map((a, i) => `<th scope="col" class="${i === 0 ? "us" : ""}">${i === 0 ? '<img src="assets/icon-512.png" alt="" width="28" height="28">' : ""}${a}</th>`).join("") +
    `</tr></thead>`;
  for (const g of Z.compare) {
    if (groups && !groups.includes(g.group)) continue;
    const rows = limit && limit[g.group] ? g.rows.slice(0, limit[g.group]) : g.rows;
    html += `<tbody><tr class="cmp-group"><th colspan="${apps.length + 1}" scope="colgroup">${g.group}` +
      (g.formats ? `<span>Open · Create</span>` : "") + `</th></tr>`;
    for (const [k, r] of rows.entries()) {
      html += `<tr style="--r:${k}"><th scope="row">${r.name}${r.note ? `<small>${r.note}</small>` : ""}</th>` +
        r.cells.map((c, i) => `<td class="${i === 0 ? "us" : ""}">${g.formats
          ? `<span class="pair">${mark(c[0], "Open")}${mark(c[1], "Create")}</span>`
          : mark(c)}</td>`).join("") + `</tr>`;
    }
    html += `</tbody>`;
  }
  host.innerHTML = html + `</table>`;
  if (growObserver) {
    const io = new IntersectionObserver((entries) => entries.forEach((e) => {
      if (e.isIntersecting) { e.target.classList.add("popped"); io.unobserve(e.target); }
    }), { threshold: 0.15 });
    host.querySelectorAll("tbody").forEach((b) => io.observe(b));
  } else {
    host.querySelectorAll("tbody").forEach((b) => b.classList.add("popped"));
  }
}

// ---------------------------------------------------------------- pages

function homePage() {
  const host = document.getElementById("home-charts");
  if (!host) return;
  const ds = Z.datasets[0];
  renderCharts(host, ds, ["finder", "7z", "fast", "balanced", "max"]);
  renderTable(document.getElementById("bench-rows"), ds, ["finder", "7z", "fast", "balanced", "max"]);
}

function benchmarksPage() {
  const host = document.getElementById("bench-charts");
  if (!host) return;
  const ids = Z.tools.map((t) => t.id);
  const tabs = document.getElementById("ds-tabs");
  const info = document.getElementById("ds-info");
  const tbody = document.getElementById("ds-rows");

  function select(i) {
    const ds = Z.datasets[i];
    tabs.querySelectorAll("button").forEach((b, j) => b.setAttribute("aria-selected", String(i === j)));
    info.innerHTML = `<strong>${ds.name}</strong> · ${ds.mb} MB · ${ds.what}. <span>${ds.why}.</span>`;
    renderCharts(host, ds, ids);
    renderTable(tbody, ds, ids);
  }
  tabs.innerHTML = Z.datasets.map((d, i) =>
    `<button type="button" role="tab" aria-selected="false" data-i="${i}">${d.name}</button>`).join("");
  tabs.addEventListener("click", (e) => { const b = e.target.closest("button"); if (b) select(+b.dataset.i); });
  tabs.addEventListener("keydown", (e) => {
    const cur = [...tabs.children].findIndex((b) => b.getAttribute("aria-selected") === "true");
    const step = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
    if (!step) return;
    const next = (cur + step + Z.datasets.length) % Z.datasets.length;
    select(next);
    tabs.children[next].focus();
  });
  select(0);

  // Summary: Zipling against 7-Zip Ultra on every dataset.
  const sum = document.getElementById("vs-rows");
  sum.innerHTML = Z.datasets.map((ds) => {
    const z = ds.r["7z"];
    const cell = (id) => {
      const d = ((ds.r[id][0] / z[0]) - 1) * 100;
      const cls = d < -0.5 ? "better" : d > 0.5 ? "worse" : "even";
      const txt = Math.abs(d) < 0.5 ? "same" : `${d > 0 ? "+" : "−"}${Math.abs(d).toFixed(0)}%`;
      return `<td><span class="delta ${cls}">${txt}</span></td>`;
    };
    const speed = (i) => {
      const r = z[i] / ds.r.balanced[i];
      return `<td><span class="delta ${r >= 1.05 ? "better" : r <= 0.95 ? "worse" : "even"}">${r >= 1 ? r.toFixed(1) + "× faster" : (1 / r).toFixed(1) + "× slower"}</span></td>`;
    };
    return `<tr><th scope="row">${ds.name}</th>${cell("balanced")}${cell("max")}${speed(1)}${speed(2)}</tr>`;
  }).join("");
}

function comparePage() {
  const full = document.getElementById("compare-full");
  if (full) renderCompare(full);
  const teaser = document.getElementById("compare-teaser");
  if (teaser) renderCompare(teaser, { groups: ["Compression", "Everyday use"], limit: { "Everyday use": 4 } });
}

// ---------------------------------------------------------------- motion

function reveals() {
  // Children of [data-stagger] reveal one after another.
  document.querySelectorAll("[data-stagger]").forEach((p) =>
    [...p.children].forEach((c, i) => { c.classList.add("reveal"); c.style.setProperty("--d", `${i * 70}ms`); }));
  const els = document.querySelectorAll(".reveal");
  if (!("IntersectionObserver" in window)) { els.forEach((el) => el.classList.add("in")); return; }
  const io = new IntersectionObserver((entries) => entries.forEach((e) => {
    if (e.isIntersecting) { e.target.classList.add("in"); io.unobserve(e.target); }
  }), { threshold: 0.12, rootMargin: "0px 0px -40px 0px" });
  els.forEach((el) => io.observe(el));
}

// Headline numbers count up once, the first time they're seen.
function countUps() {
  const els = document.querySelectorAll("[data-count]");
  const run = (el) => {
    const end = parseFloat(el.dataset.count), dec = (el.dataset.count.split(".")[1] || "").length;
    if (reduceMotion) { el.textContent = end.toFixed(dec); return; }
    const t0 = performance.now(), dur = 1100;
    const step = (t) => {
      const k = Math.min(1, (t - t0) / dur), e = 1 - Math.pow(1 - k, 3);
      el.textContent = (end * e).toFixed(dec);
      if (k < 1) requestAnimationFrame(step);
    };
    requestAnimationFrame(step);
  };
  if (!("IntersectionObserver" in window)) { els.forEach(run); return; }
  const io = new IntersectionObserver((entries) => entries.forEach((e) => {
    if (e.isIntersecting) { run(e.target); io.unobserve(e.target); }
  }), { threshold: 0.6 });
  els.forEach((el) => io.observe(el));
}

// The hero reel: scenes advance on a clock that stops while the reel is hovered,
// off screen, in a hidden tab or paused. CSS plays each scene's motion; numbers
// marked data-to count here, in step with the same clock.
function reel() {
  const root = document.querySelector(".reel");
  if (!root) return;
  const scenes = [...root.querySelectorAll(".scene")];
  const steps = root.querySelector(".reel-steps");
  const toggle = root.querySelector(".reel-toggle");
  const durations = [5200, 5800, 5800, 5000, 5400, 5400, 5200];

  // Repair: an archive's blocks, a few of them damaged, and its recovery data.
  const damaged = new Set([8, 9, 21, 33, 34, 50, 63]);
  const grid = root.querySelector(".vis-repair .grid");
  if (grid) grid.innerHTML = Array.from({ length: 72 }, (_, i) =>
    `<i class="${damaged.has(i) ? "x" : ""}" style="--d:${0.1 + i * 0.008}s;--l:${56 + ((i * 37) % 13)}%"></i>`).join("");
  const parity = root.querySelector(".vis-repair .parity");
  if (parity) parity.innerHTML = Array.from({ length: 12 }, (_, i) => `<i style="--d:${0.45 + i * 0.02}s"></i>`).join("");

  steps.innerHTML = scenes.map((s, i) =>
    `<button type="button" class="step" aria-label="${i + 1} of ${scenes.length}: ${s.dataset.name}">` +
    `<span class="label">${s.dataset.name}</span><span class="meter"><i></i></span></button>`).join("");
  const buttons = [...steps.children];

  const fit = () => {
    root.querySelectorAll(".scene-vis").forEach((h) => h.style.setProperty("--k", h.clientWidth / 560));
    root.querySelectorAll(".typed").forEach((t) => t.style.setProperty("--tw", `${t.scrollWidth}px`));
  };
  fit();
  if ("ResizeObserver" in window) new ResizeObserver(fit).observe(root);

  const live = !reduceMotion;
  root.classList.toggle("live", live);
  let current = 0, t = 0, last = 0;
  let stopped = !live, hovered = false, offscreen = false;

  const counters = (scene, at) => scene.querySelectorAll("[data-to]").forEach((el) => {
    const to = parseFloat(el.dataset.to), from = parseFloat(el.dataset.from || 0), dec = +el.dataset.dec || 0;
    const k = live ? Math.min(1, Math.max(0, (at - (+el.dataset.at || 0)) / (+el.dataset.dur || 1000))) : 1;
    el.textContent = (from + (to - from) * (1 - Math.pow(1 - k, 3))).toFixed(dec);
  });

  function show(n) {
    current = (n + scenes.length) % scenes.length;
    t = 0;
    scenes.forEach((s, i) => {
      const on = i === current;
      s.classList.remove("on");
      s.inert = !on;
      s.setAttribute("aria-hidden", String(!on));
      s.setAttribute("aria-label", `${i + 1} of ${scenes.length}: ${s.dataset.name}`);
    });
    void scenes[current].offsetWidth; // restart its motion, even when it's the same scene
    scenes[current].classList.add("on");
    buttons.forEach((b, i) => {
      b.setAttribute("aria-current", String(i === current));
      b.querySelector("i").style.setProperty("--p", i < current || (!live && i === current) ? 1 : 0);
    });
    counters(scenes[current], 0);
  }

  function state() {
    root.classList.toggle("paused", stopped || hovered || offscreen);
    root.classList.toggle("stopped", stopped);
    toggle.setAttribute("aria-label", stopped ? "Play the tour" : "Pause the tour");
  }

  function frame(now) {
    const dt = Math.min(100, now - (last || now));
    last = now;
    if (!root.classList.contains("paused")) {
      t += dt;
      counters(scenes[current], t);
      buttons[current].querySelector("i").style.setProperty("--p", Math.min(1, t / durations[current]));
      if (t >= durations[current]) show(current + 1);
    }
    requestAnimationFrame(frame);
  }

  buttons.forEach((b, i) => b.addEventListener("click", () => show(i)));
  steps.addEventListener("keydown", (e) => {
    const step = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
    if (!step) return;
    show(current + step);
    buttons[current].focus();
  });
  show(0);
  state();
  if (!live) return;

  toggle.hidden = false;
  toggle.addEventListener("click", () => { stopped = !stopped; state(); });
  if (matchMedia("(hover: hover)").matches) {
    const stage = root.querySelector(".reel-stage");
    stage.addEventListener("pointerenter", () => { hovered = true; state(); });
    stage.addEventListener("pointerleave", () => { hovered = false; state(); });
  }
  if ("IntersectionObserver" in window) {
    new IntersectionObserver(([e]) => { offscreen = !e.isIntersecting; state(); }, { threshold: 0.2 }).observe(root);
  }
  requestAnimationFrame(frame);
}

function chrome() {
  const bar = document.querySelector(".nav");
  const progress = document.querySelector(".progress");
  const update = () => {
    bar.classList.toggle("scrolled", scrollY > 8);
    if (progress) {
      const max = document.documentElement.scrollHeight - innerHeight;
      progress.style.transform = `scaleX(${max > 0 ? scrollY / max : 0})`;
    }
  };
  addEventListener("scroll", update, { passive: true });
  update();
}

homePage();
benchmarksPage();
comparePage();
reveals();
countUps();
reel();
chrome();
