import { useEffect, useState } from "react";
import { communityRatingOutOfFive, isHighQualityBanner } from "../../core/images";
import { log } from "../../platform/log";
import { getFilm, setRating } from "../../platform/filmLibrary";
import type { FilmDetail, LibraryItem } from "../../platform/types/film";
import { FilmCard } from "./FilmCard";
import { RatingControl, RatingDisplay } from "./RatingDisplay";
import { Shelf } from "./Shelf";

function runtimeLabel(minutes: number | null) {
  if (!minutes) return null;
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  if (h <= 0) return `${m} min`;
  return m ? `${h}h ${m}m` : `${h}h`;
}

function CreditList({ items }: { items: string[] }) {
  if (!items.length) return <p className="muted">Not listed yet.</p>;
  return (
    <ul className="credit-list">
      {items.map((item) => (
        <li key={item}>{item}</li>
      ))}
    </ul>
  );
}

function RelatedShelf({
  title,
  source,
  films,
  onSelect,
}: {
  title: string;
  source: string;
  films: LibraryItem[];
  onSelect?: (id: string) => void;
}) {
  if (!films.length) return null;
  return (
    <section className="detail-block">
      <Shelf title={title}>
        {films.map((film) => (
          <FilmCard key={film.id} film={film} onSelect={onSelect} />
        ))}
      </Shelf>
      <p className="section-source">{source}</p>
    </section>
  );
}

export function FilmDetailView({
  filmId,
  onBack,
  onUpdated,
  onStatus,
  onSelectFilm,
}: {
  filmId: string;
  onBack: () => void;
  onUpdated: () => Promise<void>;
  onStatus: (s: string) => void;
  onSelectFilm?: (id: string) => void;
}) {
  const [film, setFilm] = useState<FilmDetail | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setFilm(null);
    setError(null);
    void getFilm(filmId)
      .then(setFilm)
      .catch((err) => {
        log("error", "film load failed", err);
        setError(err instanceof Error ? err.message : String(err));
        onStatus("Could not load film");
      });
  }, [filmId, onStatus]);

  if (error) {
    return (
      <div className="pad">
        <button type="button" className="ghost-pill" onClick={onBack}>
          Back
        </button>
        <p className="muted">Could not load this film. {error}</p>
      </div>
    );
  }

  if (!film) return <p className="muted pad">Loading…</p>;

  async function rate(value: number) {
    if (!film) return;
    const next = await setRating(filmId, value);
    setFilm(next);
    await onUpdated();
    onStatus(`Rated ${next.title} ${value}`);
  }

  const image = isHighQualityBanner(film.backdrop) ? film.backdrop : null;
  const directors = film.directors?.length ? film.directors : [];
  const directorLine = directors.join("  ").toUpperCase();
  const runtime = runtimeLabel(film.runtime);
  const community = communityRatingOutOfFive(film.tmdbVoteAverage);
  const canRate = film.matchState !== "catalog";

  return (
    <article className="film-detail">
      <header className="hero detail-hero">
        {image ? <img className="hero-image" src={image} alt="" /> : <div className="hero-image is-empty" />}
        <div className="hero-scrim" />
        <div className="hero-copy">
          {film.poster ? <img className="detail-poster" src={film.poster} alt="" /> : null}
          <div className="hero-copy-text">
          <button type="button" className="ghost-pill" onClick={onBack}>
            Back
          </button>
          {directorLine ? <p className="hero-cast">{directorLine}</p> : null}
          <h1>{film.title}</h1>
          <p className="hero-meta">
            <span>{film.year ?? "Year unknown"}</span>
            {runtime ? <span>{runtime}</span> : null}
            {film.genres[0] ? <span>{film.genres[0]}</span> : null}
          </p>
          {film.tagline ? <p className="hero-lede">{film.tagline}</p> : null}
          <div className="detail-ratings">
            <div>
              <span className="rating-label">Average</span>
              <RatingDisplay value={community} compact />
              <span className="muted">
                {film.tmdbVoteCount ? `${film.tmdbVoteCount.toLocaleString()} votes` : "No votes yet"}
              </span>
            </div>
            <div>
              <span className="rating-label">Yours</span>
              {canRate ? (
                <RatingControl value={film.currentRating} onChange={(v) => void rate(v)} />
              ) : (
                <p className="muted">Not in your log</p>
              )}
            </div>
          </div>
          </div>
        </div>
      </header>

      <div className="detail-body">
        <section className="detail-block">
          <h2>About the film</h2>
          {film.overview ? <p className="detail-overview">{film.overview}</p> : <p className="muted">Not enriched yet.</p>}
          {film.genres.length ? <p className="muted">{film.genres.join(", ")}</p> : null}
        </section>

        {canRate ? (
          <section className="detail-block">
            <h2>Your history</h2>
            <p className="section-source">Your Letterboxd / local events</p>
            {film.yourHistory.length ? (
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
            ) : (
              <p className="muted">No diary entries for this film yet.</p>
            )}
          </section>
        ) : null}

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
          <h2>Cast</h2>
          <CreditList items={film.cast} />
        </section>

        <section className="detail-block">
          <h2>Crew</h2>
          <CreditList items={film.crew} />
        </section>

        <RelatedShelf
          title={film.collectionName ? film.collectionName : "Related films"}
          source="TMDB collection / sequels"
          films={film.collection ?? []}
          onSelect={onSelectFilm}
        />

        <RelatedShelf
          title="Similar films"
          source="TMDB recommendations"
          films={film.similar}
          onSelect={onSelectFilm}
        />
      </div>
    </article>
  );
}
