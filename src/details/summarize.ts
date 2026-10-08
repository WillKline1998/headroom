import type { DayModel, HourCount } from "../api";

export type Range = "7" | "30" | "all";

const ymd = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;

/** First local date (YYYY-MM-DD) included in a range. */
export function rangeStart(range: Range, now: Date, firstLogged?: string): string {
  if (range === "all") return firstLogged ?? ymd(now);
  return ymd(new Date(now.getFullYear(), now.getMonth(), now.getDate() - (Number(range) - 1)));
}

/** Aggregates the per-day rows from Rust into what the Models tab draws. */
export function summarize(days: DayModel[], range: Range, now: Date) {
  const start = rangeStart(range, now, days[0]?.date);
  const rows = days.filter((d) => d.date >= start);

  const models = new Map<string, { model: string; replies: number; outputTokens: number; apiValue: number }>();
  const perDay = new Map<string, Record<string, number>>();
  let replies = 0, outputTokens = 0, inputTokens = 0, apiValue = 0, claudeValue = 0, unpriced = 0;
  for (const r of rows) {
    const m = models.get(r.model) ?? { model: r.model, replies: 0, outputTokens: 0, apiValue: 0 };
    m.replies += r.replies;
    m.outputTokens += r.outputTokens;
    m.apiValue += r.apiValue;
    models.set(r.model, m);
    const day = perDay.get(r.date) ?? {};
    day[r.model] = (day[r.model] ?? 0) + r.replies;
    perDay.set(r.date, day);
    replies += r.replies;
    outputTokens += r.outputTokens;
    inputTokens += r.inputTokens + r.cacheReadTokens + r.cacheWriteTokens;
    apiValue += r.apiValue;
    if (r.model.startsWith("claude")) claudeValue += r.apiValue; // only these draw on a Claude plan
    unpriced += r.unpricedReplies;
  }

  // Every day in the range, including quiet ones, so the chart has a true time axis.
  const byDay: { date: string; perModel: Record<string, number>; total: number }[] = [];
  const cursor = new Date(`${start}T12:00:00`);
  const end = ymd(now);
  while (ymd(cursor) <= end && byDay.length < 400) {
    const date = ymd(cursor);
    const perModel = perDay.get(date) ?? {};
    byDay.push({ date, perModel, total: Object.values(perModel).reduce((a, b) => a + b, 0) });
    cursor.setDate(cursor.getDate() + 1);
  }

  const sorted = [...models.values()].sort((a, b) => b.replies - a.replies).map((m) => ({ ...m, share: replies ? m.replies / replies : 0 }));
  return {
    replies,
    outputTokens,
    inputTokens,
    apiValue,
    unpriced,
    /** Claude-model share of apiValue: the part a Claude plan actually pays for. */
    claudeValue,
    /**
     * Days of history behind the numbers, for "per month at this pace": the
     * range, but never earlier than the first day anything was logged, so a
     * new install isn't averaged over empty weeks.
     */
    spanDays: Math.max(1, daysBetween(start > (days[0]?.date ?? start) ? start : days[0]?.date ?? start, ymd(now)) + 1),
    activeDays: perDay.size,
    models: sorted,
    byDay,
    maxDay: Math.max(1, ...byDay.map((d) => d.total)),
  };
}

function daysBetween(a: string, b: string): number {
  return Math.round((new Date(`${b}T12:00:00`).getTime() - new Date(`${a}T12:00:00`).getTime()) / 86_400_000);
}

/** API value scaled to a 30-day month at the range's pace. */
export function monthlyPace(apiValue: number, spanDays: number): number {
  return spanDays > 0 ? (apiValue / spanDays) * 30 : 0;
}

const DAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/** 7×24 grid of replies (weekday × hour) plus the standout moments. */
export function heatmap(hours: HourCount[], range: Range, now: Date, firstLogged?: string) {
  const start = rangeStart(range, now, firstLogged);
  const grid = Array.from({ length: 7 }, () => new Array<number>(24).fill(0));
  const byHour = new Array<number>(24).fill(0);
  const byWeekday = new Array<number>(7).fill(0);
  for (const h of hours) {
    if (h.date < start) continue;
    grid[h.weekday][h.hour] += h.replies;
    byHour[h.hour] += h.replies;
    byWeekday[h.weekday] += h.replies;
  }
  const max = Math.max(0, ...grid.flat());
  const argmax = (xs: number[]) => xs.reduce((best, x, i) => (x > xs[best] ? i : best), 0);
  const total = byHour.reduce((a, b) => a + b, 0);
  return {
    grid,
    max,
    total,
    days: DAYS,
    busiestHour: total ? argmax(byHour) : null,
    busiestDay: total ? DAYS[argmax(byWeekday)] : null,
  };
}
