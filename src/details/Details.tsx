import { useEffect, useState } from "react";
import { LimitsTab } from "./LimitsTab";
import { ModelsTab } from "./ModelsTab";
import { SettingsTab } from "./SettingsTab";

const TABS = { limits: "Limits", models: "Models", settings: "Settings" } as const;
type Tab = keyof typeof TABS;

const fromHash = (): Tab => {
  const t = window.location.hash.split("/")[1] as Tab;
  return t in TABS ? t : "limits";
};

/** The click-through window: full limits, model analytics, and settings. */
export function Details() {
  const [tab, setTab] = useState<Tab>(fromHash);
  useEffect(() => {
    const on = () => setTab(fromHash());
    window.addEventListener("hashchange", on);
    return () => window.removeEventListener("hashchange", on);
  }, []);
  const go = (t: Tab) => {
    window.location.hash = `details/${t}`;
  };

  return (
    <div className="details">
      <nav className="tabs" role="tablist">
        {(Object.keys(TABS) as Tab[]).map((t) => (
          <button key={t} role="tab" aria-selected={tab === t} className="tab" onClick={() => go(t)}>
            {TABS[t]}
          </button>
        ))}
      </nav>
      <main className="details-body">
        {tab === "limits" && <LimitsTab />}
        {tab === "models" && <ModelsTab />}
        {tab === "settings" && <SettingsTab />}
      </main>
    </div>
  );
}
