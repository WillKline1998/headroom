// Typed bridge to the Rust side (src-tauri/src/lib.rs commands + "usage" event).
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type Limit = {
  id: string;
  label: string;
  group: string;
  percent: number;
  resetsAt: string | null;
  windowSecs: number | null;
  severity: string;
  active: boolean;
  /** Extra line, e.g. "$12.40 of $100.00" for spend limits. */
  detail: string | null;
  /** False = no ceiling: show the amount, not a bar. */
  capped: boolean;
};

export type Breakdown = { key: string; label: string; percent: number };

export type Snapshot = {
  provider: string;
  plan: string | null;
  tier: string | null;
  limits: Limit[];
  breakdown: Breakdown[];
  breakdownSince: string | null;
  extraUsage: boolean;
  fetchedAt: string;
};

export type UsageState = {
  snapshot: Snapshot | null;
  status: "loading" | "ok" | "signed_out" | "no_plan" | "error";
  message: string | null;
  checkedAt: string | null;
};

export type DayModel = {
  date: string;
  /** "claude_code" | "hermes" */
  source: string;
  model: string;
  replies: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  /** USD at API list prices. */
  apiValue: number;
  unpricedReplies: number;
};

/** Replies in one local hour; weekday 0 = Monday. */
export type HourCount = { date: string; source: string; weekday: number; hour: number; replies: number };

export type SourceInfo = { id: string; label: string; location: string; sessions: number };

export type Analytics = {
  days: DayModel[];
  hours: HourCount[];
  pricesAsOf: string;
  sessions: number;
  filesScanned: number;
  firstSeen: string | null;
  sources: SourceInfo[];
  /** Hermes is installed but switched off in Settings. */
  hermesAvailable: boolean;
};

export type Settings = {
  refreshSecs: number;
  alwaysOnTop: boolean;
  notify: boolean;
  notifyAt: number[];
  trayText: "both" | "session" | "weekly" | "none";
  includeHermes: boolean;
};

/** One recorded usage check, as kept by Rust in history.jsonl. */
export type HistoryPoint = {
  t: string;
  limits: { id: string; percent: number; resetsAt: string | null }[];
};

export const api = {
  state: () => invoke<UsageState>("get_state"),
  refresh: () => invoke<void>("refresh_now"),
  analytics: () => invoke<Analytics>("get_analytics"),
  history: () => invoke<HistoryPoint[]>("get_history"),
  hermesDetected: () => invoke<boolean>("hermes_detected"),
  settings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  openDetails: (tab?: string) => invoke<void>("open_details", { tab }),
  onUsage: (cb: (s: UsageState) => void) => listen<UsageState>("usage", (e) => cb(e.payload)),
};
