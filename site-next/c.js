// Variant C: the feature tabs play as a carousel. Each panel runs its scene for
// its own length, then the next tab takes over; a pointer over the stage holds
// it, a click on a tab goes there and keeps the carousel going from it.
(() => {
  const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;
  document.querySelectorAll("[data-ctabs]").forEach(tabs => {
    const buttons = [...tabs.querySelectorAll(".ctab")];
    const stage = document.querySelector(`[data-cpanels="${tabs.dataset.ctabs}"]`);
    const panels = [...stage.querySelectorAll(".cpanel")];
    const sel = document.createElement("span"); sel.className = "pill sel";
    const hov = document.createElement("span"); hov.className = "pill hov";
    tabs.prepend(hov); tabs.prepend(sel);
    const place = (pill, el) => { pill.style.width = el.offsetWidth + "px"; pill.style.height = el.offsetHeight + "px"; pill.style.transform = `translate(${el.offsetLeft}px, ${el.offsetTop}px)`; };
    let current = Math.max(0, buttons.findIndex(b => b.classList.contains("on")));
    let timer = null, visible = false, held = false;

    const arm = () => {
      clearTimeout(timer);
      const prog = buttons[current].querySelector(".prog");
      if (prog) { prog.classList.remove("run"); void prog.offsetWidth; }
      if (reduced || !visible || held) return;
      const dur = parseInt(panels[current].dataset.dur || "9000", 10);
      buttons[current].style.setProperty("--dur", dur + "ms");
      if (prog) prog.classList.add("run");
      timer = setTimeout(() => select((current + 1) % buttons.length), dur);
    };
    const select = i => {
      current = i;
      buttons.forEach((b, j) => { b.classList.toggle("on", j === i); b.setAttribute("aria-selected", j === i); b.tabIndex = j === i ? 0 : -1; });
      panels.forEach((p, j) => {
        p.classList.toggle("on", j === i);
        p.classList.remove("in");
        if (j === i) { void p.offsetWidth; p.classList.add("in"); }
      });
      place(sel, buttons[i]);
      arm();
    };
    buttons.forEach((b, i) => {
      b.addEventListener("click", () => select(i));
      b.addEventListener("pointerenter", () => { place(hov, b); hov.style.opacity = i === current ? "0" : "1"; });
      b.addEventListener("keydown", e => {
        const n = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
        if (!n) return;
        e.preventDefault();
        const j = (current + n + buttons.length) % buttons.length;
        select(j); buttons[j].focus();
      });
    });
    tabs.addEventListener("pointerleave", () => { hov.style.opacity = "0"; });
    // A pointer over the stage holds the carousel where it is.
    for (const el of [stage, tabs]) {
      el.addEventListener("pointerenter", () => { held = true; tabs.classList.add("paused"); clearTimeout(timer); });
      el.addEventListener("pointerleave", () => { held = false; tabs.classList.remove("paused"); arm(); });
    }
    // It only runs while on screen.
    new IntersectionObserver(entries => {
      for (const e of entries) { visible = e.isIntersecting; if (visible) select(current); else { clearTimeout(timer); panels[current].classList.remove("in"); } }
    }, { threshold: 0.35 }).observe(stage);
    sel.style.transition = "none"; select(current); void sel.offsetWidth; sel.style.transition = "";
    addEventListener("resize", () => place(sel, buttons[current]));
  });
})();
