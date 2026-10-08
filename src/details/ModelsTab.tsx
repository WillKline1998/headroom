import { useEffect, useMemo, useState } from "react";
import { api, type Analytics } from "../api";
import { compact, hourLabel, modelName, planPrice, plural, usd } from "../format";
import { useUsage } from "../hooks";
import { heatmap, monthlyPace, summarize, type Range } from "./summarize";

const RANGES: Record<Range, string> = { "7": "7 days", "30": "30 days", all: "All time" };
const shortDate = (ymd?: string) => (ymd ? new Date(`${ymd}T12:00:00`).toLocaleDateString(undefined, { month: "short", day: "numeric" }) : "");
const PALETTE = ["#d97757", "#6a9bcc", "#8fae6b", "#c9a227", "#a77fc1", "#5fb3a8", "#999"];

/** Which models you lean on, what that would cost on the API, and when you work. */
export function ModelsTab() {
  const [data, setData] = useState<Analytics | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [range, setRange] = useState<Range>("30");
  const [source, setSource] = useState<string>("all");
  const usage = useUsage();

  useEffect(() => {
    api.analytics().then(setData).catch((e) => setError(String(e)));
  }, []);

  const now = useMemo(() => new Date(), [data]);
  const pick = <T extends { source: string }>(rows: T[]) => (source === "all" ? rows : rows.filter((r) => r.source === source));
  const sum = useMemo(() => (data ? summarize(pick(data.days), range, now) : null), [data, range, now, source]);
  const heat = useMemo(() => (data ? heatmap(pick(data.hours), range, now, data.days[0]?.date) : null), [data, range, now, source]);
  const sources = data?.sources ?? [];
  const color = (model: string) => PALETTE[Math.max(0, sum?.models.findIndex((m) => m.model === model) ?? 0) % PALETTE.length];
  const plan = planPrice(usage.snapshot?.plan ?? null, usage.snapshot?.tier ?? null);

  return (
    <section>
      <h1>Models</h1>
      <p className="muted">
        From {sources.length > 1 ? "Claude Code's and Hermes Agent's logs" : sources[0]?.id === "hermes" ? "Hermes Agent's logs" : "Claude Code's logs"} on this computer. Nothing leaves your machine.
        Chats in the Claude app aren&apos;t logged locally; see “Where this week went” on the Limits tab for that split.
      </p>

      {sources.length > 1 && (
        <div className="pills" role="radiogroup" aria-label="Source">
          {[{ id: "all", label: "All sources" }, ...sources].map((s) => (
            <button key={s.id} className="pill" role="radio" aria-checked={source === s.id} onClick={() => setSource(s.id)}>
              {s.label}
            </button>
          ))}
        </div>
      )}

      <div className="pills" role="radiogroup" aria-label="Date range">
        {(Object.keys(RANGES) as Range[]).map((r) => (
          <button key={r} className="pill" role="radio" aria-checked={range === r} onClick={() => setRange(r)}>
            {RANGES[r]}
          </button>
        ))}
      </div>

      {error && <p className="warn-text">{error}</p>}
      {!data && !error && <p className="muted">Reading logs…</p>}
      {sum && sum.replies === 0 && <p className="muted">No Claude Code activity in this range.</p>}

      {sum && heat && sum.replies > 0 && (
        <>
          <div className="value-card">
            <div>
              <span className="value-label">API value</span>
              <strong className="value-amount">{usd(sum.apiValue)}</strong>
              <span className="value-sub">what this would have cost at pay-as-you-go API prices</span>
            </div>
            {plan && (
              <div className="value-compare">
                <strong>{Math.max(0, Math.round(monthlyPace(sum.apiValue, sum.spanDays) / plan.monthly * 10) / 10)}×</strong>
                <span>your {usd(plan.monthly)}/mo {plan.name} plan. At this pace that&apos;s about {usd(monthlyPace(sum.apiValue, sum.spanDays))} a month.</span>
              </div>
            )}
          </div>

          <div className="stats">
            <div><strong>{compact(sum.replies)}</strong><span>replies</span></div>
            <div><strong>{compact(sum.outputTokens)}</strong><span>tokens written</span></div>
            <div><strong>{compact(sum.inputTokens)}</strong><span>tokens read</span></div>
            <div><strong>{sum.activeDays}</strong><span>active days</span></div>
          </div>

          <h2>By model</h2>
          <ul className="model-list">
            {sum.models.map((m) => (
              <li key={m.model}>
                <span className="swatch" style={{ background: color(m.model) }} />
                <span className="model-name" title={m.model}>{modelName(m.model)}</span>
                <span className="breakdown-bar"><span style={{ width: `${m.share * 100}%`, background: color(m.model) }} /></span>
                <span className="breakdown-pct">{Math.round(m.share * 100)}%</span>
                <span className="model-sub">{plural(m.replies, "reply", "replies")} · {compact(m.outputTokens)} written · {usd(m.apiValue)} API value</span>
              </li>
            ))}
          </ul>

          <h2>By day</h2>
          <div className="chart" role="img" aria-label="Replies per day by model">
            {sum.byDay.map((d) => (
              <div key={d.date} className="chart-col" title={`${shortDate(d.date)}: ${d.total} replies`}>
                {sum.models.map((m) => {
                  const n = d.perModel[m.model] ?? 0;
                  return n ? <span key={m.model} style={{ height: `${(n / sum.maxDay) * 100}%`, background: color(m.model) }} /> : null;
                })}
              </div>
            ))}
          </div>
          <div className="chart-axis"><span>{shortDate(sum.byDay[0]?.date)}</span><span>{shortDate(sum.byDay.at(-1)?.date)}</span></div>

          <h2>Busiest hours</h2>
          {heat.busiestHour !== null && (
            <p className="muted">
              You&apos;re most active around <strong>{hourLabel(heat.busiestHour)}</strong>, and <strong>{heat.busiestDay}</strong> is your busiest day.
            </p>
          )}
          <div className="heat" role="img" aria-label="Replies by weekday and hour">
            {heat.grid.map((row, d) => (
              <div key={d} className="heat-row">
                <span className="heat-day">{heat.days[d]}</span>
                {row.map((n, h) => (
                  <span
                    key={h}
                    className="heat-cell"
                    title={`${heat.days[d]} ${hourLabel(h)}: ${n} replies`}
                    style={{ opacity: n ? 0.15 + 0.85 * (n / heat.max) : 1, background: n ? "var(--ok)" : "var(--track)" }}
                  />
                ))}
              </div>
            ))}
            <div className="heat-row heat-axis">
              <span className="heat-day" />
              {Array.from({ length: 24 }, (_, h) => (
                <span key={h} className="heat-hour">{h % 6 === 0 ? hourLabel(h).replace(" ", "") : ""}</span>
              ))}
            </div>
          </div>

          <p className="hint">
            API value uses Anthropic&apos;s list prices as of {data?.pricesAsOf}, including cache reads and writes.
            {sum.unpriced > 0 && ` ${plural(sum.unpriced, "reply", "replies")} from models without a known price aren't counted.`}
            {" "}Your plan isn&apos;t billed this way; it&apos;s a sense of what the subscription is worth to you.
          </p>
        </>
      )}
      {data && (
        <p className="hint">
          {sources.map((s) => `${s.label}: ${plural(s.sessions, "session")}`).join(" · ") || "No logs found"}
          {data.firstSeen ? ` · since ${new Date(data.firstSeen).toLocaleDateString()}` : ""}
          {data.hermesAvailable && " · Hermes Agent usage is switched off in Settings."}
        </p>
      )}
    </section>
  );
}
