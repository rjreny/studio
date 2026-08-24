import { useEffect, useMemo, useState } from "react";
import { getLibrary } from "../../platform/filmLibrary";
import { log } from "../../platform/log";
import type { LibraryItem } from "../../platform/types/film";
import { Poster } from "./Poster";
import { RatingDisplay } from "./RatingDisplay";

const SORTS = [
  { id: "recent", label: "Recently watched" },
  { id: "rating", label: "Your rating" },
  { id: "title", label: "Title" },
  { id: "year", label: "Release year" },
] as const;

export function FilmsView({
  onSelectFilm,
  onStatus,
}: {
  onSelectFilm: (id: string) => void;
  onStatus: (s: string) => void;
}) {
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<(typeof SORTS)[number]["id"]>("recent");
  const [filter, setFilter] = useState<"all" | "watched" | "watchlist" | "unresolved">("all");
  const [items, setItems] = useState<LibraryItem[]>([]);
  const [total, setTotal] = useState(0);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    void (async () => {
      setBusy(true);
      try {
        const page = await getLibrary({
          search: query.trim() || undefined,
          sort,
          limit: 500,
        });
        let rows = page.items;
        if (filter === "watchlist") rows = rows.filter((f) => f.watchlist);
        if (filter === "watched") rows = rows.filter((f) => f.watched);
        if (filter === "unresolved") rows = rows.filter((f) => f.matchState !== "confirmed");
        setItems(rows);
        setTotal(page.total);
        onStatus(`${rows.length} shown · ${page.coverage.uniqueMovies} unique films`);
      } catch (err) {
        log("error", "library load failed", err);
        onStatus("Could not load library");
      } finally {
        setBusy(false);
      }
    })();
  }, [query, sort, filter, onStatus]);

  const decades = useMemo(() => {
    const set = new Set<number>();
    items.forEach((f) => {
      if (f.year) set.add(Math.floor(f.year / 10) * 10);
    });
    return [...set].sort((a, b) => b - a);
  }, [items]);

  return (
    <div className="films">
      <header className="toolbar glass">
        <div>
          <h1>Films</h1>
          <p className="muted">{total} in your library</p>
        </div>
        <form
          className="search"
          onSubmit={(e) => {
            e.preventDefault();
          }}
        >
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search your log"
          />
        </form>
      </header>
      <div className="filter-bar glass">
        <label>
          Sort
          <select value={sort} onChange={(e) => setSort(e.target.value as typeof sort)}>
            {SORTS.map((s) => (
              <option key={s.id} value={s.id}>
                {s.label}
              </option>
            ))}
          </select>
        </label>
        <label>
          Filter
          <select value={filter} onChange={(e) => setFilter(e.target.value as typeof filter)}>
            <option value="all">All</option>
            <option value="watched">Watched</option>
            <option value="watchlist">Watchlist</option>
            <option value="unresolved">Unresolved identity</option>
          </select>
        </label>
        {decades.length ? <span className="muted">Decades: {decades.join(", ")}</span> : null}
        {busy ? <span className="muted">Updating…</span> : null}
      </div>
      <div className="film-grid">
        {items.map((film) => (
          <button
            key={film.id}
            type="button"
            className="film-grid-card solid-card"
            onClick={() => onSelectFilm(film.id)}
          >
            <Poster name={film.title} poster={film.poster} large />
            <div className="film-grid-meta">
              <strong>{film.title}</strong>
              <span className="title-year">{film.year ?? "—"}</span>
              <RatingDisplay value={film.currentRating} compact />
              {film.viewingCount > 1 ? (
                <span className="rewatch-badge">{film.viewingCount} viewings</span>
              ) : null}
              {film.matchState === "ambiguous" ? (
                <span className="source-badge">{film.matchState}</span>
              ) : null}
            </div>
          </button>
        ))}
      </div>
      {!items.length ? <p className="muted pad">Nothing here yet. Connect or import.</p> : null}
    </div>
  );
}
