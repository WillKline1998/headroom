import { api } from "../api";
import { LimitBar } from "../components/LimitBar";
import { Message } from "../components/Message";
import { ago } from "../format";
import { useNow, useUsage } from "../hooks";

export function LimitsTab() {
  const state = useUsage();
  const now = useNow();
  const s = state.snapshot;

  if (!s) {
    return (
      <section>
        <h1>Limits</h1>
        <p className="muted">{state.message ? <Message text={state.message} /> : "Checking your usage…"}</p>
        {state.status === "no_plan" ? (
          <button className="btn" onClick={() => (window.location.hash = "details/models")}>See model stats</button>
        ) : (
          <button className="btn" onClick={() => api.refresh()}>Try again</button>
        )}
      </section>
    );
  }

  return (
    <section>
      <h1>Limits</h1>
      <p className="muted">
        {s.plan ? `Claude ${s.plan[0].toUpperCase()}${s.plan.slice(1)} plan` : "Claude"} · updated {ago(new Date(s.fetchedAt), now)}{s.via === "hermes" && " · via Hermes Agent's sign-in"}
        {state.status === "error" && state.message && <span className="warn-text"> · {state.message}</span>}
      </p>
      <div className="limits limits-large">
        {s.limits.map((l) => (
          <LimitBar key={l.id} limit={l} now={now} large />
        ))}
      </div>
      <p className="hint">The thin tick on each bar shows how much of that window has passed. A fill beyond the tick means you&apos;re using it faster than the clock.</p>

      {s.breakdown.length > 0 && (
        <>
          <h2>Where this week went</h2>
          <ul className="breakdown">
            {s.breakdown.map((b) => (
              <li key={b.key}>
                <span className="breakdown-label">{b.label}</span>
                <span className="breakdown-bar"><span style={{ width: `${b.percent}%` }} /></span>
                <span className="breakdown-pct">{Math.round(b.percent)}%</span>
              </li>
            ))}
          </ul>
          {s.breakdownSince && <p className="hint">Share of your weekly usage since {new Date(s.breakdownSince).toLocaleString(undefined, { weekday: "short", hour: "numeric", minute: "2-digit" })}, as reported by Anthropic.</p>}
        </>
      )}

      <p className="hint">Extra usage (pay-as-you-go past your limits) is {s.extraUsage ? "on" : "off"} for this account.</p>
    </section>
  );
}
