import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { tasteAnalyze, tasteFeedbackSet, tasteGet } from "../../platform/filmLibrary";
import type { JobProgress, TasteFeedback, TasteModelInfo, TastePick, TasteState } from "../../platform/types/film";
import { log } from "../../platform/log";
import { Menu } from "../ui/Menu";
import { Poster } from "./Poster";

const RUN_STEPS = [
  "Reading your log",
  "Scoring candidates",
  "Critiquing the shortlist",
  "Targeted discovery",
  "Taste profile",
  "Matching posters",
];

function formatElapsed(seconds: number) {
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return m > 0 ? `${m}:${String(s).padStart(2, "0")}` : `${s}s`;
}

function pickId(pick: TastePick) {
  return pick.filmId || (pick.tmdbId ? `tmdb:${pick.tmdbId}` : null);
}

function pickKey(pick: TastePick) {
  if (pick.tmdbId) return `tmdb:${pick.tmdbId}`;
  if (pick.filmId) return pick.filmId;
  return `${pick.title}-${pick.year ?? ""}`;
}

function sectionPicks(report: NonNullable<TasteState["report"]>) {
  const hasSections = Array.isArray(report.newPicks) || Array.isArray(report.watchlistPicks);
  if (hasSections) {
    return {
      neu: [...(report.newPicks ?? []), ...(report.explorePicks ?? [])],
      watch: report.watchlistPicks ?? [],
    };
  }
  return { neu: report.picks ?? [], watch: [] as TastePick[] };
}

function waitHint() {
  return "Usually under a minute. If OpenRouter blocks a model, Taste retries without web search, then with Gemini 3.7 Flash.";
}

function likedIds(feedback: TasteFeedback[] | undefined) {
  return new Set(
    (feedback ?? []).filter((row) => row.action === "interested").map((row) => row.tmdbId),
  );
}

type TasteSort = "match" | "title" | "year";
type TasteFilter = "all" | "50" | "60" | "70";

const TASTE_SORTS: { id: TasteSort; label: string }[] = [
  { id: "match", label: "Highest match" },
  { id: "title", label: "Title A–Z" },
  { id: "year", label: "Newest" },
];

const TASTE_FILTERS: { id: TasteFilter; label: string }[] = [
  { id: "all", label: "Any score" },
  { id: "70", label: "70%+" },
  { id: "60", label: "60%+" },
  { id: "50", label: "50%+" },
];

function matchValue(pick: TastePick) {
  return typeof pick.matchScore === "number" && Number.isFinite(pick.matchScore)
    ? pick.matchScore
    : -1;
}

function matchPercent(pick: TastePick) {
  const score = matchValue(pick);
  return score >= 0 ? `${Math.round(score)}% match` : "—% match";
}

function matchTone(pick: TastePick) {
  const score = matchValue(pick);
  if (score >= 70) return "is-high";
  if (score >= 60) return "is-mid";
  return score >= 0 ? "is-low" : "is-unknown";
}

function sortTastePicks(picks: TastePick[], sort: TasteSort, filter: TasteFilter) {
  return picks
    .filter((pick) => filter === "all" || matchValue(pick) >= Number(filter))
    .sort((a, b) => {
      if (sort === "title") return a.title.localeCompare(b.title);
      if (sort === "year") {
        return (b.year ?? -Infinity) - (a.year ?? -Infinity);
      }
      return matchValue(b) - matchValue(a);
    });
}

function listCount(shown: number, total: number) {
  return shown === total ? String(shown) : `${shown} of ${total}`;
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
  const [hidden, setHidden] = useState<Set<string>>(new Set());
  const [interested, setInterested] = useState<Set<number>>(new Set());

  useEffect(() => {
    let cancelled = false;
    void tasteGet()
      .then((next) => {
        if (!cancelled) {
          setState(next);
          setInterested(likedIds(next.feedback));
        }
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
      void tasteGet()
        .then((loaded) => {
          setState(loaded);
          setInterested(likedIds(loaded.feedback));
          setHidden(new Set());
          setError(null);
        })
        .catch((err) => log("warn", "taste refresh failed", err));
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

  async function run(forceRefresh = false) {
    try {
      setError(null);
      setRunning(true);
      setHidden(new Set());
      setJob({
        job: "taste",
        label: forceRefresh ? "Refreshing metadata…" : "Reading your log…",
        current: 1,
        total: 3,
        posters: 0,
        errors: 0,
        done: false,
      });
      await tasteAnalyze(forceRefresh);
    } catch (err) {
      setRunning(false);
      setJob(null);
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      log("error", "taste analyze failed", err);
    }
  }

  async function sendFeedback(pick: TastePick, action: "interested" | "rejected" | "seen") {
    const id = pick.tmdbId;
    if (!id) {
      setError("This title is missing a TMDB id, so feedback cannot be saved.");
      return;
    }
    const key = pickKey(pick);
    const prevHidden = new Set(hidden);
    const prevLiked = new Set(interested);
    if (action === "interested") {
      setInterested(new Set(prevLiked).add(id));
    } else {
      setHidden(new Set(prevHidden).add(key));
    }
    try {
      await tasteFeedbackSet(id, action);
      const next = await tasteGet();
      setState(next);
      setInterested(likedIds(next.feedback));
      setHidden(new Set());
    } catch (err) {
      setHidden(prevHidden);
      setInterested(prevLiked);
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
    }
  }

  const snapshot = state?.snapshot;
  const report = state?.report;
  const key = state?.key;
  const keyReady = Boolean(key?.stored && key.valid !== false);
  const enoughRatings = (snapshot?.ratedCount ?? 0) >= 8;
  const step = job?.current ?? 1;

  return (
    <div className="recs page-pad">
      <header className="page-head">
        <div>
          <h1>{report?.title ?? "Taste"}</h1>
          <p className="muted">
            {snapshot
              ? `${report?.ratedCount ?? snapshot.ratedCount} ratings · ${snapshot.lovedCount} loved · ${snapshot.hatedCount} disliked`
              : "Find new films from your imported history."}
          </p>
        </div>
        {keyReady && enoughRatings ? (
          <div className="taste-actions">
            <button type="button" className="primary" disabled={running} onClick={() => void run()}>
              {running ? "Reading…" : report ? "Read again" : "Read my log"}
            </button>
            {report ? (
              <button
                type="button"
                className="taste-refresh"
                aria-label="Refresh recommendation metadata"
                title="Refresh recommendation metadata"
                disabled={running}
                onClick={() => void run(true)}
              >
                <svg viewBox="0 0 24 24" aria-hidden="true">
                  <path d="M20 11a8 8 0 1 0 2 5.2" />
                  <path d="M20 4v7h-7" />
                </svg>
              </button>
            ) : null}
          </div>
        ) : null}
      </header>

      {error ? <p className="taste-error">{error}</p> : null}

      {!keyReady && state ? (
        <section className="taste-setup">
          <h2>Pay as you go</h2>
          <p>
            Taste uses OpenRouter. DeepSeek V4 Pro 0813 is the recommended default from models
            your key can actually reach. Add a few dollars of credit, paste the key in Settings,
            then come back.
          </p>
          <button type="button" className="primary" onClick={onOpenSettings}>
            Open Settings
          </button>
        </section>
      ) : null}

      {keyReady && !enoughRatings && snapshot ? (
        <p className="muted pad">Rate at least 8 films so the agent has likes and dislikes to compare.</p>
      ) : null}

      {running ? (
        <section className="taste-run" aria-live="polite">
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
          <p className="muted">{waitHint()}</p>
        </section>
      ) : null}

      {report ? (
        <TasteLists
          report={report}
          hidden={hidden}
          interested={interested}
          onSelectFilm={onSelectFilm}
          onFeedback={(pick, action) => void sendFeedback(pick, action)}
        />
      ) : null}

      {keyReady && enoughRatings && !report && !running ? (
        <p className="muted pad">
          The scorer reads every rating, then finds new films and ranks your watchlist separately.
          Rewatches stay in the profile. The model writes a taste profile after the lists are chosen.
        </p>
      ) : null}

      {report ? <TasteProfileFooter report={report} /> : null}
    </div>
  );
}

function TasteLists({
  report,
  hidden,
  interested,
  onSelectFilm,
  onFeedback,
}: {
  report: NonNullable<TasteState["report"]>;
  hidden: Set<string>;
  interested: Set<number>;
  onSelectFilm: (id: string) => void;
  onFeedback: (pick: TastePick, action: "interested" | "rejected" | "seen") => void;
}) {
  const { neu, watch } = sectionPicks(report);
  const visibleNew = neu.filter((p) => !hidden.has(pickKey(p)));
  const visibleWatch = watch.filter((p) => !hidden.has(pickKey(p)));
  const [sort, setSort] = useState<TasteSort>("match");
  const [filter, setFilter] = useState<TasteFilter>("all");
  const sortedNew = sortTastePicks(visibleNew, sort, filter);
  const sortedWatch = sortTastePicks(visibleWatch, sort, filter);
  const controls = neu.length || watch.length ? (
    <div className="flat-menu-toolbar taste-list-toolbar" role="group" aria-label="Recommendation list controls">
      <Menu label="Sort" value={sort} options={TASTE_SORTS} onChange={(id) => setSort(id)} />
      <Menu label="Match" value={filter} options={TASTE_FILTERS} onChange={(id) => setFilter(id)} />
    </div>
  ) : null;
  return (
    <>
      {neu.length ? (
        <section>
          <div className="shelf-head taste-shelf-head">
            <div className="taste-list-heading">
              <h2>New for you</h2>
              <span className="muted">{listCount(sortedNew.length, visibleNew.length)} picks</span>
            </div>
            {controls}
          </div>
          {sortedNew.length ? (
            <ul className="rec-list taste-rec-list">
              {sortedNew.map((pick) => (
                <TastePickCard
                  key={pickKey(pick)}
                  pick={pick}
                  interested={Boolean(pick.tmdbId && interested.has(pick.tmdbId))}
                  onSelectFilm={onSelectFilm}
                  onFeedback={onFeedback}
                />
              ))}
            </ul>
          ) : (
            <p className="muted pad">
              {visibleNew.length ? "No new films match this filter." : "All new films are hidden."}
            </p>
          )}
        </section>
      ) : (
        <section>
          <div className="shelf-head taste-shelf-head">
            <div className="taste-list-heading">
              <h2>New for you</h2>
              <span className="muted">0 picks</span>
            </div>
            {controls}
          </div>
          <p className="muted pad">
            Nothing new cleared this run.
            {watch.length
              ? " Watchlist below is what you already saved."
              : ""}
          </p>
        </section>
      )}
      {watch.length ? (
        <section className="taste-watchlist-section">
          <div className="shelf-head">
            <h2>Already on your watchlist</h2>
            <span className="muted">{listCount(sortedWatch.length, visibleWatch.length)} picks</span>
          </div>
          {sortedWatch.length ? (
            <ul className="rec-list taste-rec-list">
              {sortedWatch.map((pick) => (
                <TastePickCard
                  key={pickKey(pick)}
                  pick={pick}
                  interested={Boolean(pick.tmdbId && interested.has(pick.tmdbId))}
                  onSelectFilm={onSelectFilm}
                  onFeedback={onFeedback}
                />
              ))}
            </ul>
          ) : (
            <p className="muted pad">No watchlist films match this filter.</p>
          )}
        </section>
      ) : null}
    </>
  );
}

function TasteProfileFooter({ report }: { report: NonNullable<TasteState["report"]> }) {
  const summary = report.summary || report.note;
  const affinities = report.affinities.slice(0, 2).map((item) => item.label);
  if (!summary && !affinities.length) return null;
  return (
    <footer className="taste-profile-footer" aria-label="Taste profile">
      <span className="taste-profile-label">Taste profile</span>
      {summary ? <p>{summary}</p> : null}
      {affinities.length ? <span className="taste-profile-affinities">{affinities.join(" · ")}</span> : null}
    </footer>
  );
}

function TastePickCard({
  pick,
  interested,
  onSelectFilm,
  onFeedback,
}: {
  pick: TastePick;
  interested: boolean;
  onSelectFilm: (id: string) => void;
  onFeedback: (pick: TastePick, action: "interested" | "rejected" | "seen") => void;
}) {
  const id = pickId(pick);
  const evidenceItems = (pick.evidenceItems ?? [])
    .flatMap((item) => {
      const evidenceId = item.filmId || (item.tmdbId != null ? `tmdb:${item.tmdbId}` : null);
      return evidenceId ? [{ ...item, id: evidenceId }] : [];
    })
    .slice(0, 2);
  const closeTo = pick.evidence?.length
    ? `Close to ${pick.evidence.slice(0, 2).join(", ")}`
    : pick.rhymesWith?.length
      ? `Close to ${pick.rhymesWith.slice(0, 2).join(", ")}`
      : null;
  const rationale = evidenceItems.length ? null : pick.why || closeTo;
  const heading = (
    <>
      <Poster name={pick.title} poster={pick.poster} large />
      <span className="taste-pick-copy">
        <strong title={pick.title}>{pick.title}</strong>
        <span className="taste-pick-meta muted">
          <span className={`taste-match ${matchTone(pick)}`}>{matchPercent(pick)}</span>
          {pick.year != null ? <span>{pick.year}</span> : null}
        </span>
        {rationale ? <span className="taste-pick-rationale">{rationale}</span> : null}
      </span>
    </>
  );
  return (
    <li>
      <article className={`taste-pick${interested ? " is-interested" : ""}`}>
        {id ? (
          <button type="button" className="taste-pick-open" onClick={() => onSelectFilm(id)}>
            {heading}
          </button>
        ) : (
          <div className="taste-pick-open is-static">{heading}</div>
        )}
        {evidenceItems.length ? (
          <div
            className="taste-evidence"
            role="group"
            aria-label={`Picked from your interest in ${evidenceItems.map((item) => item.title).join(", ")}`}
          >
            {evidenceItems.map((item) => (
              <button
                key={item.id}
                type="button"
                title={item.title}
                aria-label={`View ${item.title}`}
                onClick={() => onSelectFilm(item.id)}
              >
                <Poster name={item.title} poster={item.poster} />
              </button>
            ))}
          </div>
        ) : null}
        <div className="taste-fb" role="group" aria-label={`Feedback for ${pick.title}`}>
          <button
            type="button"
            aria-label="Mark as interested"
            aria-pressed={interested}
            title="Interested"
            className={`taste-fb-interest${interested ? " is-on" : ""}`}
            onClick={() => onFeedback(pick, "interested")}
          >
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path
                d="M20.84 4.61a5.5 5.5 0 0 0-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 0 0-7.78 7.78L12 21.23l8.84-8.84a5.5 5.5 0 0 0 0-7.78Z"
                style={{ fill: interested ? "currentColor" : "none" }}
              />
            </svg>
            <span>Save</span>
          </button>
          <button
            type="button"
            className="taste-fb-reject"
            aria-label="Not for me"
            title="Not for me"
            onClick={() => onFeedback(pick, "rejected")}
          >
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="m7 7 10 10M17 7 7 17" />
            </svg>
            <span>Pass</span>
          </button>
          <button
            type="button"
            className="taste-fb-seen"
            aria-label="Already seen"
            title="Already seen"
            onClick={() => onFeedback(pick, "seen")}
          >
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="m5 12.5 4.5 4.5L19 7" />
            </svg>
            <span>Seen</span>
          </button>
        </div>
      </article>
    </li>
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
          aria-pressed={selected === item.id}
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
