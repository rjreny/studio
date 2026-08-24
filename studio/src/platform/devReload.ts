import { log } from "./log";

export type DevUpdateSnapshot = {
  pending: boolean;
  frontend: boolean;
  rust: boolean;
};

const empty: DevUpdateSnapshot = { pending: false, frontend: false, rust: false };

function isTauriDev(): boolean {
  return import.meta.env.DEV && Boolean(
    (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__,
  );
}

export function devUpdatesEnabled(): boolean {
  return isTauriDev();
}

export async function fetchDevUpdateSnapshot(): Promise<DevUpdateSnapshot> {
  if (!isTauriDev()) return empty;
  try {
    const res = await fetch("/__studio/dev-update", { cache: "no-store" });
    if (!res.ok) return empty;
    return (await res.json()) as DevUpdateSnapshot;
  } catch (err) {
    log("warn", "dev update poll failed", err);
    return empty;
  }
}

export async function clearDevUpdateSnapshot(): Promise<void> {
  if (!isTauriDev()) return;
  try {
    await fetch("/__studio/dev-update/clear", { method: "POST" });
  } catch (err) {
    log("warn", "dev update clear failed", err);
  }
}

export function subscribeDevUpdates(onChange: (snap: DevUpdateSnapshot) => void): () => void {
  if (!isTauriDev()) return () => undefined;

  let stopped = false;
  const poll = async () => {
    if (stopped) return;
    const snap = await fetchDevUpdateSnapshot();
    onChange(snap);
  };

  void poll();
  const timer = window.setInterval(() => void poll(), 2000);

  const hot = import.meta.hot;
  if (hot) {
    hot.on("studio:update-available", (data: DevUpdateSnapshot) => {
      onChange(data);
    });
  }

  return () => {
    stopped = true;
    window.clearInterval(timer);
  };
}

export async function relaunchApp(): Promise<void> {
  const { relaunch } = await import("@tauri-apps/plugin-process");
  await relaunch();
}

export async function applyDevUpdate(snapshot: DevUpdateSnapshot): Promise<void> {
  await clearDevUpdateSnapshot();
  if (snapshot.rust) {
    await relaunchApp();
    return;
  }
  window.location.reload();
}
