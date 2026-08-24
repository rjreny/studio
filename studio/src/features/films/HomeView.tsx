import { useEffect, useMemo, useState } from "react";
import { getFilm } from "../../platform/filmLibrary";
import type { FilmDetail, HomeViewModel, LibraryItem } from "../../platform/types/film";
import { FilmCard } from "./FilmCard";
import { Poster } from "./Poster";
import { RatingDisplay } from "./RatingDisplay";
import { Shelf } from "./Shelf";

function heroSrc(film: LibraryItem, detail: FilmDetail | null) {
  return detail?.backdrop || film.backdrop || detail?.poster || film.poster || null;
}

function trimOverview(text: string | null | undefined) {
  if (!text) return null;
  const words = text.trim().split(/\s+/);
  if (words.length <= 22) return text.trim();
  return `${words.slice(0, 22).join(" ")}…`;
}

function runtimeLabel(minutes: number | null | undefined) {
  if (!minutes) return null;
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  if (h <= 0) return `${m} min`;
  return m ? `${h}h ${m}m` : `${h}h`;
}

export function HomeView({
  home,
  onOpenFilms,
  onOpenFriends,
  onSelectFilm,
}: {
  home: HomeViewModel | null;
  onOpenFilms: () => void;
  onOpenFriends: () => void;
  onSelectFilm: (id: string) => void;
}) {
  const slides = useMemo(() => home?.recent.slice(0, 6) ?? [], [home]);
  const [index, setIndex] = useState(0);
  const [detail, setDetail] = useState<FilmDetail | null>(null);
  const featured = slides[index] ?? home?.topRated[0] ?? null;

  useEffect(() => {
    setIndex(0);
  }, [home]);

  useEffect(() => {
    if (!featured) {
      setDetail(null);
      return;
    }
    let cancelled = false;
    void getFilm(featured.id)
      .then((next) => {
        if (!cancelled) setDetail(next);
      })
      .catch(() => {
        if (!cancelled) setDetail(null);
      });
    return () => {
      cancelled = true;
    };
  }, [featured?.id]);

  useEffect(() => {
    if (slides.length < 2) return;
    const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (reduce) return;
    const timer = window.setInterval(() => {
      setIndex((i) => (i + 1) % slides.length);
    }, 8000);
    return () => window.clearInterval(timer);
  }, [slides.length]);

  if (!home) {
    return <p className="muted pad">Loading your library…</p>;
  }

  const image = featured ? heroSrc(featured, detail) : null;
  const castLine = detail?.cast.slice(0, 3).join("  ").toUpperCase() ?? "";
  const overview = trimOverview(detail?.overview || featured?.overview);
  const runtime = runtimeLabel(detail?.runtime);
  const genre = detail?.genres[0];

  return (
    <div className="home-cinema">
      {featured ? (
        <section className="hero">
          {image ? <img className="hero-image" src={image} alt="" /> : <div className="hero-image is-empty" />}
          <div className="hero-scrim" />
          <div className="hero-copy">
            {castLine ? <p className="hero-cast">{castLine}</p> : null}
            <h1>{featured.title}</h1>
            <p className="hero-meta">
              {featured.year ? <span>{featured.year}</span> : null}
              {runtime ? <span>{runtime}</span> : null}
              {genre ? <span>{genre}</span> : null}
              <RatingDisplay value={featured.currentRating} compact />
            </p>
            {overview ? <p className="hero-lede">{overview}</p> : null}
            <div className="hero-actions">
              <button type="button" className="play-btn" onClick={() => onSelectFilm(featured.id)}>
                Open
              </button>
            </div>
          </div>
          {slides.length > 1 ? (
            <div className="hero-dots" role="tablist" aria-label="Featured films">
              {slides.map((film, i) => (
                <button
                  key={film.id}
                  type="button"
                  role="tab"
                  aria-selected={i === index}
                  className={i === index ? "is-on" : ""}
                  onClick={() => setIndex(i)}
                >
                  <span className="sr-only">{film.title}</span>
                </button>
              ))}
            </div>
          ) : null}
        </section>
      ) : (
        <section className="hero is-empty-hero">
          <div className="hero-copy">
            <h1>Your log is waiting</h1>
            <p className="hero-lede">Import a Letterboxd export or connect your public diary to fill this shelf.</p>
            <div className="hero-actions">
              <button type="button" className="play-btn" onClick={onOpenFilms}>
                All films
              </button>
            </div>
          </div>
        </section>
      )}

      <div className="home-shelves">
        <p className="coverage-line">{home.coverage.warnings[0] ?? `${home.coverage.uniqueMovies} films in your library`}</p>
        <Shelf
          title="Recent from your log"
          action={
            <button type="button" className="text-btn" onClick={onOpenFilms}>
              All films
            </button>
          }
          empty={
            home.recent.length ? undefined : (
              <p className="muted">Import your export or connect RSS to fill this shelf.</p>
            )
          }
        >
          {home.recent.map((film) => (
            <FilmCard key={film.id} film={film} onSelect={onSelectFilm} />
          ))}
        </Shelf>

        {home.topRated.length ? (
          <Shelf title="Top rated">
            {home.topRated.map((film) => (
              <FilmCard key={film.id} film={film} onSelect={onSelectFilm} />
            ))}
          </Shelf>
        ) : null}

        <Shelf
          title="Friends just rated"
          action={
            <button type="button" className="text-btn" onClick={onOpenFriends}>
              Manage
            </button>
          }
          empty={
            home.friendFeed.length ? undefined : (
              <p className="muted">Add friends by Letterboxd username to see their public ratings.</p>
            )
          }
        >
          {home.friendFeed.slice(0, 12).map((entry, idx) => (
            <div key={`${entry.username}-${entry.title}-${idx}`} className="film-card">
              <Poster name={entry.title} poster={entry.poster} large />
              <strong>{entry.title}</strong>
              <span className="muted">@{entry.username}</span>
              <RatingDisplay value={entry.rating} compact />
            </div>
          ))}
        </Shelf>
      </div>
    </div>
  );
}
