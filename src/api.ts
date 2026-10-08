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
};

export type Breakdown = { key: string; label: string; percent: number };

export type Snapshot = {
  provider: string;
  plan: string | null;
  limits: Limit[];
  breakdown: Breakdown[];
  breakdownSince: string | null;
  extraUsage: boolean;
  fetchedAt: string;
};

export type UsageState = {
  snapshot: Snapshot | null;
  status: "loading" | "ok" | "signed_out" | "error";
  message: string | null;
  checkedAt: string | null;
};

export type DayModel = {
  date: string;
  model: string;
  replies: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
};

export type Analytics = { days: DayModel[]; sessions: number; filesScanned: number; firstSeen: string | null; sources: string[] };

export type Settings = {
  refreshSecs: number;
  alwaysOnTop: boolean;
  notify: boolean;
  notifyAt: number[];
  trayText: "both" | "session" | "weekly" | "none";
};

export const api = {
  state: () => invoke<UsageState>("get_state"),
  refresh: () => invoke<void>("refresh_now"),
  analytics: () => invoke<Analytics>("get_analytics"),
  settings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  openDetails: (tab?: string) => invoke<void>("open_details", { tab }),
  onUsage: (cb: (s: UsageState) => void) => listen<UsageState>("usage", (e) => cb(e.payload)),
};
