import { useEffect, useMemo, useState } from "react";
import { getLibrary } from "../../platform/filmLibrary";
import { log } from "../../platform/log";
import type { LibraryItem } from "../../platform/types/film";
import { Menu } from "../ui/Menu";
import { FilmCard } from "./FilmCard";

const SORTS = [
  { id: "recent", label: "Recently watched" },
  { id: "rating", label: "Your rating" },
  { id: "title", label: "Title" },
  { id: "year", label: "Release year" },
] as const;

const FILTERS = [
  { id: "all", label: "All films" },
  { id: "watched", label: "Watched" },
  { id: "watchlist", label: "Watchlist" },
  { id: "unresolved", label: "Needs a match" },
] as const;

export function FilmsView({
  onSelectFilm,
  onStatus,
  reloadToken = 0,
  query = "",
}: {
  onSelectFilm: (id: string) => void;
  onStatus: (s: string) => void;
  reloadToken?: number;
  query?: string;
}) {
  const [sort, setSort] = useState<(typeof SORTS)[number]["id"]>("recent");
  const [filter, setFilter] = useState<(typeof FILTERS)[number]["id"]>("all");
  const [decade, setDecade] = useState("any");
  const [items, setItems] = useState<LibraryItem[]>([]);
  const [pool, setPool] = useState<LibraryItem[]>([]);
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
        setPool(rows);
        if (decade !== "any") {
          const start = Number(decade);
          rows = rows.filter((f) => f.year && f.year >= start && f.year < start + 10);
        }
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
  }, [query, sort, filter, decade, onStatus, reloadToken]);

  const decadeOptions = useMemo(() => {
    const set = new Set<number>();
    pool.forEach((f) => {
      if (f.year) set.add(Math.floor(f.year / 10) * 10);
    });
    return [
      { id: "any", label: "Any decade" },
      ...[...set]
        .sort((a, b) => b - a)
        .filter((d) => d >= 1920 && d <= 2020)
        .map((d) => ({ id: String(d), label: `${d}s` })),
    ];
  }, [pool]);

  return (
    <div className="films page-pad">
      <header className="page-head">
        <div>
          <h1>Films</h1>
          <p className="muted">{total} in your library</p>
        </div>
        {busy ? <span className="muted">Updating…</span> : null}
      </header>
      <div className="filter-bar">
        <Menu label="Sort" value={sort} options={[...SORTS]} onChange={(id) => setSort(id)} />
        <Menu label="Show" value={filter} options={[...FILTERS]} onChange={(id) => setFilter(id)} />
        <Menu label="Decade" value={decade} options={decadeOptions} onChange={setDecade} />
      </div>
      <div className="film-grid">
        {items.map((film) => (
          <FilmCard key={film.id} film={film} onSelect={onSelectFilm} />
        ))}
      </div>
      {!items.length ? <p className="muted pad">Nothing here yet. Connect or import.</p> : null}
    </div>
  );
}
