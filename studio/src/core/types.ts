export type Route = "home" | "films" | "friends" | "stats" | "recs" | "settings";
export type Theme = "system" | "dark" | "light";
export type Accent = "app" | "system";

export interface Note {
  id: string;
  title: string;
  body: string;
  updated: number;
}

export function newNote(): Note {
  return { id: crypto.randomUUID(), title: "Untitled", body: "", updated: Date.now() };
}

export interface Film {
  key: string;
  name: string;
  year: number | null;
  uri: string;
  rating: number | null;
  watched: boolean;
  watchlist: boolean;
  liked: boolean;
  rewatch: boolean;
  watchedDate: string | null;
  tmdbId: string | null;
  poster: string | null;
  voteAverage: number | null;
  overview: string | null;
  genres: string[];
}

export interface FriendEntry {
  name: string;
  year: number | null;
  rating: number | null;
  liked: boolean;
  watchedDate: string | null;
  tmdbId: string | null;
  poster: string | null;
  link: string;
}

export interface Friend {
  username: string;
  entries: FriendEntry[];
  fetchedAt: number;
  error?: string;
}

export interface Library {
  username: string;
  tmdbKey: string;
  films: Film[];
  friends: Friend[];
}

export function emptyLibrary(): Library {
  return { username: "", tmdbKey: "", films: [], friends: [] };
}

export function filmKey(name: string, year: number | null): string {
  return `${name.trim().toLowerCase()}|${year ?? ""}`;
}

export function resolveTheme(theme: Theme): "dark" | "light" {
  if (theme !== "system") return theme;
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}
