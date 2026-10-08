// Browser preview: `npm run dev` then open http://localhost:1420/?preview=widget
// (or ?preview=details#details/models). Add &account=enterprise for a
// usage-based work account, or &account=apikey for an API-key setup.
// Fakes the Rust side with sample data so the UI can be designed,
// screenshotted and demoed without the desktop app.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type { Analytics, Limit, Settings, UsageState } from "./api";

const now = Date.now();
const iso = (ms: number) => new Date(ms).toISOString();
const H = 3_600_000;

const limit = (l: Partial<Limit> & Pick<Limit, "id" | "label" | "group" | "percent">): Limit => ({
  resetsAt: null, windowSecs: null, severity: "normal", active: false, detail: null, capped: true, ...l,
});

const breakdown = [
  { key: "claude_code", label: "Claude Code", percent: 82 },
  { key: "chat", label: "Chats", percent: 15 },
  { key: "cowork", label: "Cowork", percent: 3 },
  { key: "other", label: "Other", percent: 0 },
];

function usageFor(account: string | null): UsageState {
  const base = { status: "ok" as const, message: null, checkedAt: iso(now - 40_000) };
  const snapshot = { provider: "claude", fetchedAt: iso(now - 40_000), extraUsage: false, breakdownSince: iso(now - 2.5 * 24 * H), breakdown };
  if (account === "apikey") {
    return { snapshot: null, status: "no_plan", checkedAt: base.checkedAt, message: "Claude Code is signed in with an API key, which is billed per use and has no plan limits. The Models tab still works. To track a Claude plan instead, run `claude auth login` (add `--sso` for company sign-in)." };
  }
  if (account === "enterprise") {
    return {
      ...base,
      snapshot: {
        ...snapshot, plan: "enterprise", tier: null,
        limits: [
          limit({ id: "session", label: "Current session", group: "session", percent: 46, resetsAt: iso(now + 1.7 * H), windowSecs: 5 * 3600 }),
          limit({ id: "weekly_all", label: "Weekly · all models", group: "weekly", percent: 58, resetsAt: iso(now + 2.1 * 24 * H), windowSecs: 7 * 24 * 3600, active: true }),
          limit({ id: "spend", label: "Spend this month", group: "spend", percent: 12.4, detail: "$12.40 of $100.00" }),
        ],
      },
    };
  }
  return {
    ...base,
    snapshot: {
      ...snapshot, plan: "pro", tier: "default_claude_ai",
      limits: [
        limit({ id: "session", label: "Current session", group: "session", percent: 14, resetsAt: iso(now + 3.2 * H), windowSecs: 5 * 3600 }),
        limit({ id: "weekly_all", label: "Weekly · all models", group: "weekly", percent: 31, resetsAt: iso(now + 4.4 * 24 * H), windowSecs: 7 * 24 * 3600, active: true }),
      ],
    },
  };
}

// Thirty days of plausible activity: mostly Opus, evenings and weekday mornings.
function sampleAnalytics(): Analytics {
  const days: Analytics["days"] = [];
  const hours: Analytics["hours"] = [];
  let seed = 7;
  const rand = () => ((seed = (seed * 9301 + 49297) % 233280) / 233280);
  const price = { "claude-opus-5-5": 0.09, "claude-sonnet-5-5": 0.03, "claude-haiku-5-5": 0.002 } as const;
  for (let i = 29; i >= 0; i--) {
    const d = new Date(now - i * 24 * H);
    const date = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
    if (d.getDay() === 0 && rand() < 0.7) continue; // quiet Sundays
    let total = 0;
    for (const [model, scale] of [["claude-opus-5-5", 120], ["claude-sonnet-5-5", 45], ["claude-haiku-5-5", 15]] as const) {
      const replies = Math.round(scale * rand() * (i < 10 ? 1.4 : 1));
      total += replies;
      if (replies) days.push({ date, source: i % 3 === 0 && model === "claude-opus-5-5" ? "hermes" : "claude_code", model, replies, inputTokens: replies * 40, outputTokens: replies * 900, cacheReadTokens: replies * 30_000, cacheWriteTokens: replies * 1_500, apiValue: replies * price[model], unpricedReplies: 0 });
    }
    const weekday = (d.getDay() + 6) % 7;
    const weights = Array.from({ length: 24 }, (_, h) => (h >= 19 && h <= 23 ? 3 : h >= 8 && h <= 11 && weekday < 5 ? 1.5 : h < 7 ? 0 : 0.4) * (0.5 + rand()));
    const sum = weights.reduce((a, b) => a + b, 0);
    weights.forEach((w, hour) => {
      const replies = Math.round((total * w) / sum);
      if (replies) hours.push({ date, source: "claude_code", weekday, hour, replies });
    });
  }
  return {
    days, hours, pricesAsOf: "2026-10-08", sessions: 64, filesScanned: 211, firstSeen: iso(now - 29 * 24 * H), hermesAvailable: false,
    sources: [
      { id: "claude_code", label: "Claude Code", location: "~/.claude/projects", sessions: 52 },
      { id: "hermes", label: "Hermes Agent", location: "~/.hermes/state.db", sessions: 12 },
    ],
  };
}

let settings: Settings = { refreshSecs: 180, alwaysOnTop: true, notify: true, notifyAt: [80, 95], trayText: "both", includeHermes: true };

export function installPreview(label: string) {
  mockWindows(label);
  const usage = usageFor(new URLSearchParams(location.search).get("account"));
  const analytics = sampleAnalytics();
  mockIPC((cmd, args) => {
    switch (cmd) {
      case "get_state": return usage;
      case "get_analytics": return analytics;
      case "get_settings": return settings;
      case "save_settings": settings = (args as { settings: Settings }).settings; return settings;
      case "plugin:autostart|is_enabled": return false;
      case "hermes_detected": return true;
      default: return null; // window sizing, refresh, event listeners: no-ops in the browser
    }
  });
}
