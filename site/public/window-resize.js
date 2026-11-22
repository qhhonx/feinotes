// Drag a demo window's edges to resize it, like a macOS window: the sides
// change the width (both at once, the window stays centred), the bottom the
// height. Double-click a handle to restore the size.
// The app inside lays itself out again as the window changes.
//
// So that people find out they can: handles sit just outside the edges, the
// window narrows and springs back once when it first comes into view, and a
// hint under it stays until someone has resized a window.
(() => {
"use strict";
const MIN_W = 560, MIN_H = 420, MAX_H = 1100;
const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;
const HINT = {
  "zh-CN": "拖动窗口两侧或底部，看看笔记在不同尺寸下的样子",
  "zh-Hant": "拖動視窗兩側或底部，看看筆記在不同尺寸下的樣子",
  en: "Drag the sides or bottom of the window to see the notes at other sizes",
  ja: "ウィンドウの左右や下端をドラッグして、いろいろなサイズで見てみてください",
  fr: "Faites glisser les côtés ou le bas de la fenêtre pour voir les notes à d’autres tailles",
};
const style = document.createElement("style");
style.textContent = `
.resizable { position: relative; width: min(1180px, 100%); max-width: 100%; }
.resizable > .window { width: 100% !important; }
.handle { position: absolute; z-index: 5; display: grid; place-items: center; touch-action: none; }
.handle::before { content: ""; border-radius: 999px; background: rgba(255, 255, 255, .92);
  box-shadow: 0 0 0 .5px rgba(0, 0, 0, .25), 0 2px 8px rgba(0, 0, 0, .28); transition: background .15s, transform .15s; }
.handle:hover::before, .handle.active::before { background: #fff; transform: scale(1.15); }
.handle-left, .handle-right { top: 50%; width: 24px; height: 96px; margin-top: -48px; cursor: ew-resize; }
.handle-left { left: -22px; } .handle-right { right: -22px; }
.handle-left::before, .handle-right::before { width: 6px; height: 56px; }
.handle-bottom { left: 50%; bottom: -22px; width: 96px; height: 24px; margin-left: -48px; cursor: ns-resize; }
.handle-bottom::before { width: 56px; height: 6px; }
.resize-hint { display: inline-flex; align-items: center; gap: 8px; margin-top: 34px; padding: 7px 14px; border-radius: 999px; font-size: 13px; font-weight: 550; color: #1d211f;
  background: rgba(255, 255, 255, .72); border: 1px solid rgba(255, 255, 255, .8); backdrop-filter: blur(14px) saturate(1.4); -webkit-backdrop-filter: blur(14px) saturate(1.4); transition: opacity .4s; }
.resize-hint svg { width: 16px; height: 16px; flex: none; }
.resize-hint.gone { opacity: 0; pointer-events: none; }
.breathing .handle::before { animation: nudge 1.4s ease-in-out 2; }
@keyframes nudge { 50% { transform: scale(1.35); background: #fff; } }
html.resizing, html.resizing * { user-select: none !important; -webkit-user-select: none !important; }
@media (max-width: 640px) { .handle, .resize-hint { display: none; } .resizable { width: 100%; } }`;
document.head.append(style);

let resized = false;
try { resized = localStorage.getItem("demo-resized") === "1"; } catch {}
const hints = [];
function done() {
  if (resized) return;
  resized = true;
  hints.forEach(h => h.classList.add("gone"));
  try { localStorage.setItem("demo-resized", "1"); } catch {}
}

for (const w of document.querySelectorAll(".window[data-live]")) {
  w.querySelectorAll(".grip").forEach(g => g.remove());
  const box = document.createElement("div");
  box.className = "resizable";
  w.before(box);
  box.append(w);
  if (!resized) {
    const hint = document.createElement("div");
    hint.className = "resize-hint";
    hint.innerHTML = `<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M1.5 8h13M4 5.5L1.5 8 4 10.5M12 5.5L14.5 8 12 10.5"/></svg><span></span>`;
    hint.querySelector("span").textContent = HINT[document.documentElement.lang] || HINT.en;
    box.after(hint);
    hints.push(hint);
    new MutationObserver(() => { hint.querySelector("span").textContent = HINT[document.documentElement.lang] || HINT.en; })
      .observe(document.documentElement, { attributes: true, attributeFilter: ["lang"] });
  }
  for (const edge of ["left", "right", "bottom"]) {
    const h = document.createElement("div");
    h.className = `handle handle-${edge}`;
    box.append(h);
    h.addEventListener("dblclick", () => { box.style.width = w.style.height = ""; });
    h.addEventListener("pointerdown", e => {
      if (e.button !== 0) return;
      e.preventDefault();
      h.setPointerCapture(e.pointerId);
      h.classList.add("active");
      box.classList.remove("breathing");
      const r = w.getBoundingClientRect(), x0 = e.clientX, y0 = e.clientY;
      const maxW = box.parentElement.clientWidth;
      const shield = w.querySelector(".shield");
      if (shield) shield.hidden = false;            // the app must not take the drag
      document.documentElement.classList.add("resizing");
      const move = ev => {
        if (edge !== "bottom") {
          const dx = (ev.clientX - x0) * (edge.endsWith("left") ? -1 : 1);
          box.style.width = `${Math.round(Math.min(maxW, Math.max(MIN_W, r.width + dx * 2)))}px`;
        }
        if (edge.startsWith("bottom")) w.style.height = `${Math.round(Math.min(MAX_H, Math.max(MIN_H, r.height + ev.clientY - y0)))}px`;
        done();
      };
      const up = () => {
        h.removeEventListener("pointermove", move);
        h.classList.remove("active");
        document.documentElement.classList.remove("resizing");
      };
      h.addEventListener("pointermove", move);
      h.addEventListener("pointerup", up, { once: true });
      h.addEventListener("pointercancel", up, { once: true });
    });
  }
  // The first time a window is well in view (and its app has had a moment to
  // start), it narrows a little and springs back, handles pulsing.
  if (reduced || resized) continue;
  const once = new IntersectionObserver(es => {
    if (!es[0].isIntersecting) return;
    once.disconnect();
    setTimeout(() => {
      if (box.style.width) return;                  // already resized by hand
      const full = box.getBoundingClientRect().width;
      box.classList.add("breathing");
      box.animate([{ width: `${full}px` }, { width: `${Math.max(MIN_W, full - 140)}px`, offset: .45 }, { width: `${full}px` }],
        { duration: 1500, easing: "cubic-bezier(.45, 0, .25, 1)" }).finished.then(() => setTimeout(() => box.classList.remove("breathing"), 1400));
    }, 2600);
  }, { threshold: .6 });
  once.observe(w);
}
})();
