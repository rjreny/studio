import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { tasteAnalyze, tasteGet, tasteSetModel, tasteSetWeb } from "../../platform/filmLibrary";
import type { JobProgress, TasteModelInfo, TastePick, TasteState } from "../../platform/types/film";
import { log } from "../../platform/log";
import { Poster } from "./Poster";

const DIM_LABEL: Record<string, string> = {
  visual: "Visual",
  story: "Story",
  intensity: "Intensity",
  comedy: "Comedy",
  spectacle: "Spectacle",
  atmosphere: "Atmosphere",
  comfort: "Comfort",
  genre: "Genre",
  era: "Era",
  director: "Director",
  performance: "Performance",
  image: "Image",
  motif: "Motif",
};

const RUN_STEPS = [
  "Reading your log",
  "Scoring candidates",
  "Critiquing the shortlist",
  "Targeted discovery",
  "Final 12",
  "Matching posters",
];

function formatWhen(iso: string | undefined) {
  if (!iso) return "";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  return date.toLocaleString(undefined, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" });
}

function formatElapsed(seconds: number) {
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return m > 0 ? `${m}:${String(s).padStart(2, "0")}` : `${s}s`;
}

function pickId(pick: TastePick) {
  return pick.filmId || (pick.tmdbId ? `tmdb:${pick.tmdbId}` : null);
}

function waitHint(_model: string | undefined) {
  return "Usually under a minute. If OpenRouter blocks a model, Taste retries without web search, then with Llama 3.3 or DeepSeek V3.";
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
  const [job, setJob] = useState<JobProgress | null>(null);
  const [elapsed, setElapsed] = useState(0);

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
      if (!next.done) {
        setRunning(true);
        setJob(next);
        return;
      }
      setRunning(false);
      setJob(null);
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
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  useEffect(() => {
    if (!running) {
      setElapsed(0);
      return;
    }
    const started = Date.now();
    const id = window.setInterval(() => {
      setElapsed(Math.floor((Date.now() - started) / 1000));
    }, 1000);
    return () => window.clearInterval(id);
  }, [running]);

  async function run() {
    try {
      setError(null);
      setRunning(true);
      setJob({
        job: "taste",
        label: "Reading your log…",
        current: 1,
        total: 3,
        posters: 0,
        errors: 0,
        done: false,
      });
      await tasteAnalyze();
    } catch (err) {
      setRunning(false);
      setJob(null);
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      log("error", "taste analyze failed", err);
    }
  }

  async function pickModel(id: string) {
    try {
      const key = await tasteSetModel(id);
      setState((prev) => (prev ? { ...prev, key } : prev));
    } catch (err) {
      log("warn", "taste model save failed", err);
    }
  }

  async function toggleWeb(enabled: boolean) {
    try {
      const key = await tasteSetWeb(enabled);
      setState((prev) => (prev ? { ...prev, key } : prev));
    } catch (err) {
      log("warn", "taste web save failed", err);
    }
  }

  const snapshot = state?.snapshot;
  const report = state?.report;
  const key = state?.key;
  const keyReady = Boolean(key?.stored && key.valid !== false);
  const enoughRatings = (snapshot?.ratedCount ?? 0) >= 8;
  const models = key?.models?.length ? key.models : [];
  const step = job?.current ?? 1;

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
                : "The agent scores a candidate universe from your full log, then reasons over a shortlist."}
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
            Taste uses OpenRouter. Llama 3.3 70B is the cheap default from models your key can
            actually reach. Add a few dollars of credit, paste the key in Settings, then come back.
          </p>
          <button type="button" className="primary" onClick={onOpenSettings}>
            Open Settings
          </button>
        </section>
      ) : null}

      {keyReady && !enoughRatings && snapshot ? (
        <p className="muted pad">Rate at least 8 films so the agent has likes and dislikes to compare.</p>
      ) : null}

      {keyReady ? (
        <section className="solid-card taste-controls">
          <h2>How it reads</h2>
          <TasteModelList
            models={models}
            selected={key?.model ?? "llama"}
            disabled={running}
            onPick={(id) => void pickModel(id)}
          />
          <div className="taste-web">
            <button
              type="button"
              className={key?.web ? "is-on" : ""}
              disabled={running}
              onClick={() => void toggleWeb(!(key?.web ?? true))}
            >
              {key?.web ? "Web search on" : "Web search off"}
            </button>
            <p className="muted">
              A few cheap lookups for critic lists. Pennies per run, not dollars. No model swarm.
            </p>
          </div>
        </section>
      ) : null}

      {running ? (
        <section className="solid-card taste-run" aria-live="polite">
          <div className="taste-run-head">
            <strong>{job?.label ?? "Reading your log…"}</strong>
            <span className="muted taste-elapsed">{formatElapsed(elapsed)}</span>
          </div>
          <ol>
            {RUN_STEPS.map((label, index) => {
              const n = index + 1;
              const cls = n < step ? "is-done" : n === step ? "is-now" : "";
              return (
                <li key={label} className={cls}>
                  <span>{n}</span>
                  {label}
                </li>
              );
            })}
          </ol>
          <p className="muted">{waitHint(key?.model)}</p>
        </section>
      ) : null}

      {report?.note ? <p className="hint">{report.note}</p> : null}

      {snapshot && (snapshot.genres.length || snapshot.directors.length || snapshot.cinematographers?.length) ? (
        <div className="taste-stats">
          {snapshot.genres.slice(0, 6).map((g) => (
            <span key={`g-${g.label}`} className="taste-chip">
              {g.label}
              {g.affinity != null ? ` · ${g.affinity >= 0 ? "+" : ""}${g.affinity.toFixed(2)}` : ` · ${g.avg.toFixed(1)}`}
            </span>
          ))}
          {(snapshot.cinematographers ?? []).slice(0, 4).map((c) => (
            <span key={`c-${c.label}`} className="taste-chip">
              {c.label}
              {c.affinity != null ? ` · ${c.affinity >= 0 ? "+" : ""}${c.affinity.toFixed(2)}` : ""}
            </span>
          ))}
          {snapshot.decades.slice(0, 2).map((d) => (
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
              {report.webUsed ? " · web" : ""}
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
                    {pick.mode ? <p className="muted">{pick.mode}</p> : null}
                    {pick.reasons?.length ? (
                      <p className="taste-rhyme">{pick.reasons.join(" · ")}</p>
                    ) : null}
                    {pick.evidence?.length ? (
                      <p className="taste-rhyme">Close to {pick.evidence.join(", ")}</p>
                    ) : pick.rhymesWith?.length ? (
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
          The scorer reads every rating, then separates films you like from films that should pull new recommendations.
          Rewatches stay in the profile; they do not turn a decade of nostalgia into a hunt target. The model critiques a
          scored shortlist across taste modes, may run a few targeted searches, and picks 12.
        </p>
      ) : null}
    </div>
  );
}

export function TasteModelList({
  models,
  selected,
  disabled,
  onPick,
}: {
  models: TasteModelInfo[];
  selected: string;
  disabled?: boolean;
  onPick: (id: string) => void;
}) {
  if (!models.length) return null;
  return (
    <div className="taste-model-list">
      {models.map((item) => (
        <button
          key={item.id}
          type="button"
          className={selected === item.id ? "is-on" : ""}
          disabled={disabled}
          onClick={() => onPick(item.id)}
        >
          <strong>
            {item.label}
            <span>{item.context} · {item.cost}</span>
          </strong>
          <span>{item.blurb}</span>
        </button>
      ))}
    </div>
  );
}
