export interface TmdbMovie {
  id: number;
  title: string;
  release_date?: string;
  poster_path: string | null;
  vote_average: number;
  overview: string;
  genre_ids?: number[];
}

const GENRES: Record<number, string> = {
  28: "Action",
  12: "Adventure",
  16: "Animation",
  35: "Comedy",
  80: "Crime",
  99: "Documentary",
  18: "Drama",
  10751: "Family",
  14: "Fantasy",
  36: "History",
  27: "Horror",
  10402: "Music",
  9648: "Mystery",
  10749: "Romance",
  878: "Science Fiction",
  53: "Thriller",
  10752: "War",
  37: "Western",
};

export function posterUrl(path: string | null | undefined): string | null {
  if (!path) return null;
  if (path.startsWith("http")) return path;
  return `https://image.tmdb.org/t/p/w342${path}`;
}

export function genreNames(ids: number[] | undefined): string[] {
  return (ids ?? []).map((id) => GENRES[id]).filter(Boolean);
}

async function tmdb<T>(key: string, path: string, params: Record<string, string> = {}): Promise<T> {
  const url = new URL(`https://api.themoviedb.org/3/${path}`);
  url.searchParams.set("api_key", key);
  for (const [k, v] of Object.entries(params)) url.searchParams.set(k, v);
  const res = await fetch(url);
  if (!res.ok) throw new Error(`TMDB ${res.status}`);
  return res.json() as Promise<T>;
}

export async function searchMovies(key: string, query: string): Promise<TmdbMovie[]> {
  if (!key.trim() || !query.trim()) return [];
  const data = await tmdb<{ results: TmdbMovie[] }>(key, "search/movie", { query: query.trim() });
  return data.results ?? [];
}

export async function similarMovies(key: string, tmdbId: string): Promise<TmdbMovie[]> {
  const data = await tmdb<{ results: TmdbMovie[] }>(key, `movie/${tmdbId}/similar`);
  return data.results ?? [];
}

export function yearFromDate(date?: string): number | null {
  if (!date) return null;
  const y = Number.parseInt(date.slice(0, 4), 10);
  return Number.isFinite(y) ? y : null;
}
