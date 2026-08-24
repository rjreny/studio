import { useEffect, useMemo, useState } from "react";
import { getCoverage, getLibrary } from "../../platform/filmLibrary";
import type { LibraryCoverage, LibraryItem } from "../../platform/types/film";

function ratedCount(items: LibraryItem[]) {
  return items.filter((f) => f.currentRating != null).length;
}

export function StatsView() {
  const [items, setItems] = useState<LibraryItem[]>([]);
  const [coverage, setCoverage] = useState<LibraryCoverage | null>(null);

  useEffect(() => {
    void (async () => {
      const [page, c] = await Promise.all([
        getLibrary({ limit: 10000, sort: "rating" }),
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
      if (d < 1920 || d > 2020) return;
      map.set(d, (map.get(d) ?? 0) + 1);
    });
    return [...map.entries()].sort((a, b) => b[0] - a[0]);
  }, [items]);

  const maxBucket = Math.max(1, ...distribution);
  const maxDecade = Math.max(1, ...decades.map(([, n]) => n), 1);

  return (
    <div className="stats-page page-pad">
      <header className="page-head">
        <div>
          <h1>Stats</h1>
          <p className="muted">Your log, as numbers</p>
        </div>
      </header>
      <div className="stat-tiles">
        <div className="stat-tile">
          <strong>{coverage?.uniqueMovies ?? items.length}</strong>
          <span>Films</span>
        </div>
        <div className="stat-tile">
          <strong>{coverage?.totalViewings ?? 0}</strong>
          <span>Times watched</span>
        </div>
        <div className="stat-tile">
          <strong>{coverage?.ratingEvents ?? ratedCount(items)}</strong>
          <span>Rating events</span>
        </div>
        <div className="stat-tile">
          <strong>{coverage?.unresolvedMovies ?? 0}</strong>
          <span>Unmatched</span>
        </div>
      </div>
      <div className="stats-split">
        <section className="solid-card">
          <h2>Ratings</h2>
          <div className="chart" role="img" aria-label="Rating histogram">
            {distribution.map((count, idx) => {
              const label = ((idx + 1) / 2).toFixed(1);
              const pct = (count / maxBucket) * 100;
              return (
                <div key={label} className="bar">
                  <div className="bar-track">
                    <span style={{ height: `${Math.max(count ? 8 : 0, pct)}%` }} title={`${label} stars: ${count}`} />
                  </div>
                  <small>{label.replace(".0", "")}</small>
                </div>
              );
            })}
          </div>
        </section>
        <section className="solid-card">
          <h2>By decade</h2>
          <ul className="decade-bars">
            {decades.map(([decade, count]) => (
              <li key={decade}>
                <span>{decade}s</span>
                <div>
                  <i style={{ width: `${(count / maxDecade) * 100}%` }} />
                </div>
                <strong>{count}</strong>
              </li>
            ))}
          </ul>
        </section>
      </div>
    </div>
  );
}
