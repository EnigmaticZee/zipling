// Variant C: the feature tabs. Everything else comes from next.js.
(() => {
  document.querySelectorAll("[data-ctabs]").forEach(tabs => {
    const buttons = [...tabs.querySelectorAll(".ctab")];
    const panels = [...document.querySelectorAll(`[data-cpanels="${tabs.dataset.ctabs}"] .cpanel`)];
    const sel = document.createElement("span"); sel.className = "pill sel";
    const hov = document.createElement("span"); hov.className = "pill hov";
    tabs.prepend(hov); tabs.prepend(sel);
    const place = (pill, el) => { pill.style.width = el.offsetWidth + "px"; pill.style.height = el.offsetHeight + "px"; pill.style.transform = `translate(${el.offsetLeft}px, ${el.offsetTop}px)`; };
    let current = Math.max(0, buttons.findIndex(b => b.classList.contains("on")));
    const select = i => {
      current = i;
      buttons.forEach((b, j) => { b.classList.toggle("on", j === i); b.setAttribute("aria-selected", j === i); b.tabIndex = j === i ? 0 : -1; });
      panels.forEach((p, j) => { p.classList.toggle("on", j === i); if (j === i) p.classList.add("in"); });
      place(sel, buttons[i]);
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
    sel.style.transition = "none"; select(current); void sel.offsetWidth; sel.style.transition = "";
    addEventListener("resize", () => place(sel, buttons[current]));
  });
})();
