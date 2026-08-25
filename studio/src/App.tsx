import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { DevUpdateButton } from "./app/shell/DevUpdateButton";
import { NavTabs } from "./app/shell/NavTabs";
import { TitleBar } from "./app/shell/TitleBar";
import { ConnectView } from "./features/films/ConnectView";
import { FilmDetailView } from "./features/films/FilmDetailView";
import { FilmsView } from "./features/films/FilmsView";
import { FriendsView } from "./features/films/FriendsView";
import { HomeView } from "./features/films/HomeView";
import { RecsView } from "./features/films/RecsView";
import { StatsView } from "./features/films/StatsView";
import { SettingsView } from "./features/settings/SettingsView";
import { emptyLibrary, resolveTheme, type Accent, type Library, type Route, type Theme } from "./core/types";
import { appVersion } from "./platform/app";
import {
  getHome,
  getSession,
  migrateFromLegacy,
  setSelfUsername,
  tmdbEnrich,
} from "./platform/filmLibrary";
import type { AppSession, HomeViewModel, JobProgress, LibraryCoverage } from "./platform/types/film";
import { log } from "./platform/log";
import { getSetting, setSetting } from "./platform/settings";
import "./styles.css";
import "./materials.css";

const NAV: { id: Route; label: string }[] = [
  { id: "home", label: "Home" },
  { id: "films", label: "Films" },
  { id: "friends", label: "Friends" },
  { id: "stats", label: "Stats" },
  { id: "recs", label: "Taste" },
  { id: "settings", label: "Settings" },
];

export default function App() {
  const [route, setRoute] = useState<Route>("home");
  const [theme, setTheme] = useState<Theme>("dark");
  const [accent, setAccent] = useState<Accent>("app");
  const [username, setUsername] = useState("");
  const [coverage, setCoverage] = useState<LibraryCoverage | null>(null);
  const [home, setHome] = useState<HomeViewModel | null>(null);
  const [status, setStatus] = useState("Ready");
  const [version, setVersion] = useState("…");
  const [palette, setPalette] = useState(false);
  const [hydrated, setHydrated] = useState(false);
  const [selectedFilmId, setSelectedFilmId] = useState<string | null>(null);
  const [legacyLibrary, setLegacyLibrary] = useState<Library>(emptyLibrary);
  const [session, setSession] = useState<AppSession | null>(null);
  const [libraryEpoch, setLibraryEpoch] = useState(0);
  const [job, setJob] = useState<JobProgress | null>(null);
  const [libraryQuery, setLibraryQuery] = useState("");

  const refresh = useCallback(async () => {
    try {
      const [s, h] = await Promise.all([getSession(), getHome()]);
      setSession(s);
      setCoverage(s.coverage);
      setHome(h);
      setLibraryEpoch((n) => n + 1);
    } catch (err) {
      log("warn", "refresh failed", err);
    }
  }, []);

  useEffect(() => {
    document.documentElement.dataset.theme = resolveTheme(theme);
    document.documentElement.dataset.accent = accent;
  }, [theme, accent]);

  useEffect(() => {
    void (async () => {
      try {
        const [r, t, a, u, migrated, lib, v] = await Promise.all([
          getSetting<Route>("route"),
          getSetting<Theme>("theme"),
          getSetting<Accent>("accent"),
          getSetting<string>("username"),
          getSetting<boolean>("sqlite_migrated"),
          getSetting<Library>("library"),
          appVersion().catch(() => "dev"),
        ]);
        if (r && NAV.some((n) => n.id === r)) setRoute(r);
        if (t) setTheme(t);
        if (a) setAccent(a);
        if (lib) setLegacyLibrary({ ...emptyLibrary(), ...lib });
        setVersion(v);

        if (lib && !migrated && (lib.films?.length || lib.username)) {
          try {
            const result = await migrateFromLegacy(lib);
            if (result.status === "completed") {
              await setSetting("sqlite_migrated", true);
              setStatus(`Migrated legacy library · ${result.validationResult}`);
            }
          } catch (err) {
            log("warn", "legacy migration skipped", err);
          }
        }

        const loadedSession = await getSession();
        setSession(loadedSession);
        setCoverage(loadedSession.coverage);
        const resolvedUsername = loadedSession.selfUsername ?? u ?? "";
        if (resolvedUsername) setUsername(resolvedUsername);
        if (u && !loadedSession.selfUsername) {
          await setSelfUsername(u);
        } else if (loadedSession.selfUsername && loadedSession.selfUsername !== u) {
          await setSetting("username", loadedSession.selfUsername);
        }

        try {
          const h = await getHome();
          setHome(h);
        } catch (err) {
          log("warn", "home load failed", err);
        }

        void tmdbEnrich().catch((err) => log("warn", "poster enrich skipped", err));

        setHydrated(true);
        log("info", "shell hydrated");
      } catch (err) {
        log("error", "hydrate failed", err);
        setHydrated(true);
      }
    })();
  }, [refresh]);

  useEffect(() => {
    if (!hydrated) return;
    void setSetting("route", route);
    void setSetting("theme", theme);
    void setSetting("accent", accent);
    void setSetting("username", username);
    void setSelfUsername(username).catch((err) => log("warn", "username persist failed", err));
  }, [route, theme, accent, username, hydrated]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<JobProgress>("studio-job", (event) => {
      const next = event.payload;
      setJob(next.done ? null : next);
      setStatus(next.label);
      if (next.done || (next.current > 0 && next.current % 20 === 0)) {
        void refresh();
      }
      if (next.done && next.import) {
        void tmdbEnrich().catch((err) => log("warn", "poster enrich after import skipped", err));
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [refresh]);

  useEffect(() => {
    function onFocus() {
      void refresh();
    }
    window.addEventListener("focus", onFocus);
    const timer = window.setInterval(() => void refresh(), 60 * 60 * 1000);
    return () => {
      window.removeEventListener("focus", onFocus);
      window.clearInterval(timer);
    };
  }, [refresh]);

  const closePalette = useCallback(() => setPalette(false), []);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        if (selectedFilmId) {
          setSelectedFilmId(null);
          return;
        }
        if (palette) {
          e.preventDefault();
          closePalette();
        }
      }
      const meta = e.ctrlKey || e.metaKey;
      if (meta && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPalette(true);
      }
      if (meta && e.key === ",") {
        e.preventDefault();
        setRoute("settings");
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [palette, closePalette, selectedFilmId]);

  const connected = Boolean(session?.hasSetup);

  function go(id: Route) {
    setSelectedFilmId(null);
    setRoute(id);
  }

  function onNavSearch(value: string) {
    setLibraryQuery(value);
    if (route !== "films") {
      setSelectedFilmId(null);
      setRoute("films");
    }
  }

  return (
    <div className="app canvas-surface">
      <TitleBar />
      <div className="stage">
        <header className="cinema-nav">
          <div className="cinema-nav-inner">
            {selectedFilmId ? (
              <button type="button" className="nav-back glass" onClick={() => setSelectedFilmId(null)}>
                Back
              </button>
            ) : null}
            <NavTabs items={NAV} active={route} onGo={go} />
            <input
              className="nav-search glass"
              value={libraryQuery}
              onChange={(e) => onNavSearch(e.target.value)}
              placeholder="Search your log"
              spellCheck={false}
            />
          </div>
        </header>
        <main className="main-shell">
          <div className={`main-content${!connected && route === "home" ? " is-connect" : ""}`}>
            {!connected && route === "home" ? (
              <ConnectView
                username={username}
                onUsername={setUsername}
                onStatus={setStatus}
                onConnected={refresh}
              />
            ) : selectedFilmId ? (
              <FilmDetailView
                filmId={selectedFilmId}
                onBack={() => setSelectedFilmId(null)}
                onUpdated={refresh}
                onStatus={setStatus}
                onSelectFilm={setSelectedFilmId}
              />
            ) : (
              <>
                {route === "home" ? (
                  <HomeView
                    home={home}
                    onOpenFilms={() => setRoute("films")}
                    onOpenFriends={() => setRoute("friends")}
                    onSelectFilm={setSelectedFilmId}
                  />
                ) : null}
                {route === "films" ? (
                  <FilmsView
                    onSelectFilm={setSelectedFilmId}
                    onStatus={setStatus}
                    reloadToken={libraryEpoch}
                    query={libraryQuery}
                  />
                ) : null}
                {route === "friends" ? (
                  <FriendsView onStatus={setStatus} onRefresh={refresh} />
                ) : null}
                {route === "stats" ? <StatsView /> : null}
                {route === "recs" ? <RecsView library={legacyLibrary} /> : null}
                {route === "settings" ? (
                  <SettingsView
                    theme={theme}
                    accent={accent}
                    version={version}
                    username={username}
                    coverage={coverage}
                    onTheme={setTheme}
                    onAccent={setAccent}
                    onUsername={setUsername}
                    onStatus={setStatus}
                    onRefresh={refresh}
                  />
                ) : null}
              </>
            )}
          </div>
        </main>
      </div>
      {job && !job.done ? (
        <div className="job-toast glass" role="status">
          <div className="job-toast-copy">
            <strong>{job.label}</strong>
            <span>
              {job.total > 0 ? `${job.current}/${job.total}` : null}
              {job.posters ? ` · ${job.posters} posters` : ""}
            </span>
          </div>
          {job.total > 0 ? (
            <div className="job-toast-bar" aria-hidden>
              <span style={{ width: `${Math.min(100, Math.round((job.current / job.total) * 100))}%` }} />
            </div>
          ) : null}
        </div>
      ) : null}
      <footer className="status">
        <span>{status}</span>
        <div className="status-actions">
          <DevUpdateButton />
          <span>{username ? `@${username}` : "Studio"}</span>
        </div>
      </footer>
      {palette ? (
        <div className="overlay" onMouseDown={closePalette}>
          <div className="palette glass" onMouseDown={(e) => e.stopPropagation()}>
            <input autoFocus placeholder="Search Studio" readOnly />
            {NAV.map((item) => (
              <button
                key={item.id}
                type="button"
                onClick={() => {
                  go(item.id);
                  closePalette();
                }}
              >
                Go to {item.label}
              </button>
            ))}
          </div>
        </div>
      ) : null}
    </div>
  );
}
