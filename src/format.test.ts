import { describe, expect, it } from "vitest";
import { ago, compact, countdown, elapsed, level, modelName, resetClock } from "./format";

describe("countdown", () => {
  it("formats days, hours and minutes", () => {
    expect(countdown(4 * 86_400_000 + 8 * 3_600_000 + 5 * 60_000)).toBe("4d 8h");
    expect(countdown(2 * 3_600_000 + 14 * 60_000)).toBe("2h 14m");
    expect(countdown(3 * 3_600_000)).toBe("3h");
    expect(countdown(9 * 60_000 + 30_000)).toBe("9m");
    expect(countdown(20_000)).toBe("<1m");
    expect(countdown(-5)).toBe("now");
  });
});

describe("resetClock", () => {
  const now = new Date(2026, 9, 8, 13, 0);
  it("shows only the time for today", () => {
    expect(resetClock(new Date(2026, 9, 8, 17, 49), now)).toMatch(/5:49/);
    expect(resetClock(new Date(2026, 9, 8, 17, 49), now)).not.toMatch(/tomorrow/);
  });
  it("says tomorrow, or the weekday further out", () => {
    expect(resetClock(new Date(2026, 9, 9, 9, 0), now)).toMatch(/^tomorrow 9:00/);
    expect(resetClock(new Date(2026, 9, 12, 0, 59), now)).toMatch(/^Mon 12:59/);
  });
});

describe("ago", () => {
  it("rounds recent checks to just now", () => {
    const now = new Date(2026, 9, 8, 13, 0);
    expect(ago(new Date(now.getTime() - 10_000), now)).toBe("just now");
    expect(ago(new Date(now.getTime() - 3 * 60_000), now)).toBe("3m ago");
  });
});

describe("elapsed", () => {
  const now = new Date("2026-10-08T17:00:00Z");
  it("is the share of the window already passed", () => {
    const resets = new Date("2026-10-08T19:30:00Z"); // 2.5h left of 5h
    expect(elapsed(resets, 5 * 3600, now)).toBeCloseTo(0.5);
  });
  it("is clamped and null when unknown", () => {
    expect(elapsed(new Date("2026-10-30T00:00:00Z"), 3600, now)).toBe(0);
    expect(elapsed(null, 3600, now)).toBeNull();
    expect(elapsed(now, null, now)).toBeNull();
  });
});

describe("level", () => {
  it("warms up as usage climbs", () => {
    expect(level(10)).toBe("ok");
    expect(level(75)).toBe("warn");
    expect(level(95)).toBe("hot");
  });
});

describe("modelName", () => {
  it.each([
    ["claude-opus-5-5", "Opus 5.5"],
    ["claude-sonnet-4-5-20250929", "Sonnet 4.5"],
    ["claude-3-5-haiku-20241022", "Haiku 3.5"],
    ["claude-opus-4-1-20250805", "Opus 4.1"],
    ["claude-fable-1", "Fable 1"],
    ["gpt-4o", "gpt-4o"],
  ])("%s → %s", (id, name) => expect(modelName(id)).toBe(name));
});

describe("compact", () => {
  it("shortens big numbers", () => {
    expect(compact(950)).toBe("950");
    expect(compact(1234)).toBe("1.2K");
    expect(compact(3_400_000)).toBe("3.4M");
  });
});
