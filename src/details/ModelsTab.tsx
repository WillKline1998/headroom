import { useEffect, useMemo, useState } from "react";
import { api, type Analytics } from "../api";
import { compact, modelName, plural } from "../format";
import { summarize, type Range } from "./summarize";

const RANGES: Record<Range, string> = { "7": "7 days", "30": "30 days", all: "All time" };
const PALETTE = ["#d97757", "#6a9bcc", "#8fae6b", "#c9a227", "#a77fc1", "#5fb3a8", "#999"];

/** Which models you lean on, from Claude Code's logs on this computer. */
export function ModelsTab() {
  const [data, setData] = useState<Analytics | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [range, setRange] = useState<Range>("30");

  useEffect(() => {
    api.analytics().then(setData).catch((e) => setError(String(e)));
  }, []);

  const sum = useMemo(() => (data ? summarize(data.days, range, new Date()) : null), [data, range]);
  const color = (model: string) => PALETTE[Math.max(0, sum?.models.findIndex((m) => m.model === model) ?? 0) % PALETTE.length];

  return (
    <section>
      <h1>Models</h1>
      <p className="muted">From Claude Code&apos;s logs on this computer. Nothing leaves your machine. Chats in the Claude app aren&apos;t logged locally; see “Where this week went” on the Limits tab for that split.</p>

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

      {sum && sum.replies > 0 && (
        <>
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
                <span className="model-sub">{plural(m.replies, "reply", "replies")} · {compact(m.outputTokens)} written</span>
              </li>
            ))}
          </ul>

          <h2>By day</h2>
          <div className="chart" role="img" aria-label="Replies per day by model">
            {sum.byDay.map((d) => (
              <div key={d.date} className="chart-col" title={`${d.date}: ${d.total} replies`}>
                {sum.models.map((m) => {
                  const n = d.perModel[m.model] ?? 0;
                  return n ? <span key={m.model} style={{ height: `${(n / sum.maxDay) * 100}%`, background: color(m.model) }} /> : null;
                })}
              </div>
            ))}
          </div>
          <div className="chart-axis"><span>{sum.byDay[0]?.date}</span><span>{sum.byDay.at(-1)?.date}</span></div>
        </>
      )}
      {data && <p className="hint">{plural(data.filesScanned, "log file")} · {plural(data.sessions, "session")} overall{data.firstSeen ? ` · since ${new Date(data.firstSeen).toLocaleDateString()}` : ""}</p>}
    </section>
  );
}
