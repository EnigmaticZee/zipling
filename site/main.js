// Zipling landing page: benchmark charts, hover tooltips and scroll reveals.

// Mixed folder (736 MB), from docs/BENCHMARKS.md. Finder was measured in a separate session.
const ORIGINAL_MB = 736;
const BENCH = [
  { name: "Finder Compress", detail: "ZIP, like Archive Utility and MacPacker", us: false, size: 322.7, pack: 19.8, unpack: 9.32 },
  { name: "7-Zip Ultra", detail: "7z -mx9, the engine behind Keka and BetterZip", us: false, size: 233.4, pack: 49.3, unpack: 3.84 },
  { name: "Zipling Fast", detail: "Fastest setting", us: true, size: 286.8, pack: 1.4, unpack: 0.88 },
  { name: "Zipling Default", detail: "Balanced, the default", us: true, size: 221.5, pack: 9.7, unpack: 2.30 },
  { name: "Zipling Max", detail: "Smallest archives", us: true, size: 192.8, pack: 51.6, unpack: 3.50 },
];
const LABELS = { size: "Archive size", pack: "Compress", unpack: "Extract" };

function fmt(v, unit) {
  if (unit === "MB") return `${v.toFixed(1)} MB`;
  return `${v < 10 ? v.toFixed(1) : Math.round(v)} s`;
}

const tip = document.querySelector(".tooltip");

function showTip(row, x, y) {
  const r = BENCH[row.dataset.index];
  tip.innerHTML = `<b>${r.name}</b><span>${r.detail}</span><br>` +
    `${fmt(r.size, "MB")} · ${Math.round((r.size / ORIGINAL_MB) * 100)}% of original<br>` +
    `Compress ${fmt(r.pack, "s")} · Extract ${fmt(r.unpack, "s")}`;
  tip.hidden = false;
  const w = tip.offsetWidth, h = tip.offsetHeight;
  tip.style.left = `${Math.min(Math.max(8, x + 14), innerWidth - w - 8)}px`;
  tip.style.top = `${Math.min(Math.max(8, y - h - 12), innerHeight - h - 8)}px`;
}

function buildCharts() {
  document.querySelectorAll(".chart").forEach((chart) => {
    const metric = chart.dataset.metric, unit = chart.dataset.unit;
    const max = Math.max(...BENCH.map((r) => r[metric]));
    const bars = chart.querySelector(".bars");
    BENCH.forEach((r, i) => {
      const row = document.createElement("div");
      row.className = `bar-row${r.us ? " us" : ""}`;
      row.setAttribute("role", "listitem");
      row.tabIndex = 0;
      row.dataset.index = i;
      row.setAttribute("aria-label", `${r.name}: ${LABELS[metric]} ${fmt(r[metric], unit)}`);
      row.innerHTML = `<span class="bar-name">${r.name}</span>` +
        `<span class="bar-track"><span class="bar" data-w="${(r[metric] / max) * 100}"></span>` +
        `<span class="bar-val">${fmt(r[metric], unit)}</span></span>`;
      row.addEventListener("pointermove", (e) => showTip(row, e.clientX, e.clientY));
      row.addEventListener("pointerleave", () => { tip.hidden = true; });
      row.addEventListener("focus", () => { const b = row.getBoundingClientRect(); showTip(row, b.left + 40, b.top); });
      row.addEventListener("blur", () => { tip.hidden = true; });
      bars.appendChild(row);
    });
  });
  // Bars leave room for their value label.
  const grow = (chart) => chart.querySelectorAll(".bar").forEach((b) => { b.style.width = `calc((100% - 74px) * ${b.dataset.w / 100})`; });
  const charts = document.querySelectorAll(".chart");
  if ("IntersectionObserver" in window) {
    const io = new IntersectionObserver((entries) => entries.forEach((e) => {
      if (e.isIntersecting) { grow(e.target); io.unobserve(e.target); }
    }), { threshold: 0.3 });
    charts.forEach((c) => io.observe(c));
  } else {
    charts.forEach(grow);
  }

  const tbody = document.getElementById("bench-rows");
  BENCH.forEach((r) => {
    const tr = document.createElement("tr");
    if (r.us) tr.className = "us";
    tr.innerHTML = `<td>${r.name}</td><td>${fmt(r.size, "MB")}</td><td>${((r.size / ORIGINAL_MB) * 100).toFixed(1)}%</td>` +
      `<td>${fmt(r.pack, "s")}</td><td>${fmt(r.unpack, "s")}</td>`;
    tbody.appendChild(tr);
  });
}

function reveals() {
  const els = document.querySelectorAll(".reveal");
  if (!("IntersectionObserver" in window)) { els.forEach((el) => el.classList.add("in")); return; }
  const io = new IntersectionObserver((entries) => entries.forEach((e) => {
    if (e.isIntersecting) { e.target.classList.add("in"); io.unobserve(e.target); }
  }), { threshold: 0.12, rootMargin: "0px 0px -40px 0px" });
  els.forEach((el) => io.observe(el));
}

function nav() {
  const bar = document.querySelector(".nav");
  const update = () => bar.classList.toggle("scrolled", scrollY > 8);
  addEventListener("scroll", update, { passive: true });
  update();
}

buildCharts();
reveals();
nav();
