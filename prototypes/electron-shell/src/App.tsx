import { useCallback, useEffect, useState } from "react";
import { Catalog } from "./Catalog";
import {
  formatBytes,
  resolveTheme,
  type Accent,
  type Route,
  type SortKey,
  type Theme,
} from "./items";
import "./styles.css";

const NAV: { id: Route; label: string }[] = [
  { id: "home", label: "Home" },
  { id: "library", label: "Library" },
  { id: "projects", label: "Projects" },
  { id: "settings", label: "Settings" },
];

export default function App() {
  const [route, setRoute] = useState<Route>("library");
  const [theme, setTheme] = useState<Theme>("dark");
  const [accent, setAccent] = useState<Accent>("app");
  const [collapsed, setCollapsed] = useState(false);
  const [sort, setSort] = useState<SortKey>("name");
  const [search, setSearch] = useState("");
  const [selected, setSelected] = useState<number[]>([]);
  const [status, setStatus] = useState("Ready");
  const [palette, setPalette] = useState(false);
  const [query, setQuery] = useState("");
  const [palIndex, setPalIndex] = useState(0);
  const [menu, setMenu] = useState<{ x: number; y: number; ids: number[] } | null>(null);
  const [version, setVersion] = useState("…");
  const [file, setFile] = useState("No file opened");
  const [updateNote, setUpdateNote] = useState("Not checked");
  const [hydrated, setHydrated] = useState(false);

  const applied = resolveTheme(theme);

  useEffect(() => {
    document.documentElement.dataset.theme = applied;
    document.documentElement.dataset.accent = accent;
    void window.studio.setOverlayTheme(applied);
  }, [applied, accent]);

  useEffect(() => {
    void (async () => {
      const [r, t, a, s, c, v] = await Promise.all([
        window.studio.getSetting("route"),
        window.studio.getSetting("theme"),
        window.studio.getSetting("accent"),
        window.studio.getSetting("sort"),
        window.studio.getSetting("collapsed"),
        window.studio.version(),
      ]);
      if (typeof r === "string") setRoute(r as Route);
      if (typeof t === "string") setTheme(t as Theme);
      if (typeof a === "string") setAccent(a as Accent);
      if (typeof s === "string") setSort(s as SortKey);
      if (typeof c === "boolean") setCollapsed(c);
      setVersion(v);
      setHydrated(true);
    })();
  }, []);

  useEffect(() => {
    if (!hydrated) return;
    void window.studio.setSetting("route", route);
    void window.studio.setSetting("theme", theme);
    void window.studio.setSetting("accent", accent);
    void window.studio.setSetting("sort", sort);
    void window.studio.setSetting("collapsed", collapsed);
  }, [route, theme, accent, sort, collapsed, hydrated]);

  const closeTop = useCallback(() => {
    if (menu) {
      setMenu(null);
      return true;
    }
    if (palette) {
      setPalette(false);
      setQuery("");
      return true;
    }
    return false;
  }, [menu, palette]);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape" && closeTop()) {
        e.preventDefault();
        return;
      }
      const meta = e.ctrlKey || e.metaKey;
      if (meta && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPalette(true);
        setPalIndex(0);
      }
      if (meta && e.key === ",") {
        e.preventDefault();
        setRoute("settings");
      }
      if (meta && e.key.toLowerCase() === "p") {
        e.preventDefault();
        setPalette(true);
      }
      if (meta && e.key === "1") setRoute("home");
      if (meta && e.key === "2") setRoute("library");
      if (meta && e.key === "3") setRoute("projects");
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [closeTop]);

  const commands = [
    { label: "Go to Home", hint: "Ctrl+1", run: () => setRoute("home") },
    { label: "Go to Library", hint: "Ctrl+2", run: () => setRoute("library") },
    { label: "Go to Projects", hint: "Ctrl+3", run: () => setRoute("projects") },
    { label: "Open Settings", hint: "Ctrl+,", run: () => setRoute("settings") },
  ].filter((c) => c.label.toLowerCase().includes(query.trim().toLowerCase()));

  const listActive = route === "library" || route === "projects";

  return (
    <div className="app">
      <header className="titlebar">
        <span className="tb-mark" />
        <span className="tb-name">Studio</span>
        <span className="tb-ws">Workspace</span>
      </header>
      <div className={`body${collapsed ? " is-collapsed" : ""}`}>
        <nav className="sidebar" aria-label="Main">
          <div className="nav">
            {NAV.map((item) => (
              <button
                key={item.id}
                type="button"
                className={`nav-item${route === item.id ? " is-active" : ""}`}
                onClick={() => setRoute(item.id)}
              >
                <span>{item.label}</span>
              </button>
            ))}
          </div>
          <div className="sidebar-foot">
            <button type="button" className="icon-btn" onClick={() => setCollapsed((v) => !v)}>
              {collapsed ? "›" : "‹"}
            </button>
          </div>
        </nav>
        <main className="canvas">
          <section className={`panel${route === "home" ? " is-active" : ""}`}>
            <div className="home">
              <h1>Studio bakeoff — Electron</h1>
              <p>
                Same stressful shell as the Tauri prototype, implemented with
                Electron APIs: hidden title bar plus native titleBarOverlay
                (Windows caption buttons and Snap Layouts).
              </p>
              <p className="muted">
                Keyboard: Ctrl+K palette, Ctrl+, settings, Escape closes
                transients first.
              </p>
            </div>
          </section>
          <section className={`panel${listActive ? " is-active" : ""}`}>
            <Catalog
              visible={listActive}
              mode={route === "projects" ? "projects" : "library"}
              sort={sort}
              onSort={setSort}
              search={search}
              onSearch={setSearch}
              selected={selected}
              onSelected={setSelected}
              onStatus={setStatus}
              onContext={(x, y, ids) => setMenu({ x, y, ids })}
            />
          </section>
          <section className={`panel${route === "settings" ? " is-active" : ""}`}>
            <div className="settings">
              <h1>Settings</h1>
              <div className="setting-row">
                <label>Theme</label>
                <div className="seg">
                  {(["system", "dark", "light"] as const).map((t) => (
                    <button key={t} type="button" className={theme === t ? "is-on" : ""} onClick={() => setTheme(t)}>
                      {t[0].toUpperCase() + t.slice(1)}
                    </button>
                  ))}
                </div>
              </div>
              <div className="setting-row">
                <label>Accent</label>
                <div className="seg">
                  {(["app", "system"] as const).map((a) => (
                    <button key={a} type="button" className={accent === a ? "is-on" : ""} onClick={() => setAccent(a)}>
                      {a === "app" ? "App" : "System"}
                    </button>
                  ))}
                </div>
              </div>
              <div className="setting-row">
                <label>Version</label>
                <span>{version}</span>
              </div>
              <div className="setting-row">
                <label>Native file</label>
                <button
                  type="button"
                  className="primary"
                  onClick={async () => {
                    const info = await window.studio.openFile();
                    if (!info) return;
                    const line = `${info.name} (${formatBytes(info.bytes)})`;
                    setFile(line);
                    setStatus(`Opened ${line}`);
                  }}
                >
                  Open file…
                </button>
              </div>
              <p className="file-result">{file}</p>
              <div className="setting-row">
                <label>Updater</label>
                <button
                  type="button"
                  className="primary"
                  onClick={async () => {
                    try {
                      const res = await fetch(
                        "https://github.com/local/studio/releases/latest/download/RELEASES",
                      );
                      setUpdateNote(`HTTP ${res.status} — bakeoff smoke, no updater pipeline`);
                    } catch (err) {
                      setUpdateNote(`Fetch failed (${String(err)}) — expected in bakeoff`);
                    }
                  }}
                >
                  Check for updates
                </button>
              </div>
              <p className="file-result">{updateNote}</p>
            </div>
          </section>
        </main>
      </div>
      <footer className="status">
        <span>{status}</span>
        <span>Electron · Chromium</span>
      </footer>
      {palette ? (
        <div className="overlay" onMouseDown={() => { setPalette(false); setQuery(""); }}>
          <div className="palette" onMouseDown={(e) => e.stopPropagation()}>
            <input
              autoFocus
              value={query}
              placeholder="Type a command"
              onChange={(e) => { setQuery(e.target.value); setPalIndex(0); }}
              onKeyDown={(e) => {
                if (e.key === "ArrowDown") {
                  e.preventDefault();
                  setPalIndex((i) => Math.min(commands.length - 1, i + 1));
                }
                if (e.key === "ArrowUp") {
                  e.preventDefault();
                  setPalIndex((i) => Math.max(0, i - 1));
                }
                if (e.key === "Enter") {
                  commands[palIndex]?.run();
                  setPalette(false);
                  setQuery("");
                }
              }}
            />
            <ul>
              {commands.map((c, i) => (
                <li key={c.label}>
                  <button type="button" className={i === palIndex ? "is-on" : ""} onClick={() => { c.run(); setPalette(false); setQuery(""); }}>
                    <span>{c.label}</span>
                    <span className="muted">{c.hint}</span>
                  </button>
                </li>
              ))}
            </ul>
          </div>
        </div>
      ) : null}
      {menu ? (
        <div className="overlay" onMouseDown={() => setMenu(null)}>
          <ul className="menu" style={{ top: menu.y, left: menu.x }} onMouseDown={(e) => e.stopPropagation()}>
            <li>
              <button type="button" onClick={() => { setStatus(`Reveal ${menu.ids.length} item(s)`); setMenu(null); }}>
                Reveal in explorer
              </button>
            </li>
            <li>
              <button type="button" onClick={() => { setStatus("Stub: duplicate"); setMenu(null); }}>
                Duplicate
              </button>
            </li>
          </ul>
        </div>
      ) : null}
    </div>
  );
}
