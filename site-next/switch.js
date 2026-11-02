// Prototype switcher: a thin bar above the page to flip between the two
// variants and between dark and light. Not part of the site; it exists so
// the drafts can be compared side by side.
(() => {
  const KEY = "zipling-proto-theme";
  const html = document.documentElement;
  const variant = document.body.dataset.variant || "a";
  const preferred = () => matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
  let theme = localStorage.getItem(KEY) || html.dataset.theme || preferred();
  const apply = t => { theme = t; html.dataset.theme = t; localStorage.setItem(KEY, t); update(); };

  const css = `
    .proto { position: fixed; top: 0; left: 0; right: 0; z-index: 100; height: 38px; display: flex; align-items: center; justify-content: center; gap: 18px;
      font: 500 12.5px/1 -apple-system, BlinkMacSystemFont, "SF Pro Text", Inter, sans-serif; color: #D9DBE6; background: #0B0C14; border-bottom: 1px solid rgba(255,255,255,.08); }
    .proto span.lbl { color: #8A8EA3; }
    .proto .seg { display: inline-flex; padding: 3px; border-radius: 999px; background: rgba(255,255,255,.07); }
    .proto .seg a, .proto .seg button { appearance: none; border: 0; background: transparent; color: #B9BCCB; font: inherit; padding: 5px 11px; border-radius: 999px; cursor: pointer; text-decoration: none; }
    .proto .seg .on { background: #FFFFFF; color: #0B0C14; }
    .proto .note { color: #6F7388; }
    @media (max-width: 600px) { .proto { gap: 10px; } .proto .note { display: none; } }
  `;
  const style = document.createElement("style"); style.textContent = css; document.head.appendChild(style);

  const bar = document.createElement("div");
  bar.className = "proto";
  bar.setAttribute("role", "toolbar");
  bar.setAttribute("aria-label", "Prototype controls");
  bar.innerHTML = `
    <span class="lbl">Prototype</span>
    <span class="seg" id="proto-variant">
      <a href="index.html" data-v="a">A · Bold</a>
      <a href="b.html" data-v="b">B · Fluid</a>
    </span>
    <span class="lbl">Theme</span>
    <span class="seg" id="proto-theme">
      <button type="button" data-t="dark">Dark</button>
      <button type="button" data-t="light">Light</button>
    </span>
    <span class="note">Local draft · nothing here is published</span>`;
  document.body.prepend(bar);
  bar.querySelectorAll("[data-t]").forEach(b => b.addEventListener("click", () => apply(b.dataset.t)));

  function update() {
    bar.querySelectorAll("[data-v]").forEach(a => a.classList.toggle("on", a.dataset.v === variant));
    bar.querySelectorAll("[data-t]").forEach(b => b.classList.toggle("on", b.dataset.t === theme));
  }
  apply(theme);
  html.classList.add("has-proto");
})();
