import type { Limit } from "../api";
import { countdown, elapsed, level, resetClock } from "../format";

/**
 * One limit: label, percent, a bar, and when it resets. The thin tick marks how
 * much of the window has passed: a fill past the tick means you're using it
 * faster than the clock is running. Spend with no ceiling shows just the amount.
 */
export function LimitBar({ limit, now, large = false }: { limit: Limit; now: Date; large?: boolean }) {
  const resets = limit.resetsAt ? new Date(limit.resetsAt) : null;
  const pace = elapsed(resets, limit.windowSecs, now);
  const pct = Math.min(100, Math.max(0, limit.percent));

  if (!limit.capped) {
    return (
      <div className={`limit ${large ? "limit-large" : ""}`} data-level="ok">
        <div className="limit-head">
          <span className="limit-label">{limit.label}</span>
          <span className="limit-pct">{limit.detail?.split(" · ")[0]}</span>
        </div>
        <div className="limit-reset">No spending limit set</div>
      </div>
    );
  }

  // Spend has no reset timestamp; its label already says "this month".
  const when = resets ? `Resets in ${countdown(resets.getTime() - now.getTime())} · ${resetClock(resets, now)}` : limit.group === "spend" ? "" : "Not started";
  return (
    <div className={`limit ${large ? "limit-large" : ""}`} data-level={level(pct)}>
      <div className="limit-head">
        <span className="limit-label">{limit.label}</span>
        <span className="limit-pct">{Math.round(pct)}%</span>
      </div>
      <div className="bar" role="progressbar" aria-valuenow={Math.round(pct)} aria-valuemin={0} aria-valuemax={100} aria-label={limit.label}>
        <div className="bar-fill" style={{ width: `${pct}%` }} />
        {pace !== null && <div className="bar-pace" style={{ left: `${pace * 100}%` }} title="How much of this window has passed" />}
      </div>
      <div className="limit-reset">
        {limit.detail && <span className="limit-detail">{limit.detail}</span>}
        {limit.detail && when && " · "}
        {when}
      </div>
    </div>
  );
}
