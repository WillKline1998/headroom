import { useEffect, useState } from "react";
import { api, type UsageState } from "./api";

/** Live usage state: initial fetch, then every update the Rust poller pushes. */
export function useUsage(): UsageState {
  const [state, setState] = useState<UsageState>({ snapshot: null, status: "loading", message: null, checkedAt: null });
  useEffect(() => {
    let alive = true;
    api.state().then((s) => alive && setState(s));
    const off = api.onUsage((s) => alive && setState(s));
    return () => {
      alive = false;
      off.then((f) => f());
    };
  }, []);
  return state;
}

/** Re-render on an interval so countdowns tick between network checks. */
export function useNow(everyMs = 15_000): Date {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const id = setInterval(() => setNow(new Date()), everyMs);
    return () => clearInterval(id);
  }, [everyMs]);
  return now;
}
