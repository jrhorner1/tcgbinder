<!-- The value chart: lines over time, in dollars, with a legend. -->
<script lang="ts" module>
  export interface Series { name: string; color: string; points: { date: string; value: number }[]; step?: boolean }
</script>

<script lang="ts">
  import { usd } from "../lib/format";

  let { series, from = "", to = "" }: { series: Series[]; from?: string; to?: string } = $props();

  let width = $state(900);
  const day = (d: string) => new Date(d + "T00:00:00Z").getTime();
  function niceCeil(v: number) {
    if (v <= 0) return 1;
    const p = Math.pow(10, Math.floor(Math.log10(v)));
    for (const m of [1, 2, 2.5, 5, 10]) if (v <= m * p) return m * p;
    return 10 * p;
  }

  const chart = $derived.by(() => {
    const W = Math.round(Math.max(300, Math.min(900, width))), H = W < 600 ? 220 : 280, L = 64, R = 10, T = 10, B = 26;
    const pts = series.flatMap((s) => s.points);
    if (!pts.length) return { W, H, empty: true as const };
    let t0 = from ? day(from) : Math.min(...pts.map((p) => day(p.date)));
    let t1 = to ? day(to) : Math.max(...pts.map((p) => day(p.date)));
    if (t1 <= t0) { t0 -= 15 * 864e5; t1 += 15 * 864e5; }
    const yMax = niceCeil(Math.max(...pts.map((p) => p.value)) * 1.05);
    const x = (d: string) => L + ((day(d) - t0) / (t1 - t0)) * (W - L - R);
    const y = (v: number) => T + (H - T - B) * (1 - v / yMax);
    const long = t1 - t0 > 400 * 864e5;
    const fmt = (t: number) => new Date(t).toLocaleDateString("en-US", long ? { month: "short", year: "numeric", timeZone: "UTC" } : { month: "short", day: "numeric", timeZone: "UTC" });
    return {
      W, H, empty: false as const,
      grid: [0, 1, 2, 3, 4].map((i) => ({ y: y((yMax * i) / 4), label: usd((yMax * i) / 4) })),
      ticks: ([[t0, "start"], [(t0 + t1) / 2, "middle"], [t1, "end"]] as [number, string][]).map(([t, anchor]) => ({ x: L + ((t - t0) / (t1 - t0)) * (W - L - R), anchor, label: fmt(t) })),
      lines: series.filter((s) => s.points.length).map((s) => ({
        ...s,
        d: s.points.map((p, i) => (i === 0 ? `M${x(p.date)},${y(p.value)}` : s.step ? `H${x(p.date)}V${y(p.value)}` : `L${x(p.date)},${y(p.value)}`)).join(""),
        dots: s.points.length <= 40 ? s.points.map((p) => ({ x: x(p.date), y: y(p.value), title: `${s.name} ${p.date}: ${usd(p.value)}` })) : [],
      })),
      L, R,
    };
  });
</script>

<div class="legend">
  {#each series.filter((s) => s.points.length) as s}<span><i style:background={s.color}></i>{s.name}</span>{/each}
</div>
<div bind:clientWidth={width}>
  <svg viewBox="0 0 {chart.W} {chart.H}" class="chart" style:height="{chart.H}px" role="img" aria-label="Value over time">
    {#if chart.empty}
      <text x={chart.W / 2} y={chart.H / 2} text-anchor="middle">Add cards to see value over time</text>
    {:else}
      {#each chart.grid as g}
        <line x1={chart.L} x2={chart.W - chart.R} y1={g.y} y2={g.y} class="grid" />
        <text x={chart.L - 6} y={g.y + 4} text-anchor="end">{g.label}</text>
      {/each}
      {#each chart.ticks as t}<text x={t.x} y={chart.H - 6} text-anchor={t.anchor as "start" | "middle" | "end"}>{t.label}</text>{/each}
      {#each chart.lines as l}
        <path d={l.d} fill="none" stroke={l.color} stroke-width="2" />
        {#each l.dots as p}<circle cx={p.x} cy={p.y} r="3" fill={l.color}><title>{p.title}</title></circle>{/each}
      {/each}
    {/if}
  </svg>
</div>

<style>
  .chart { width: 100%; height: 280px; display: block; }
  .chart text { fill: var(--muted); font-size: 11px; }
  .chart .grid { stroke: var(--line); }
  .legend { display: flex; gap: 16px; flex-wrap: wrap; font-size: 13px; color: var(--muted); margin: 0 0 6px; }
  .legend i { display: inline-block; width: 10px; height: 10px; border-radius: 2px; margin-right: 6px; vertical-align: -1px; }
</style>
