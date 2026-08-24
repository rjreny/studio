import { useEffect, useState } from "react";
import { localRecommendations, tasteProfile, type Recommendation } from "../../core/taste";
import { filmKey, type Library } from "../../core/types";
import { posterUrl, similarMovies, yearFromDate } from "../../core/tmdb";
import { log } from "../../platform/log";
import { Poster } from "./Poster";
import { Stars } from "./Stars";

export function RecsView({ library }: { library: Library }) {
  const taste = tasteProfile(library);
  const [extra, setExtra] = useState<Recommendation[]>([]);

  useEffect(() => {
    let cancelled = false;
    async function load() {
      if (!library.tmdbKey) return;
      const seeds = library.films.filter((f) => (f.rating ?? 0) >= 4.5 && f.tmdbId).slice(0, 4);
      const seen = new Set(library.films.filter((f) => f.watched || f.rating).map((f) => f.key));
      const found: Recommendation[] = [];
      for (const seed of seeds) {
        try {
          const similar = await similarMovies(library.tmdbKey, seed.tmdbId!);
          for (const movie of similar.slice(0, 5)) {
            const year = yearFromDate(movie.release_date);
            const key = filmKey(movie.title, year);
            if (seen.has(key)) continue;
            found.push({
              name: movie.title,
              year,
              poster: posterUrl(movie.poster_path),
              voteAverage: movie.vote_average,
              why: `Neighbor of ${seed.name}, which you rated ${seed.rating}.`,
              source: "tmdb",
            });
          }
        } catch (err) {
          log("warn", "similar movies failed", err);
        }
      }
      if (!cancelled) setExtra(found.slice(0, 12));
    }
    void load();
    return () => {
      cancelled = true;
    };
  }, [library]);

  const recs = [...localRecommendations(library), ...extra];

  return (
    <div className="recs page-pad">
      <header className="page-head">
        <div>
          <h1>{taste.title}</h1>
          <p className="muted">{taste.summary}</p>
        </div>
      </header>
      <ul className="rec-list">
        {recs.map((rec) => (
          <li key={`${rec.source}-${rec.name}-${rec.year}`}>
            <Poster name={rec.name} poster={rec.poster} large />
            <div>
              <strong>{rec.name}</strong>
              <span className="muted">
                {rec.year ?? ""}
                {rec.voteAverage ? ` · TMDB ${rec.voteAverage.toFixed(1)}` : ""}
              </span>
              <p>{rec.why}</p>
              {rec.rating != null ? <Stars value={rec.rating} compact /> : null}
            </div>
          </li>
        ))}
      </ul>
      {!recs.length ? (
        <p className="muted pad">
          Rate films, follow friends, or add a TMDB key in Settings so recommendations have something to stand
          on.
        </p>
      ) : null}
    </div>
  );
}
