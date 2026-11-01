// Inside Arca: the scripts behind the engine page. A byte field on a canvas, the
// strongbox following the pointer, the scenes built from data (coefficient block,
// token tapes, hex rows, the sorting tiles), and the on-screen switch for every loop.
(() => {
  const reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;
  const $ = (s, r = document) => r.querySelector(s);
  const $$ = (s, r = document) => [...r.querySelectorAll(s)];

  // Whatever is already in view reveals at once; the observer in main.js takes the rest.
  const unveil = () => $$(".reveal:not(.in)").forEach((el) => {
    const r = el.getBoundingClientRect();
    if (r.top < innerHeight && r.bottom > 0) el.classList.add("in");
  });
  requestAnimationFrame(unveil);
  addEventListener("load", () => setTimeout(unveil, 300));

  // Loops run only while their scene is on screen.
  const live = $$("[data-live]");
  if ("IntersectionObserver" in window) {
    const io = new IntersectionObserver((es) => es.forEach((e) => e.target.classList.toggle("live", e.isIntersecting)), { threshold: 0.25 });
    live.forEach((el) => io.observe(el));
  } else live.forEach((el) => el.classList.add("live"));

  // The field: bytes drifting toward the viewer.
  const cv = $(".stars");
  if (cv) {
    const ctx = cv.getContext("2d");
    const glyphs = "0123456789ABCDEF";
    let w = 0, h = 0, dpr = 1, pts = [];
    const size = () => {
      dpr = Math.min(2, devicePixelRatio || 1);
      w = cv.width = innerWidth * dpr; h = cv.height = innerHeight * dpr;
      cv.style.width = innerWidth + "px"; cv.style.height = innerHeight + "px";
      if (!pts.length) for (let i = 0; i < 160; i++) pts.push(spawn(true));
    };
    const spawn = (any) => ({ x: (Math.random() - 0.5) * 2, y: (Math.random() - 0.5) * 2, z: any ? Math.random() : 1, g: glyphs[Math.floor(Math.random() * 16)], t: Math.random() < 0.3 });
    let last = 0, hidden = false;
    const frame = (now) => {
      if (!hidden) {
        const dt = Math.min(0.05, (now - last) / 1000); last = now;
        ctx.clearRect(0, 0, w, h);
        const cx = w / 2, cy = h * 0.3, f = w * 0.45;
        for (const p of pts) {
          p.z -= dt * 0.06;
          if (p.z <= 0.02) Object.assign(p, spawn(false));
          const sx = cx + (p.x * f) / p.z, sy = cy + (p.y * f) / p.z;
          if (sx < -40 || sx > w + 40 || sy < -40 || sy > h + 40) { Object.assign(p, spawn(false)); continue; }
          const a = Math.max(0, Math.min(1, (1 - p.z) * 0.9)) * 0.55;
          if (p.t) {
            ctx.fillStyle = `rgba(127, 231, 255, ${a})`;
            ctx.font = `${Math.round(9 * dpr + (1 - p.z) * 8 * dpr)}px ui-monospace, Menlo, monospace`;
            ctx.fillText(p.g, sx, sy);
          } else {
            ctx.fillStyle = `rgba(135, 148, 255, ${a})`;
            const r = (0.6 + (1 - p.z) * 1.6) * dpr;
            ctx.fillRect(sx, sy, r, r);
          }
        }
      }
      if (!reduce) requestAnimationFrame(frame);
    };
    size();
    addEventListener("resize", size);
    document.addEventListener("visibilitychange", () => { hidden = document.hidden; });
    if (reduce) { pts.forEach((p) => (p.z = 0.3 + Math.random() * 0.6)); frame(0); } else requestAnimationFrame(frame);
  }

  // The strongbox leans toward the pointer.
  const hero = $(".a-hero"), rig = $(".rig");
  if (hero && rig && !reduce) {
    hero.addEventListener("pointermove", (e) => {
      if (e.pointerType === "touch") return;
      const r = hero.getBoundingClientRect();
      const px = (e.clientX - r.left) / r.width - 0.5, py = (e.clientY - r.top) / r.height - 0.5;
      rig.style.setProperty("--ry", `${24 + px * 30}deg`);
      rig.style.setProperty("--rx", `${-16 - py * 24}deg`);
    });
    hero.addEventListener("pointerleave", () => { rig.style.removeProperty("--ry"); rig.style.removeProperty("--rx"); });
  }

  // The pipeline board scales to its container.
  const board = $(".board");
  if (board) {
    const fit = () => board.style.setProperty("--k", Math.min(1, board.clientWidth / 1120).toFixed(3));
    fit();
    if ("ResizeObserver" in window) new ResizeObserver(fit).observe(board); else addEventListener("resize", fit);
  }

  // 1. The coefficient block: big values near the top-left corner, small ones elsewhere.
  const grid = $(".dct-grid");
  if (grid) {
    const frag = document.createDocumentFragment();
    for (let i = 0; i < 64; i++) {
      const r = Math.floor(i / 8), c = i % 8;
      const noise = ((r * 7 + c * 13) % 5) / 5;
      const mag = Math.exp(-(r + c) / 2.1) * (0.75 + noise * 0.5);
      const cell = document.createElement("i");
      cell.style.setProperty("--i", i);
      cell.style.setProperty("--h", `${Math.round(6 + mag * 76)}px`);
      cell.style.setProperty("--c", `rgba(${Math.round(135 - mag * 10)}, ${Math.round(148 + mag * 83)}, 255, ${(0.25 + mag * 0.7).toFixed(2)})`);
      frag.appendChild(cell);
    }
    grid.prepend(frag);
  }

  // 2. Token tapes: the same stream twice; the second one marks where lzfse 1.0 flushed early.
  const tokens = [["L", 12], ["M", "3·40"], ["L", 4], ["M", "7·512"], ["L", 315], ["L", 315], ["M", "5·18"], ["L", 9], ["M", "4·96"], ["L", 2], ["M", "11·1k"], ["L", 61], ["L", 315], ["M", "3·24"], ["L", 7], ["M", "6·233"]];
  const flushAt = new Set([5, 12]);
  $$(".trow .tape").forEach((tape) => {
    const marks = tape.dataset.mark === "x";
    const html = tokens.concat(tokens).map(([k, v], i) => `<span class="tok${k === "M" ? " m" : ""}${marks && flushAt.has(i % tokens.length) ? " x" : ""}">${k} ${v}</span>`).join("");
    tape.innerHTML = html;
  });

  // 6. Hex rows: the same 22 bytes before and after the round trip; one loop shows a bad byte.
  const hex = (seed) => { let s = seed; return () => { s = (s * 1103515245 + 12345) & 0x7fffffff; return (s >> 8 & 255).toString(16).padStart(2, "0"); }; };
  $$(".hexrow .cells").forEach((cells) => {
    const next = hex(7);
    cells.innerHTML = Array.from({ length: 22 }, (_, i) => `<i style="--i:${i}"${i === 13 && cells.closest(".hexrow").classList.contains("out") ? ' class="bad"' : ""}>${next()}</i>`).join("");
  });

  // 5. The sorting tiles: a real Burrows–Wheeler transform of a short line.
  const bwt = $(".bwt");
  if (bwt) {
    const text = "the_archive_in_the_archive";
    const n = text.length, step = 26;
    const rot = Array.from({ length: n }, (_, i) => i).sort((a, b) => (text.slice(a) + text.slice(0, a)).localeCompare(text.slice(b) + text.slice(0, b)));
    // Output position p holds the character before rotation rot[p]: original index (rot[p] + n - 1) % n.
    const dest = new Array(n);
    rot.forEach((start, p) => { dest[(start + n - 1) % n] = p; });
    const out = rot.map((start) => text[(start + n - 1) % n]).join("");
    const palette = { t: "#7FE7FF", h: "#8794FF", e: "#6EE7A8", _: "#C4C6D2", a: "#FFB066", r: "#F08C9A", c: "#B58CFF", i: "#7CC4FF", v: "#FFD36E", n: "#9AD6B0" };
    const tiles = $(".tiles", bwt), cap = $(".caption", bwt);
    tiles.style.width = `${n * step}px`;
    const fitTiles = () => { tiles.style.scale = Math.min(1, (bwt.clientWidth - 36) / (n * step)).toFixed(3); };
    fitTiles();
    if ("ResizeObserver" in window) new ResizeObserver(fitTiles).observe(bwt); else addEventListener("resize", fitTiles);
    const els = [...text].map((ch, i) => {
      const t = document.createElement("span");
      t.className = "tile"; t.textContent = ch === "_" ? "·" : ch;
      t.style.translate = `${i * step}px 0`;
      t.style.setProperty("--rc", palette[ch] || "#C4C6D2");
      tiles.appendChild(t);
      return t;
    });
    const states = [
      () => { bwt.classList.remove("runs"); els.forEach((t, i) => (t.style.translate = `${i * step}px 0`)); cap.innerHTML = `<b>input</b> · ${text.replace(/_/g, "·")}`; },
      () => { els.forEach((t, i) => (t.style.translate = `${dest[i] * step}px 0`)); cap.innerHTML = `<b>after the transform</b> · ${out.replace(/_/g, "·")}`; },
      () => { bwt.classList.add("runs"); cap.innerHTML = `<b>runs</b> · the same letters sit together and code in a few bits each`; },
    ];
    let k = 0; states[0]();
    if (!reduce) setInterval(() => { k = (k + 1) % states.length; states[k](); }, 3000);
  }
})();
