// Charts drawn as SVG: the value line chart and its legend, bar rows, and the number
// tiles at the top of pages.

import { h } from "./dom.js";
import { usd } from "./format.js";

const SVG = "http://www.w3.org/2000/svg";

function s(tag, attrs, text) {
  const el = document.createElementNS(SVG, tag);
  for (const [k, v] of Object.entries(attrs || {})) el.setAttribute(k, v);
  if (text !== undefined) el.textContent = text;
  return el;
}

function niceCeil(v) {
  if (v <= 0) return 1;
  const p = Math.pow(10, Math.floor(Math.log10(v)));
  for (const m of [1, 2, 2.5, 5, 10]) if (v <= m * p) return m * p;
  return 10 * p;
}

const day = (d) => new Date(d + "T00:00:00Z").getTime();

// lineChart draws series [{name, color, points:[{date,value}], step}] over [from, to],
// width pixels wide (so text stays readable on phones).
export function lineChart(series, from, to, width) {
  const W = Math.round(Math.max(300, Math.min(900, width || 900))), H = W < 600 ? 220 : 280, L = 64, R = 10, T = 10, B = 26;
  const svg = s("svg", { viewBox: `0 0 ${W} ${H}`, class: "chart", role: "img", "aria-label": "Value over time" });
  const pts = series.flatMap((x) => x.points);
  if (!pts.length) {
    svg.append(s("text", { x: W / 2, y: H / 2, "text-anchor": "middle" }, "Add cards to see value over time"));
    return svg;
  }
  let t0 = from ? day(from) : Math.min(...pts.map((p) => day(p.date)));
  let t1 = to ? day(to) : Math.max(...pts.map((p) => day(p.date)));
  if (t1 <= t0) { t0 -= 15 * 864e5; t1 += 15 * 864e5; }
  const yMax = niceCeil(Math.max(...pts.map((p) => p.value)) * 1.05);
  const x = (d) => L + ((day(d) - t0) / (t1 - t0)) * (W - L - R);
  const y = (v) => T + (H - T - B) * (1 - v / yMax);
  for (let i = 0; i <= 4; i++) {
    const v = (yMax * i) / 4;
    svg.append(s("line", { x1: L, x2: W - R, y1: y(v), y2: y(v), class: "grid" }));
    svg.append(s("text", { x: L - 6, y: y(v) + 4, "text-anchor": "end" }, usd(v)));
  }
  const long = t1 - t0 > 400 * 864e5;
  svg.style.height = `${H}px`;
  const fmt = (t) => new Date(t).toLocaleDateString("en-US", long ? { month: "short", year: "numeric", timeZone: "UTC" } : { month: "short", day: "numeric", timeZone: "UTC" });
  [[t0, "start"], [(t0 + t1) / 2, "middle"], [t1, "end"]].forEach(([t, anchor]) => {
    svg.append(s("text", { x: L + ((t - t0) / (t1 - t0)) * (W - L - R), y: H - 6, "text-anchor": anchor }, fmt(t)));
  });
  for (const ser of series) {
    if (!ser.points.length) continue;
    let d = "";
    ser.points.forEach((p, i) => {
      const px = x(p.date), py = y(p.value);
      if (i === 0) d += `M${px},${py}`;
      else d += ser.step ? `H${px}V${py}` : `L${px},${py}`;
    });
    svg.append(s("path", { d, fill: "none", stroke: ser.color, "stroke-width": 2 }));
    if (ser.points.length <= 40) {
      for (const p of ser.points) {
        const dot = s("circle", { cx: x(p.date), cy: y(p.value), r: 3, fill: ser.color });
        dot.append(s("title", {}, `${ser.name} ${p.date}: ${usd(p.value)}`));
        svg.append(dot);
      }
    }
  }
  return svg;
}

export function legend(series) {
  return h("div.legend", series.filter((x) => x.points.length).map((x) => h("span", h("i", { style: `background:${x.color}` }), x.name)));
}

export function bars(items, max, color) {
  return h("div.bars", items.flatMap((it) => [
    it.onclick ? h("button.label.link", { type: "button", title: it.label, onclick: it.onclick }, it.label) : h("div.label", { title: it.label }, it.label),
    h("div.track", h("div.fill", { style: `width:${Math.max(0, Math.min(1, it.value / (max || 1))) * 100}%;background:${color}` })),
    h("div.num", it.text),
  ]));
}

export function tile(cap, val, det, color) {
  return h("div.tile", h("div.cap", cap), h("div.val", { style: color ? `color:${color}` : null }, val), h("div.det", det || ""));
}

export function gain(cost, market) {
  const diff = market - cost;
  const pct = cost > 0 ? `${diff >= 0 ? "+" : ""}${((diff / cost) * 100).toFixed(1)}% vs. price paid` : "";
  return { text: (diff >= 0 ? "+" : "") + usd(diff), pct, color: diff >= 0 ? "var(--green)" : "var(--red)" };
}
