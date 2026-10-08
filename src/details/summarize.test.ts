import { describe, expect, it } from "vitest";
import { summarize } from "./summarize";
import type { DayModel } from "../api";

const row = (date: string, model: string, replies: number, outputTokens = 0): DayModel => ({
  date, model, replies, outputTokens, inputTokens: 10, cacheReadTokens: 0, cacheWriteTokens: 0,
});

describe("summarize", () => {
  const now = new Date(2026, 9, 8, 15, 0);
  const days = [
    row("2026-08-01", "claude-sonnet-4-5", 10, 100),
    row("2026-10-02", "claude-opus-5-5", 30, 300),
    row("2026-10-07", "claude-opus-5-5", 50, 500),
    row("2026-10-07", "claude-sonnet-5-5", 20, 50),
  ];

  it("keeps only the chosen range and ranks models by replies", () => {
    const s = summarize(days, "7", now);
    expect(s.replies).toBe(100);
    expect(s.models.map((m) => m.model)).toEqual(["claude-opus-5-5", "claude-sonnet-5-5"]);
    expect(s.models[0].share).toBeCloseTo(0.8);
    expect(s.activeDays).toBe(2);
  });

  it("has one chart column per calendar day, quiet days included", () => {
    const s = summarize(days, "7", now);
    expect(s.byDay).toHaveLength(7);
    expect(s.byDay[0].date).toBe("2026-10-02");
    expect(s.byDay.at(-1)?.date).toBe("2026-10-08");
    expect(s.byDay.find((d) => d.date === "2026-10-07")?.total).toBe(70);
    expect(s.maxDay).toBe(70);
  });

  it("all time starts at the first logged day", () => {
    const s = summarize(days, "all", now);
    expect(s.replies).toBe(110);
    expect(s.byDay[0].date).toBe("2026-08-01");
  });

  it("handles no data", () => {
    const s = summarize([], "30", now);
    expect(s.replies).toBe(0);
    expect(s.models).toEqual([]);
    expect(s.byDay).toHaveLength(30);
  });
});
