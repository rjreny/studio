import { csvRecords } from "./csv";
import { filmKey, type Film, type Library } from "./types";

function num(value: string): number | null {
  const n = Number.parseFloat(value);
  return Number.isFinite(n) ? n : null;
}

function yearOf(value: string): number | null {
  const n = Number.parseInt(value, 10);
  return Number.isFinite(n) ? n : null;
}

function filmFromRow(row: Record<string, string>): Film | null {
  const name = (row.Name || row.name || "").trim();
  if (!name) return null;
  const year = yearOf(row.Year || row.year);
  return {
    key: filmKey(name, year),
    name,
    year,
    uri: row["Letterboxd URI"] || row.uri || "",
    rating: num(row.Rating || row.rating),
    watched: false,
    watchlist: false,
    liked: false,
    rewatch: (row.Rewatch || "").toLowerCase() === "yes",
    watchedDate: row["Watched Date"] || row.Date || null,
    tmdbId: null,
    poster: null,
    voteAverage: null,
    overview: null,
    genres: [],
  };
}

function upsert(map: Map<string, Film>, incoming: Film) {
  const prev = map.get(incoming.key);
  if (!prev) {
    map.set(incoming.key, incoming);
    return;
  }
  map.set(incoming.key, {
    ...prev,
    ...incoming,
    rating: incoming.rating ?? prev.rating,
    uri: incoming.uri || prev.uri,
    watched: prev.watched || incoming.watched,
    watchlist: prev.watchlist || incoming.watchlist,
    liked: prev.liked || incoming.liked,
    rewatch: prev.rewatch || incoming.rewatch,
    watchedDate: incoming.watchedDate || prev.watchedDate,
    poster: incoming.poster || prev.poster,
    tmdbId: incoming.tmdbId || prev.tmdbId,
  });
}

export function mergeCsvFiles(files: Record<string, string>, library: Library): Library {
  const map = new Map(library.films.map((f) => [f.key, f]));
  const pick = (name: string) => {
    const key = Object.keys(files).find((k) => k.endsWith(name));
    return key ? files[key] : "";
  };

  for (const row of csvRecords(pick("ratings.csv"))) {
    const film = filmFromRow(row);
    if (film) upsert(map, { ...film, watched: true });
  }
  for (const row of csvRecords(pick("watched.csv"))) {
    const film = filmFromRow(row);
    if (film) upsert(map, { ...film, watched: true, rating: film.rating });
  }
  for (const row of csvRecords(pick("watchlist.csv"))) {
    const film = filmFromRow(row);
    if (film) upsert(map, { ...film, watchlist: true });
  }
  for (const row of csvRecords(pick("diary.csv"))) {
    const film = filmFromRow(row);
    if (film) upsert(map, { ...film, watched: true });
  }

  if (map.size === 0) {
    const only = Object.values(files)[0];
    if (only) {
      for (const row of csvRecords(only)) {
        const film = filmFromRow(row);
        if (film) upsert(map, { ...film, watched: Boolean(film.rating) });
      }
    }
  }

  return { ...library, films: [...map.values()] };
}
