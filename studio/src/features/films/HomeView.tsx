import type { HomeViewModel } from "../../platform/types/film";
import { Poster } from "./Poster";
import { RatingDisplay } from "./RatingDisplay";

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
  if (!home) {
    return <p className="muted pad">Loading your library…</p>;
  }

  return (
    <div className="home-grid">
      <section className="coverage-banner solid-card">
        <p className="eyebrow">Library coverage</p>
        <p className="coverage-line">
          {home.coverage.uniqueMovies} unique films · {home.coverage.totalViewings} recorded viewings
        </p>
        {home.coverage.warnings.map((w) => (
          <p key={w} className="hint">
            {w}
          </p>
        ))}
      </section>
      <section className="solid-card">
        <header className="section-head">
          <h2>Recent from your log</h2>
          <button type="button" className="text-btn" onClick={onOpenFilms}>
            All films
          </button>
        </header>
        <div className="poster-row">
          {home.recent.length ? (
            home.recent.map((film) => (
              <button
                key={film.id}
                type="button"
                className="poster-card"
                onClick={() => onSelectFilm(film.id)}
              >
                <Poster name={film.title} poster={film.poster} />
                <strong>{film.title}</strong>
                <span className="title-year">{film.year ?? "—"}</span>
                <RatingDisplay value={film.currentRating} compact />
                {film.viewingCount > 1 ? (
                  <span className="muted">{film.viewingCount} viewings</span>
                ) : null}
              </button>
            ))
          ) : (
            <p className="muted">Import your export or connect RSS to fill this shelf.</p>
          )}
        </div>
      </section>
      <section className="solid-card">
        <header className="section-head">
          <h2>Friends just rated</h2>
          <button type="button" className="text-btn" onClick={onOpenFriends}>
            Manage
          </button>
        </header>
        {home.friendFeed.length ? (
          <ul className="feed">
            {home.friendFeed.slice(0, 10).map((e, idx) => (
              <li key={`${e.username}-${idx}`}>
                <Poster name={e.title} poster={e.poster} />
                <div>
                  <strong>{e.title}</strong>
                  <span className="muted">
                    @{e.username}
                    {e.year ? ` · ${e.year}` : ""}
                  </span>
                  <RatingDisplay value={e.rating} compact />
                </div>
              </li>
            ))}
          </ul>
        ) : (
          <p className="muted">Add friends by Letterboxd username to see their public ratings.</p>
        )}
      </section>
    </div>
  );
}
