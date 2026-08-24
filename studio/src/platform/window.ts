import { LogicalSize } from "@tauri-apps/api/dpi";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { log } from "./log";
import { getSetting, setSetting } from "./settings";

function hostWindow() {
  const internals = (window as unknown as { __TAURI_INTERNALS__?: { metadata?: { currentWindow?: unknown } } })
    .__TAURI_INTERNALS__;
  if (!internals?.metadata?.currentWindow) return null;
  return getCurrentWindow();
}

export function windowApi() {
  const win = hostWindow();
  return {
    minimize: () => win?.minimize() ?? Promise.resolve(),
    toggleMaximize: () => win?.toggleMaximize() ?? Promise.resolve(),
    close: () => win?.close() ?? Promise.resolve(),
    isMaximized: () => win?.isMaximized() ?? Promise.resolve(false),
    onResized: (cb: () => void) => win?.onResized(cb) ?? Promise.resolve(() => undefined),
  };
}

export async function restoreWindowBounds(): Promise<void> {
  const saved = await getSetting<{ width: number; height: number }>("windowSize");
  const win = hostWindow();
  if (!saved?.width || !saved?.height || !win) return;
  try {
    await win.setSize(new LogicalSize(saved.width, saved.height));
  } catch (err) {
    log("warn", "could not restore window size", err);
  }
}

export function persistWindowBounds(): Promise<() => void> {
  const win = hostWindow();
  if (!win) return Promise.resolve(() => undefined);
  let timer: number | undefined;
  return win.onResized(async () => {
    window.clearTimeout(timer);
    timer = window.setTimeout(async () => {
      try {
        const factor = await win.scaleFactor();
        const size = await win.innerSize();
        await setSetting("windowSize", {
          width: Math.round(size.width / factor),
          height: Math.round(size.height / factor),
        });
      } catch (err) {
        log("warn", "could not persist window size", err);
      }
    }, 400);
  });
}
