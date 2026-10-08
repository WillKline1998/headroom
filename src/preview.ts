// Browser preview: `npm run dev` then open http://localhost:1420/?preview=widget
// (or ?preview=details#details/models). Fakes the Rust side with sample data so
// the UI can be designed, screenshotted and demoed without the desktop app.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type { Analytics, Settings, UsageState } from "./api";

const now = Date.now();
const iso = (ms: number) => new Date(ms).toISOString();
const H = 3_600_000;

const usage: UsageState = {
  status: "ok",
  message: null,
  checkedAt: iso(now - 40_000),
  snapshot: {
    provider: "claude",
    plan: "pro",
    fetchedAt: iso(now - 40_000),
    extraUsage: false,
    breakdownSince: iso(now - 2.5 * 24 * H),
    limits: [
      { id: "session", label: "Current session", group: "session", percent: 14, resetsAt: iso(now + 3.2 * H), windowSecs: 5 * 3600, severity: "normal", active: false },
      { id: "weekly_all", label: "Weekly · all models", group: "weekly", percent: 31, resetsAt: iso(now + 4.4 * 24 * H), windowSecs: 7 * 24 * 3600, severity: "normal", active: true },
    ],
    breakdown: [
      { key: "claude_code", label: "Claude Code", percent: 82 },
      { key: "chat", label: "Chats", percent: 15 },
      { key: "cowork", label: "Cowork", percent: 3 },
      { key: "other", label: "Other", percent: 0 },
    ],
  },
};

// Thirty days of plausible activity, mostly Opus with some Sonnet and Haiku.
function sampleAnalytics(): Analytics {
  const days: Analytics["days"] = [];
  let seed = 7;
  const rand = () => ((seed = (seed * 9301 + 49297) % 233280) / 233280);
  for (let i = 29; i >= 0; i--) {
    const d = new Date(now - i * 24 * H);
    const date = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
    if (d.getDay() === 0 && rand() < 0.7) continue; // quiet Sundays
    for (const [model, scale] of [["claude-opus-5-5", 120], ["claude-sonnet-5-5", 45], ["claude-haiku-5-0", 15]] as const) {
      const replies = Math.round(scale * rand() * (i < 10 ? 1.4 : 1));
      if (replies) days.push({ date, model, replies, inputTokens: replies * 40, outputTokens: replies * 900, cacheReadTokens: replies * 30_000, cacheWriteTokens: replies * 1_500 });
    }
  }
  return { days, sessions: 64, filesScanned: 211, firstSeen: iso(now - 29 * 24 * H), sources: ["~/.claude/projects"] };
}

let settings: Settings = { refreshSecs: 180, alwaysOnTop: true, notify: true, notifyAt: [80, 95], trayText: "both" };

export function installPreview(label: string) {
  mockWindows(label);
  const analytics = sampleAnalytics();
  mockIPC((cmd, args) => {
    switch (cmd) {
      case "get_state": return usage;
      case "get_analytics": return analytics;
      case "get_settings": return settings;
      case "save_settings": settings = (args as { settings: Settings }).settings; return settings;
      case "plugin:autostart|is_enabled": return false;
      default: return null; // window sizing, refresh, event listeners: no-ops in the browser
    }
  });
}
