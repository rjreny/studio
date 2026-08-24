import { useEffect, useState } from "react";
import { getFilm, setRating } from "../../platform/filmLibrary";
import type { FilmDetail } from "../../platform/types/film";
import { FilmCard } from "./FilmCard";
import { RatingControl, RatingDisplay } from "./RatingDisplay";

function runtimeLabel(minutes: number | null) {
  if (!minutes) return null;
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  if (h <= 0) return `${m} min`;
  return m ? `${h}h ${m}m` : `${h}h`;
}

export function FilmDetailView({
  filmId,
  onBack,
  onUpdated,
  onStatus,
}: {
  filmId: string;
  onBack: () => void;
  onUpdated: () => Promise<void>;
  onStatus: (s: string) => void;
}) {
  const [film, setFilm] = useState<FilmDetail | null>(null);

  useEffect(() => {
    void getFilm(filmId)
      .then(setFilm)
      .catch(() => onStatus("Could not load film"));
  }, [filmId, onStatus]);

  if (!film) return <p className="muted pad">Loading…</p>;

  async function rate(value: number) {
    if (!film) return;
    const next = await setRating(filmId, value);
    setFilm(next);
    await onUpdated();
    onStatus(`Rated ${next.title} ${value}`);
  }

  const image = film.backdrop || film.poster;
  const castLine = film.cast.slice(0, 3).join("  ").toUpperCase();
  const runtime = runtimeLabel(film.runtime);

  return (
    <article className="film-detail">
      <header className="hero detail-hero">
        {image ? <img className="hero-image" src={image} alt="" /> : <div className="hero-image is-empty" />}
        <div className="hero-scrim" />
        <div className="hero-copy">
          <button type="button" className="ghost-pill" onClick={onBack}>
            Back
          </button>
          {castLine ? <p className="hero-cast">{castLine}</p> : null}
          <h1>{film.title}</h1>
          <p className="hero-meta">
            <span>{film.year ?? "Year unknown"}</span>
            {runtime ? <span>{runtime}</span> : null}
            {film.genres[0] ? <span>{film.genres[0]}</span> : null}
          </p>
          <p className="source-badge">Source: {film.sourceIdentity}</p>
          <RatingControl value={film.currentRating} onChange={(v) => void rate(v)} />
        </div>
      </header>

      <div className="detail-body">
        <section className="detail-block">
          <h2>Your history</h2>
          <p className="section-source">Your Letterboxd / local events</p>
          <ul className="history-list">
            {film.yourHistory.map((v) => (
              <li key={v.id}>
                <strong>{v.occurredAt ?? v.publishedAt ?? "Unknown date"}</strong>
                {v.rewatch ? <span className="rewatch-badge">Rewatch</span> : null}
                <RatingDisplay value={v.rating} compact />
                <span className="muted">{v.source}</span>
              </li>
            ))}
          </ul>
        </section>

        <section className="detail-block">
          <h2>Friends</h2>
          <p className="section-source">Followed friends, public RSS</p>
          {film.friends.length ? (
            <ul className="feed">
              {film.friends.map((f, idx) => (
                <li key={`${f.username}-${idx}`}>
                  <div>
                    <strong>@{f.username}</strong>
                    <RatingDisplay value={f.rating} compact />
                    {f.review ? <p>{f.review}</p> : null}
                  </div>
                </li>
              ))}
            </ul>
          ) : (
            <p className="muted">No friend activity for this film yet.</p>
          )}
        </section>

        <section className="detail-block">
          <h2>About the film</h2>
          <p className="section-source">TMDB catalog, {film.matchState}</p>
          {film.overview ? <p className="detail-overview">{film.overview}</p> : <p className="muted">Not enriched yet.</p>}
          {film.runtime ? (
            <p className="muted">
              {film.runtime} min
              {film.genres.length ? `  ${film.genres.join(", ")}` : ""}
            </p>
          ) : null}
        </section>

        <section className="detail-block">
          <h2>TMDB community data</h2>
          <p className="section-source">TMDB vote average, not Letterboxd or your friends</p>
          <p>
            Average: {film.tmdbVoteAverage?.toFixed(1) ?? "—"} ({film.tmdbVoteCount ?? 0} votes)
          </p>
          <ul>
            {film.tmdbReviews.map((r) => (
              <li key={r}>{r}</li>
            ))}
          </ul>
        </section>

        <section className="detail-block">
          <h2>Cast & crew</h2>
          <p className="section-source">TMDB credits</p>
          <p>{film.cast.join("  ")}</p>
          <p className="muted">{film.crew.join("  ")}</p>
        </section>

        {film.similar.length ? (
          <section className="detail-block">
            <h2>Similar films</h2>
            <p className="section-source">TMDB recommendations</p>
            <div className="shelf-track">
              {film.similar.map((s) => (
                <FilmCard key={s.id} film={s} />
              ))}
            </div>
          </section>
        ) : null}
      </div>
    </article>
  );
}
