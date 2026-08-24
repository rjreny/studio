import { useEffect, useState } from "react";
import { getFilm, setRating } from "../../platform/filmLibrary";
import type { FilmDetail } from "../../platform/types/film";
import { Poster } from "./Poster";
import { RatingControl, RatingDisplay } from "./RatingDisplay";

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

  return (
    <article className="film-detail">
      <header className="film-detail-hero">
        {film.backdrop ? (
          <img className="film-detail-backdrop" src={film.backdrop} alt="" />
        ) : null}
        <div className="film-detail-hero-inner solid-card">
          <button type="button" className="ghost" onClick={onBack}>
            ← Back
          </button>
          <div className="film-detail-head">
            <Poster name={film.title} poster={film.poster} large />
            <div>
              <h1>{film.title}</h1>
              <p className="title-year">{film.year ?? "Year unknown"}</p>
              <p className="source-badge">Source: {film.sourceIdentity}</p>
              <RatingControl value={film.currentRating} onChange={(v) => void rate(v)} />
            </div>
          </div>
        </div>
      </header>

      <section className="detail-section solid-card">
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

      <section className="detail-section solid-card">
        <h2>Friends</h2>
        <p className="section-source">Followed friends · public RSS</p>
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

      <section className="detail-section solid-card">
        <h2>About the film</h2>
        <p className="section-source">TMDB catalog · {film.matchState}</p>
        {film.overview ? <p>{film.overview}</p> : <p className="muted">Not enriched yet.</p>}
        {film.runtime ? <p className="muted">{film.runtime} min · {film.genres.join(", ")}</p> : null}
      </section>

      <section className="detail-section solid-card">
        <h2>TMDB community data</h2>
        <p className="section-source">TMDB vote average — not Letterboxd or your friends</p>
        <p>
          Average: {film.tmdbVoteAverage?.toFixed(1) ?? "—"} ({film.tmdbVoteCount ?? 0} votes)
        </p>
        <ul>
          {film.tmdbReviews.map((r) => (
            <li key={r}>{r}</li>
          ))}
        </ul>
      </section>

      <section className="detail-section solid-card">
        <h2>Cast & crew</h2>
        <p className="section-source">TMDB credits</p>
        <p>{film.cast.join(" · ")}</p>
        <p className="muted">{film.crew.join(" · ")}</p>
      </section>

      {film.similar.length ? (
        <section className="detail-section solid-card">
          <h2>Similar films</h2>
          <p className="section-source">TMDB recommendations</p>
          <div className="poster-row">
            {film.similar.map((s) => (
              <div key={s.id} className="poster-card">
                <Poster name={s.title} poster={s.poster} />
                <strong>{s.title}</strong>
              </div>
            ))}
          </div>
        </section>
      ) : null}
    </article>
  );
}
