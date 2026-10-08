import { describe, expect, it } from "vitest";
import { axisLabels, chartable, chartPaths, hasEnoughData, windowChart } from "./history";
import type { HistoryPoint, Limit } from "../api";

const HOUR = 3_600_000;
const now = new Date("2026-10-08T15:00:00Z");
const resetsAt = new Date(now.getTime() + 2 * HOUR).toISOString(); // 5h window: started 3h ago

const limit = (over: Partial<Limit> = {}): Limit => ({
  id: "session", label: "Session", group: "session", percent: 40, resetsAt, windowSecs: 5 * 3600,
  severity: "normal", active: false, detail: null, capped: true, ...over,
});

const point = (hoursAgo: number, percent: number, reset = resetsAt): HistoryPoint => ({
  t: new Date(now.getTime() - hoursAgo * HOUR).toISOString(),
  limits: [{ id: "session", percent, resetsAt: reset }],
});

describe("chartable", () => {
  it("keeps only capped limits with a window", () => {
    const keep = limit();
    const out = chartable([keep, limit({ id: "spend", capped: false }), limit({ id: "x", resetsAt: null }), limit({ id: "y", windowSecs: null })]);
    expect(out).toEqual([keep]);
  });
});

describe("windowChart", () => {
  it("places readings by share of the window and appends the live value", () => {
    const c = windowChart([point(2, 10), point(1, 25)], limit(), now);
    expect(c.points.map((p) => [Math.round(p.x * 100), p.y])).toEqual([[20, 10], [40, 25], [60, 40]]);
    expect(c.nowX).toBeCloseTo(0.6);
  });

  it("ignores readings from a previous window", () => {
    const old = new Date(now.getTime() - 4 * HOUR).toISOString();
    const c = windowChart([point(2, 90, old), point(1, 25)], limit(), now);
    expect(c.points).toHaveLength(2);
    expect(c.points[0].y).toBe(25);
  });

  it("tolerates a few seconds of drift in the reset time", () => {
    const drifted = new Date(new Date(resetsAt).getTime() + 3000).toISOString();
    expect(windowChart([point(1, 25, drifted)], limit(), now).points).toHaveLength(2);
  });

  it("ignores readings before the window start or after now", () => {
    const c = windowChart([point(4, 5), point(-1, 50)], limit(), now);
    expect(c.points).toEqual([]);
  });

  it("ignores readings that lack this limit", () => {
    const other: HistoryPoint = { t: point(1, 1).t, limits: [{ id: "weekly_all", percent: 9, resetsAt }] };
    expect(windowChart([other], limit(), now).points).toEqual([]);
  });
});

describe("hasEnoughData", () => {
  it("needs two recorded readings, not just the live one", () => {
    expect(hasEnoughData(windowChart([], limit(), now))).toBe(false);
    expect(hasEnoughData(windowChart([point(1, 20)], limit(), now))).toBe(false);
    expect(hasEnoughData(windowChart([point(2, 10), point(1, 20)], limit(), now))).toBe(true);
  });
});

describe("chartPaths", () => {
  it("draws a line and closes the area down to the baseline", () => {
    const { line, area } = chartPaths([{ x: 0, y: 0 }, { x: 0.5, y: 50 }, { x: 1, y: 100 }], 100, 50);
    expect(line).toBe("M0 50 L50 25 L100 0");
    expect(area).toBe("M0 50 L50 25 L100 0 L100 50 L0 50 Z");
  });

  it("is empty without points", () => {
    expect(chartPaths([], 100, 50)).toEqual({ line: "", area: "" });
  });
});

describe("axisLabels", () => {
  it("uses clock times for short windows", () => {
    const { start, end } = axisLabels(new Date(2026, 9, 8, 17, 49), 5 * 3600);
    expect(start).toMatch(/12:49/);
    expect(end).toMatch(/5:49/);
  });

  it("uses weekdays for the weekly window", () => {
    const { start, end } = axisLabels(new Date(2026, 9, 12, 9, 0), 7 * 86400); // Mon Oct 12 2026
    expect(end).toBe("Mon");
    expect(start).toBe("Mon");
  });
});
