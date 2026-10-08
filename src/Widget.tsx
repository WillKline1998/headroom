import { useEffect, useRef } from "react";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { api } from "./api";
import { LimitBar } from "./components/LimitBar";
import { ago } from "./format";
import { useNow, useUsage } from "./hooks";

const WIDTH = 300;

/** The always-visible desktop widget. Drag it anywhere by its body. */
export function Widget() {
  const state = useUsage();
  const now = useNow();
  const ref = useRef<HTMLDivElement>(null);

  // Size the native window to the content, so new limits never get cut off.
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      getCurrentWindow().setSize(new LogicalSize(WIDTH, Math.ceil(el.getBoundingClientRect().height)));
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const s = state.snapshot;
  const plan = s?.plan ? s.plan[0].toUpperCase() + s.plan.slice(1) : null;

  return (
    <div className="widget" ref={ref} data-tauri-drag-region>
      <header className="widget-head" data-tauri-drag-region>
        <span className="widget-title" data-tauri-drag-region>
          Claude{plan && <span className="plan"> · {plan}</span>}
        </span>
        <span className="widget-tools">
          <button className="icon" onClick={() => api.refresh()} title="Refresh now" aria-label="Refresh now">
            ↻
          </button>
          <button className="icon" onClick={() => api.openDetails()} title="Details" aria-label="Open details">
            ⤢
          </button>
          <button className="icon" onClick={() => getCurrentWindow().hide()} title="Hide (it stays in the menu bar)" aria-label="Hide widget">
            ×
          </button>
        </span>
      </header>

      {state.status === "loading" && !s && <p className="widget-note">Checking your usage…</p>}

      {state.status === "signed_out" && (
        <div className="widget-note">
          <p>{state.message}</p>
          <button className="btn" onClick={() => api.refresh()}>Try again</button>
        </div>
      )}

      {s && (
        <div className="limits" data-tauri-drag-region>
          {s.limits.map((l) => (
            <LimitBar key={l.id} limit={l} now={now} />
          ))}
          {s.limits.length === 0 && <p className="widget-note">No limits reported for this account.</p>}
        </div>
      )}

      <footer className="widget-foot" data-tauri-drag-region>
        {state.status === "error" && state.message ? (
          <span className="warn-text" title={state.message}>⚠ {state.message}</span>
        ) : (
          state.checkedAt && <span>Updated {ago(new Date(state.checkedAt), now)}</span>
        )}
      </footer>
    </div>
  );
}
