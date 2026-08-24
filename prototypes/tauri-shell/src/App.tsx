import { LogicalSize } from "@tauri-apps/api/dpi";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { LazyStore } from "@tauri-apps/plugin-store";
import { useCallback, useEffect, useState } from "react";
import { TitleBar } from "./app/shell/TitleBar";
import { Sidebar } from "./app/shell/Sidebar";
import { CommandPalette } from "./components/CommandPalette";
import { ContextMenu } from "./components/ContextMenu";
import { LibraryView } from "./features/library/LibraryView";
import { SettingsView } from "./features/settings/SettingsView";
import type { Accent, Route, SortKey, Theme } from "./features/library/data";
import "./styles/fonts.css";
import "./styles/tokens.css";
import "./styles/globals.css";
import "./features/library/library.css";
import "./components/overlays.css";

const store = new LazyStore("studio.json");

function resolvedTheme(theme: Theme): "dark" | "light" {
  if (theme !== "system") return theme;
  return window.matchMedia("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}

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
  const [menu, setMenu] = useState<{ x: number; y: number; ids: number[] } | null>(
    null,
  );
  const [hydrated, setHydrated] = useState(false);

  useEffect(() => {
    document.documentElement.dataset.theme = resolvedTheme(theme);
    document.documentElement.dataset.accent = accent;
  }, [theme, accent]);

  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => {
      if (theme === "system") {
        document.documentElement.dataset.theme = resolvedTheme("system");
      }
    };
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, [theme]);

  useEffect(() => {
    void (async () => {
      const savedRoute = await store.get<Route>("route");
      const savedTheme = await store.get<Theme>("theme");
      const savedAccent = await store.get<Accent>("accent");
      const savedSort = await store.get<SortKey>("sort");
      const savedCollapsed = await store.get<boolean>("collapsed");
      if (savedRoute) setRoute(savedRoute);
      if (savedTheme) setTheme(savedTheme);
      if (savedAccent) setAccent(savedAccent);
      if (savedSort) setSort(savedSort);
      if (typeof savedCollapsed === "boolean") setCollapsed(savedCollapsed);
      const w = await store.get<number>("width");
      const h = await store.get<number>("height");
      if (w && h) {
        await getCurrentWindow().setSize(new LogicalSize(w, h));
      }
      setHydrated(true);
    })();
  }, []);

  useEffect(() => {
    if (!hydrated) return;
    void store.set("route", route);
    void store.set("theme", theme);
    void store.set("accent", accent);
    void store.set("sort", sort);
    void store.set("collapsed", collapsed);
    void store.save();
  }, [route, theme, accent, sort, collapsed, hydrated]);

  useEffect(() => {
    const win = getCurrentWindow();
    let t: number | undefined;
    let unlisten: (() => void) | undefined;
    void win.onResized(async () => {
      window.clearTimeout(t);
      t = window.setTimeout(async () => {
        const size = await win.innerSize();
        const factor = await win.scaleFactor();
        await store.set("width", Math.round(size.width / factor));
        await store.set("height", Math.round(size.height / factor));
        await store.save();
      }, 250);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
      window.clearTimeout(t);
    };
  }, []);

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
      if (e.key === "Escape") {
        if (closeTop()) {
          e.preventDefault();
          return;
        }
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
      if (meta && (e.key.toLowerCase() === "p" || (e.shiftKey && e.key.toLowerCase() === "p"))) {
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

  const listActive = route === "library" || route === "projects";

  return (
    <div className={`app${collapsed ? " is-collapsed" : ""}`}>
      <TitleBar collapsed={collapsed} />
      <div className={`body${collapsed ? " is-collapsed" : ""}`}>
        <Sidebar
          route={route}
          collapsed={collapsed}
          onNavigate={setRoute}
          onToggle={() => setCollapsed((v) => !v)}
        />
        <main className="canvas">
          <section className={`panel${route === "home" ? " is-active" : ""}`}>
            <div className="home">
              <h1>Studio bakeoff — Tauri</h1>
              <p>
                This is a stressful desktop-shell prototype, not the product. The
                Library holds 6,000 virtualized rows. Keyboard: Ctrl+K palette,
                Ctrl+, settings, Escape closes transients first.
              </p>
              <p className="muted">
                Snap Layouts on a custom Tauri titlebar are expected to fail
                without a Win32 overlay. Record that on the scorecard.
              </p>
            </div>
          </section>
          <section className={`panel${listActive ? " is-active" : ""}`}>
            <LibraryView
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
            <SettingsView
              theme={theme}
              accent={accent}
              onTheme={setTheme}
              onAccent={setAccent}
              onStatus={setStatus}
            />
          </section>
        </main>
      </div>
      <footer className="status">
        <span>{status}</span>
        <span>Tauri · WebView2</span>
      </footer>
      <CommandPalette
        open={palette}
        query={query}
        onQuery={setQuery}
        onClose={() => {
          setPalette(false);
          setQuery("");
        }}
        onNavigate={setRoute}
        index={palIndex}
        onIndex={setPalIndex}
      />
      {menu ? (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          ids={menu.ids}
          onClose={() => setMenu(null)}
          onAction={setStatus}
        />
      ) : null}
    </div>
  );
}
