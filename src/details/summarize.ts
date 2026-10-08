import type { DayModel } from "../api";

export type Range = "7" | "30" | "all";

const ymd = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;

/** Aggregates the per-day rows from Rust into what the Models tab draws. */
export function summarize(days: DayModel[], range: Range, now: Date) {
  const span = range === "all" ? null : Number(range);
  const start = span ? ymd(new Date(now.getFullYear(), now.getMonth(), now.getDate() - (span - 1))) : days[0]?.date ?? ymd(now);
  const rows = days.filter((d) => d.date >= start);

  const models = new Map<string, { model: string; replies: number; outputTokens: number }>();
  const perDay = new Map<string, Record<string, number>>();
  let replies = 0, outputTokens = 0, inputTokens = 0;
  for (const r of rows) {
    const m = models.get(r.model) ?? { model: r.model, replies: 0, outputTokens: 0 };
    m.replies += r.replies;
    m.outputTokens += r.outputTokens;
    models.set(r.model, m);
    const day = perDay.get(r.date) ?? {};
    day[r.model] = (day[r.model] ?? 0) + r.replies;
    perDay.set(r.date, day);
    replies += r.replies;
    outputTokens += r.outputTokens;
    inputTokens += r.inputTokens + r.cacheReadTokens + r.cacheWriteTokens;
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
    activeDays: perDay.size,
    models: sorted,
    byDay,
    maxDay: Math.max(1, ...byDay.map((d) => d.total)),
  };
}
