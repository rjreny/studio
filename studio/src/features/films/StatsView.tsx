import { useEffect, useMemo, useState } from "react";
import { getCoverage, getLibrary } from "../../platform/filmLibrary";
import type { LibraryCoverage, LibraryItem } from "../../platform/types/film";

export function StatsView() {
  const [items, setItems] = useState<LibraryItem[]>([]);
  const [coverage, setCoverage] = useState<LibraryCoverage | null>(null);

  useEffect(() => {
    void (async () => {
      const [page, c] = await Promise.all([
        getLibrary({ limit: 1000, sort: "rating" }),
        getCoverage(),
      ]);
      setItems(page.items);
      setCoverage(c);
    })();
  }, []);

  const distribution = useMemo(() => {
    const buckets = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    items.forEach((f) => {
      if (f.currentRating == null) return;
      const idx = Math.min(9, Math.max(0, Math.round(f.currentRating * 2) - 1));
      buckets[idx] += 1;
    });
    return buckets;
  }, [items]);

  const decades = useMemo(() => {
    const map = new Map<number, number>();
    items.forEach((f) => {
      if (!f.year) return;
      const d = Math.floor(f.year / 10) * 10;
      map.set(d, (map.get(d) ?? 0) + 1);
    });
    return [...map.entries()].sort((a, b) => b[0] - a[0]);
  }, [items]);

  const maxBucket = Math.max(1, ...distribution);

  return (
    <div className="stats-page">
      <header className="toolbar">
        <div>
          <h1>Stats</h1>
          {coverage ? (
            <p className="muted">
              {coverage.totalViewings} viewings · {coverage.ratingEvents} rating events ·{" "}
              {coverage.unresolvedMovies} unresolved
            </p>
          ) : null}
        </div>
      </header>
      <section className="solid-card">
        <h2>Rating distribution</h2>
        <div className="chart" role="img" aria-label="Rating histogram">
          {distribution.map((count, idx) => {
            const label = ((idx + 1) / 2).toFixed(1);
            return (
              <div key={label} className="bar">
                <span style={{ height: `${(count / maxBucket) * 100}%` }} title={`${label} stars: ${count}`} />
                <small>{label}</small>
                <strong>{count}</strong>
              </div>
            );
          })}
        </div>
        <table className="stats-table">
          <caption>Rating distribution table</caption>
          <thead>
            <tr>
              <th>Stars</th>
              <th>Films</th>
            </tr>
          </thead>
          <tbody>
            {distribution.map((count, idx) => (
              <tr key={idx}>
                <td>{((idx + 1) / 2).toFixed(1)}</td>
                <td>{count}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>
      <section className="solid-card">
        <h2>By decade</h2>
        <ul className="decade-bars">
          {decades.map(([decade, count]) => (
            <li key={decade}>
              <span>{decade}s</span>
              <div>
                <i style={{ width: `${(count / items.length) * 100}%` }} />
              </div>
              <strong>{count}</strong>
            </li>
          ))}
        </ul>
      </section>
    </div>
  );
}
