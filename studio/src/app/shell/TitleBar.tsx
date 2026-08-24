import { useEffect, useState } from "react";
import { windowApi } from "../../platform/window";

export function TitleBar({ collapsed, title }: { collapsed: boolean; title: string }) {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    const win = windowApi();
    let unlisten: (() => void) | undefined;
    void win.isMaximized().then(setMaximized);
    void win.onResized(async () => setMaximized(await win.isMaximized())).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  const win = windowApi();

  return (
    <header className={`titlebar${collapsed ? " is-collapsed" : ""}`}>
      <div className="tb-brand" data-tauri-drag-region>
        <span className="tb-mark" data-tauri-drag-region />
        <span data-tauri-drag-region>Studio</span>
      </div>
      <div className="tb-drag" data-tauri-drag-region onDoubleClick={() => void win.toggleMaximize()}>
        {title}
      </div>
      <div className="tb-controls">
        <button type="button" className="tb-btn" title="Minimize" onClick={() => void win.minimize()}>
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
            <path fill="currentColor" d="M1 5h8v1H1z" />
          </svg>
        </button>
        <button type="button" className="tb-btn" title={maximized ? "Restore" : "Maximize"} onClick={() => void win.toggleMaximize()}>
          {maximized ? (
            <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
              <path fill="none" stroke="currentColor" d="M2.5 3.5h5v5h-5zM3.5 3.5V2h5v5H7" />
            </svg>
          ) : (
            <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
              <rect x="1.5" y="1.5" width="7" height="7" fill="none" stroke="currentColor" />
            </svg>
          )}
        </button>
        <button type="button" className="tb-btn close" title="Close" onClick={() => void win.close()}>
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
            <path
              fill="currentColor"
              d="M2.2 1.5 5 4.3 7.8 1.5 8.5 2.2 5.7 5l2.8 2.8-.7.7L5 5.7 2.2 8.5l-.7-.7L4.3 5 1.5 2.2z"
            />
          </svg>
        </button>
      </div>
    </header>
  );
}
