import { LogicalPosition, LogicalSize } from "@tauri-apps/api/dpi";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { log } from "./log";
import { getSetting, setSetting } from "./settings";
import { nextWindowLayout, type WindowLayout } from "./windowLayout";

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

async function readLayout(): Promise<WindowLayout | undefined> {
  const saved = await getSetting<WindowLayout>("windowLayout");
  if (saved?.width && saved?.height) return saved;
  const legacy = await getSetting<{ width: number; height: number }>("windowSize");
  if (!legacy?.width || !legacy?.height) return undefined;
  return { x: 80, y: 40, width: legacy.width, height: legacy.height, maximized: false };
}

async function currentLayout(): Promise<WindowLayout | null> {
  const win = hostWindow();
  if (!win) return null;
  const [factor, position, size, maximized] = await Promise.all([
    win.scaleFactor(),
    win.outerPosition(),
    win.outerSize(),
    win.isMaximized(),
  ]);
  return {
    x: Math.round(position.x / factor),
    y: Math.round(position.y / factor),
    width: Math.round(size.width / factor),
    height: Math.round(size.height / factor),
    maximized,
  };
}

export async function restoreWindowBounds(): Promise<void> {
  const saved = await readLayout();
  const win = hostWindow();
  if (!saved || !win) return;
  try {
    if (saved.x > -10_000 && saved.y > -10_000) {
      await win.setPosition(new LogicalPosition(saved.x, saved.y));
    }
    await win.setSize(new LogicalSize(saved.width, saved.height));
    if (saved.maximized) await win.maximize();
  } catch (err) {
    log("warn", "could not restore window layout", err);
  }
}

export function persistWindowBounds(): Promise<() => void> {
  const win = hostWindow();
  if (!win) return Promise.resolve(() => undefined);
  let timer: number | undefined;
  let previous: WindowLayout | undefined;

  async function save() {
    const next = await currentLayout();
    if (!next) return;
    const saved = nextWindowLayout(previous, next);
    previous = saved;
    await setSetting("windowLayout", saved);
  }

  const schedule = () => {
    window.clearTimeout(timer);
    timer = window.setTimeout(() => {
      void save().catch((err) => log("warn", "could not persist window layout", err));
    }, 400);
  };

  return Promise.all([win.onResized(schedule), win.onMoved(schedule)]).then((fns) => {
    return () => {
      window.clearTimeout(timer);
      fns.forEach((fn) => fn());
    };
  });
}
