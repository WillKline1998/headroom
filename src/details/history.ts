// Pure data-shaping for the "How this window filled" charts (tested in history.test.ts).
import type { HistoryPoint, Limit } from "../api";

/** A recorded reading placed on its window: x = share of the window elapsed, y = percent used (both clamped to 0–1 / 0–100). */
export type ChartPoint = { x: number; y: number };

export type WindowChart = {
  points: ChartPoint[];
  /** Where "now" falls on the window, 0–1. */
  nowX: number;
};

// Anthropic's reset time can shift by a few seconds between checks, so
// compare windows with some slack rather than exactly.
const SAME_WINDOW_MS = 5 * 60_000;

const DAY_MS = 86_400_000;

/** Limits that have a bounded window to chart (not spend, which has no reset). */
export function chartable(limits: Limit[]): Limit[] {
  return limits.filter((l) => l.capped && l.resetsAt && l.windowSecs);
}

/**
 * The recorded readings for a limit's current window. Readings from an
 * earlier window are dropped (a reset starts the chart over), and the live
 * value is appended so the line always reaches "now".
 */
export function windowChart(history: HistoryPoint[], limit: Limit, now: Date): WindowChart {
  const resetsAt = new Date(limit.resetsAt!).getTime();
  const span = limit.windowSecs! * 1000;
  const start = resetsAt - span;
  const at = (ms: number) => Math.min(1, Math.max(0, (ms - start) / span));

  const points: ChartPoint[] = [];
  for (const p of history) {
    const t = new Date(p.t).getTime();
    const r = p.limits.find((x) => x.id === limit.id);
    if (!r?.resetsAt || Math.abs(new Date(r.resetsAt).getTime() - resetsAt) > SAME_WINDOW_MS) continue;
    if (t < start || t > now.getTime()) continue;
    points.push({ x: at(t), y: clampPercent(r.percent) });
  }
  if (points.length > 0) points.push({ x: at(now.getTime()), y: clampPercent(limit.percent) });
  return { points, nowX: at(now.getTime()) };
}

/** A chart needs a line to draw: at least two recorded readings (the live one doesn't count). */
export function hasEnoughData(chart: WindowChart): boolean {
  return chart.points.length >= 3;
}

function clampPercent(p: number): number {
  return Math.min(100, Math.max(0, p));
}

/** SVG path data for the usage line and the filled area under it, inside a w×h plot. */
export function chartPaths(points: ChartPoint[], w: number, h: number): { line: string; area: string } {
  if (points.length === 0) return { line: "", area: "" };
  const xy = points.map((p) => `${round(p.x * w)} ${round(h - (p.y / 100) * h)}`);
  const line = `M${xy.join(" L")}`;
  const last = points[points.length - 1];
  const first = points[0];
  return { line, area: `${line} L${round(last.x * w)} ${h} L${round(first.x * w)} ${h} Z` };
}

const round = (n: number) => Math.round(n * 10) / 10;

/** Axis labels at window start and reset: weekdays for multi-day windows, clock times otherwise. */
export function axisLabels(resetsAt: Date, windowSecs: number): { start: string; end: string } {
  const start = new Date(resetsAt.getTime() - windowSecs * 1000);
  const fmt = (d: Date) =>
    windowSecs * 1000 >= 2 * DAY_MS
      ? d.toLocaleDateString(undefined, { weekday: "short" })
      : d.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
  return { start: fmt(start), end: fmt(resetsAt) };
}
