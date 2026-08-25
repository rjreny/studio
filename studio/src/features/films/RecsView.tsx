import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { tasteAnalyze, tasteGet } from "../../platform/filmLibrary";
import type { JobProgress, TastePick, TasteState } from "../../platform/types/film";
import { log } from "../../platform/log";
import { Poster } from "./Poster";

const DIM_LABEL: Record<string, string> = {
  genre: "Genre",
  era: "Era",
  director: "Director",
  performance: "Performance",
  image: "Image",
  intensity: "Intensity",
  motif: "Motif",
};

function formatWhen(iso: string | undefined) {
  if (!iso) return "";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  return date.toLocaleString(undefined, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" });
}

function pickId(pick: TastePick) {
  return pick.filmId || (pick.tmdbId ? `tmdb:${pick.tmdbId}` : null);
}

export function RecsView({
  onSelectFilm,
  onOpenSettings,
}: {
  onSelectFilm: (id: string) => void;
  onOpenSettings: () => void;
}) {
  const [state, setState] = useState<TasteState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void tasteGet()
      .then((next) => {
        if (!cancelled) setState(next);
      })
      .catch((err) => {
        log("warn", "taste load failed", err);
        if (!cancelled) setError("Could not read your log for Taste.");
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<JobProgress>("studio-job", (event) => {
      const next = event.payload;
      if (next.job !== "taste") return;
      if (!next.done) setRunning(true);
      if (next.done) {
        setRunning(false);
        if (next.errors) {
          setError(next.label.replace(/^taste failed · /i, "") || "Taste read failed");
          return;
        }
        if (next.taste) {
          setState((prev) =>
            prev
              ? { ...prev, report: next.taste ?? prev.report }
              : prev,
          );
          setError(null);
        } else {
          void tasteGet()
            .then(setState)
            .catch((err) => log("warn", "taste refresh failed", err));
        }
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  async function run() {
    try {
      setError(null);
      setRunning(true);
      await tasteAnalyze();
    } catch (err) {
      setRunning(false);
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      log("error", "taste analyze failed", err);
    }
  }

  const snapshot = state?.snapshot;
  const report = state?.report;
  const keyReady = Boolean(state?.key.stored && state.key.valid !== false);
  const enoughRatings = (snapshot?.ratedCount ?? 0) >= 8;

  return (
    <div className="recs page-pad">
      <header className="page-head">
        <div>
          <h1>{report?.title ?? "Taste"}</h1>
          <p className="muted">
            {report
              ? report.summary
              : snapshot
                ? `${snapshot.ratedCount} ratings · ${snapshot.lovedCount} loved · ${snapshot.hatedCount} disliked`
                : "The agent reads your whole log, then explains the pattern."}
          </p>
        </div>
        {keyReady && enoughRatings ? (
          <button type="button" className="primary" disabled={running} onClick={() => void run()}>
            {running ? "Reading…" : report ? "Read again" : "Read my log"}
          </button>
        ) : null}
      </header>

      {error ? <p className="taste-error">{error}</p> : null}

      {!keyReady && state ? (
        <section className="solid-card taste-setup">
          <h2>Pay as you go</h2>
          <p>
            Taste uses OpenRouter so you can pick DeepSeek (cheap default) or Kimi K3 (stronger, more
            expensive). Add a few dollars of credit, paste the key in Settings, then come back.
          </p>
          <button type="button" className="primary" onClick={onOpenSettings}>
            Open Settings
          </button>
        </section>
      ) : null}

      {keyReady && !enoughRatings && snapshot ? (
        <p className="muted pad">Rate at least 8 films so the agent has likes and dislikes to compare.</p>
      ) : null}

      {snapshot && (snapshot.genres.length || snapshot.directors.length) ? (
        <div className="taste-stats">
          {snapshot.genres.slice(0, 6).map((g) => (
            <span key={`g-${g.label}`} className="taste-chip">
              {g.label} · {g.avg.toFixed(1)}
            </span>
          ))}
          {snapshot.decades.slice(0, 3).map((d) => (
            <span key={`d-${d.label}`} className="taste-chip">
              {d.label}
            </span>
          ))}
        </div>
      ) : null}

      {report?.affinities.length || report?.aversions.length ? (
        <div className="taste-split">
          {report?.affinities.length ? (
            <section className="solid-card">
              <h2>You keep returning to</h2>
              <ul className="taste-notes">
                {report.affinities.map((item) => (
                  <li key={item.label}>
                    <strong>{item.label}</strong>
                    <span>{item.evidence}</span>
                  </li>
                ))}
              </ul>
            </section>
          ) : null}
          {report?.aversions.length ? (
            <section className="solid-card">
              <h2>You bounce off</h2>
              <ul className="taste-notes">
                {report.aversions.map((item) => (
                  <li key={item.label}>
                    <strong>{item.label}</strong>
                    <span>{item.evidence}</span>
                  </li>
                ))}
              </ul>
            </section>
          ) : null}
        </div>
      ) : null}

      {report?.dimensions.length ? (
        <section className="taste-dims">
          {report.dimensions.map((dim) => (
            <article key={dim.name}>
              <h3>{DIM_LABEL[dim.name] ?? dim.name}</h3>
              <p>{dim.take}</p>
            </article>
          ))}
        </section>
      ) : null}

      {report?.picks.length ? (
        <section>
          <div className="shelf-head">
            <h2>For you</h2>
            <span className="muted">
              {report.model}
              {report.generatedAt ? ` · ${formatWhen(report.generatedAt)}` : ""}
            </span>
          </div>
          <ul className="rec-list">
            {report.picks.map((pick) => {
              const id = pickId(pick);
              const body = (
                <>
                  <Poster name={pick.title} poster={pick.poster} large />
                  <div>
                    <strong>{pick.title}</strong>
                    <span className="muted">{pick.year ?? ""}</span>
                    {pick.why ? <p>{pick.why}</p> : null}
                    {pick.rhymesWith.length ? (
                      <p className="taste-rhyme">Close to {pick.rhymesWith.join(", ")}</p>
                    ) : null}
                  </div>
                </>
              );
              return (
                <li key={`${pick.title}-${pick.year ?? ""}`}>
                  {id ? (
                    <button type="button" className="taste-pick" onClick={() => onSelectFilm(id)}>
                      {body}
                    </button>
                  ) : (
                    <div className="taste-pick is-static">{body}</div>
                  )}
                </li>
              );
            })}
          </ul>
        </section>
      ) : null}

      {keyReady && enoughRatings && !report && !running ? (
        <p className="muted pad">
          The agent will take your 4.5+ loves, your 2.5-and-under dislikes, genres, decades, directors,
          actors, cinematographers, writers, runtime, overviews, watchlist, and friend loves, then rank
          films you have not logged.
        </p>
      ) : null}
    </div>
  );
}
