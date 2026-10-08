// Pure formatting helpers (unit-tested in format.test.ts).

const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

/** "4d 8h", "2h 14m", "9m", "<1m" */
export function countdown(ms: number): string {
  if (ms <= 0) return "now";
  if (ms < MIN) return "<1m";
  const d = Math.floor(ms / DAY);
  const h = Math.floor((ms % DAY) / HOUR);
  const m = Math.floor((ms % HOUR) / MIN);
  if (d > 0) return h ? `${d}d ${h}h` : `${d}d`;
  if (h > 0) return m ? `${h}h ${m}m` : `${h}h`;
  return `${m}m`;
}

/** Reset moment in local time: "5:49 PM" today, "Mon 12:59 AM" later in the week. */
export function resetClock(at: Date, now: Date): string {
  const time = at.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
  const sameDay = at.toDateString() === now.toDateString();
  if (sameDay) return time;
  const tomorrow = new Date(now.getTime() + DAY).toDateString() === at.toDateString();
  if (tomorrow) return `tomorrow ${time}`;
  return `${at.toLocaleDateString(undefined, { weekday: "short" })} ${time}`;
}

/** "Updated just now" / "Updated 3m ago" */
export function ago(at: Date, now: Date): string {
  const ms = now.getTime() - at.getTime();
  return ms < MIN ? "just now" : `${countdown(ms)} ago`;
}

/**
 * Share of the window that has already passed (0–1), so the bar can show
 * whether usage is ahead of or behind the clock. Null when unknown.
 */
export function elapsed(resetsAt: Date | null, windowSecs: number | null, now: Date): number | null {
  if (!resetsAt || !windowSecs) return null;
  const left = (resetsAt.getTime() - now.getTime()) / 1000;
  return Math.min(1, Math.max(0, 1 - left / windowSecs));
}

/** Calm → warm → hot, by how much of the limit is used. */
export function level(percent: number): "ok" | "warn" | "hot" {
  return percent >= 90 ? "hot" : percent >= 70 ? "warn" : "ok";
}

const FAMILIES = ["opus", "sonnet", "haiku"];

/** "claude-opus-5-5" → "Opus 5.5", "claude-3-5-haiku-20241022" → "Haiku 3.5". */
export function modelName(id: string): string {
  if (!id.startsWith("claude")) return id; // other providers: show their id as-is
  const parts = id.replace(/^claude-/, "").replace(/-\d{8}$/, "").split("-");
  const family = parts.find((p) => FAMILIES.includes(p)) ?? parts.find((p) => /^[a-z]+$/i.test(p));
  if (!family) return id;
  const version = parts.filter((p) => /^\d+$/.test(p)).join(".");
  const name = family[0].toUpperCase() + family.slice(1);
  return version ? `${name} ${version}` : name;
}

/** 1234 → "1.2K", 3_400_000 → "3.4M" */
export function compact(n: number): string {
  return new Intl.NumberFormat("en", { notation: "compact", maximumFractionDigits: 1 }).format(n);
}

/** "$0.42", "$38", "$1,240" */
export function usd(n: number): string {
  const digits = n >= 100 || Number.isInteger(n) ? 0 : 2;
  return n.toLocaleString("en-US", { style: "currency", currency: "USD", minimumFractionDigits: digits, maximumFractionDigits: digits });
}

/** Monthly list price of personal Claude plans, for the "API value" comparison. */
export function planPrice(plan: string | null, tier: string | null): { name: string; monthly: number } | null {
  switch (plan) {
    case "pro":
      return { name: "Pro", monthly: 20 };
    case "max":
      return tier?.includes("20x") ? { name: "Max 20x", monthly: 200 } : { name: "Max 5x", monthly: 100 };
    default:
      return null; // Team / Enterprise pricing varies by contract
  }
}

/** "10 PM", "12 AM" */
export function hourLabel(h: number): string {
  const suffix = h < 12 ? "AM" : "PM";
  return `${h % 12 === 0 ? 12 : h % 12} ${suffix}`;
}

export function plural(n: number, one: string, many = `${one}s`): string {
  return `${n.toLocaleString()} ${n === 1 ? one : many}`;
}
