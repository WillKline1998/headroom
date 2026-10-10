import { useEffect, useState } from "react";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, type Settings } from "../api";

const INTERVALS = [60, 180, 300, 600, 900];
const REPO = "https://github.com/WillKline1998/headroom";
const CREDITS = `${REPO}/blob/main/THIRD_PARTY_LICENSES.md`;

export function SettingsTab() {
  const [s, setS] = useState<Settings | null>(null);
  const [autostart, setAutostart] = useState(false);
  const [saved, setSaved] = useState(false);
  const [hermes, setHermes] = useState(false);

  useEffect(() => {
    api.settings().then(setS);
    api.hermesDetected().then(setHermes).catch(() => {});
    isEnabled().then(setAutostart).catch(() => {});
  }, []);

  if (!s) return <p className="muted">Loading…</p>;

  const update = async (patch: Partial<Settings>) => {
    const next = await api.saveSettings({ ...s, ...patch });
    setS(next);
    setSaved(true);
    setTimeout(() => setSaved(false), 1500);
  };
  const toggleThreshold = (t: number) =>
    update({ notifyAt: s.notifyAt.includes(t) ? s.notifyAt.filter((x) => x !== t) : [...s.notifyAt, t] });

  return (
    <section className="settings">
      <h1>Settings {saved && <span className="saved">Saved ✓</span>}</h1>

      <label className="row">
        <span>Check for new numbers every</span>
        <select value={s.refreshSecs} onChange={(e) => update({ refreshSecs: Number(e.target.value) })}>
          {INTERVALS.map((i) => <option key={i} value={i}>{i / 60} min</option>)}
        </select>
      </label>
      <p className="hint">Countdowns tick on their own in between. Anthropic rate-limits this endpoint, so a few minutes is plenty.</p>

      <label className="row">
        <span>Keep the widget above other windows</span>
        <input type="checkbox" checked={s.alwaysOnTop} onChange={(e) => update({ alwaysOnTop: e.target.checked })} />
      </label>

      <label className="row">
        <span>Start Headroom when I log in</span>
        <input
          type="checkbox"
          checked={autostart}
          onChange={async (e) => {
            await (e.target.checked ? enable() : disable());
            setAutostart(await isEnabled());
          }}
        />
      </label>

      <label className="row">
        <span>Menu bar shows</span>
        <select value={s.trayText} onChange={(e) => update({ trayText: e.target.value as Settings["trayText"] })}>
          <option value="both">Session · Weekly</option>
          <option value="session">Session only</option>
          <option value="weekly">Weekly only</option>
          <option value="none">Icon only</option>
        </select>
      </label>
      <p className="hint">macOS only. On Windows and Linux, hover the tray icon to see the numbers.</p>

      <label className="row">
        <span>Notify me when a limit passes…</span>
        <input type="checkbox" checked={s.notify} onChange={(e) => update({ notify: e.target.checked })} />
      </label>
      <div className="pills" aria-label="Alert thresholds">
        {[50, 80, 90, 95].map((t) => (
          <button key={t} className="pill" aria-pressed={s.notifyAt.includes(t)} disabled={!s.notify} onClick={() => toggleThreshold(t)}>
            {t}%
          </button>
        ))}
      </div>

      {hermes && (
        <>
          <label className="row">
            <span>Include Hermes Agent usage in Models</span>
            <input type="checkbox" checked={s.includeHermes} onChange={(e) => update({ includeHermes: e.target.checked })} />
          </label>
          <p className="hint">Hermes Agent is installed here. When Hermes uses your Claude account, its replies count toward the same limits as Claude Code. Headroom reads only token counts and times from its database, never your messages.</p>
        </>
      )}

      <h2>About</h2>
      <p className="hint">
        Headroom reads the sign-in Claude Code already saved on this computer and asks Anthropic for the same numbers shown on Claude&apos;s Settings → Usage page.
        It never sees your password and never sends data anywhere else. That usage endpoint isn&apos;t officially documented, so a future Claude update could change it.
      </p>
      <p className="hint">
        Unofficial; not affiliated with Anthropic. <a href={REPO} onClick={(e) => { e.preventDefault(); openUrl(REPO); }}>Source on GitHub</a>
        {" · "}
        <a href={CREDITS} onClick={(e) => { e.preventDefault(); openUrl(CREDITS); }}>Open-source credits</a>
      </p>
    </section>
  );
}
