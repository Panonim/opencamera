import { check, type Update } from "@tauri-apps/plugin-updater";
import { create } from "zustand";

const LAST_CHECK_KEY = "lastUpdateCheck";
export const CHECK_INTERVAL_MS = 3 * 24 * 60 * 60 * 1000;

export const useUpdate = create<{ update: Update | null }>(() => ({ update: null }));

export const isCheckDue = (lastCheck: number, now: number) => now - lastCheck >= CHECK_INTERVAL_MS;

export async function checkForUpdate() {
  const update = await check();
  useUpdate.setState({ update });
  // Only a clean "no update" result pauses checks, so a pending update keeps showing after restarts.
  if (!update) localStorage.setItem(LAST_CHECK_KEY, String(Date.now()));
  return update;
}

export function startPeriodicUpdateCheck() {
  const tick = () => {
    if (useUpdate.getState().update || !isCheckDue(Number(localStorage.getItem(LAST_CHECK_KEY) ?? 0), Date.now())) return;
    checkForUpdate().catch(() => {});
  };
  tick();
  setInterval(tick, 60 * 60 * 1000);
}
