import { invoke } from "@tauri-apps/api/core";
import type {
  FilmDetail,
  FriendRow,
  HomeViewModel,
  ImportResult,
  LibraryCoverage,
  LibraryPage,
  LibraryQuery,
} from "./types/film";

export async function getCoverage(): Promise<LibraryCoverage> {
  return invoke("get_coverage");
}

export async function getLibrary(query: LibraryQuery = {}): Promise<LibraryPage> {
  return invoke("library_get", { query });
}

export async function getFilm(id: string): Promise<FilmDetail> {
  return invoke("film_get", { id });
}

export async function getHome(): Promise<HomeViewModel> {
  return invoke("home_get");
}

export async function importExportZip(path: string): Promise<ImportResult> {
  return invoke("import_export_zip", { path });
}

export async function syncSelf(username: string): Promise<{
  username: string;
  entriesSeen: number;
  entriesAdded: number;
  coverage: LibraryCoverage;
}> {
  return invoke("sync_self", { username });
}

export async function syncFriends(): Promise<{
  friendsSynced: number;
  entriesAdded: number;
  errors: string[];
}> {
  return invoke("sync_friends");
}

export async function importFriendUsernames(text: string): Promise<number> {
  return invoke("import_friend_usernames", { text });
}

export async function setRating(id: string, rating: number): Promise<FilmDetail> {
  return invoke("film_set_rating", { input: { id, rating } });
}

export async function importGetDiagnostics() {
  return invoke<{ imports: unknown[]; warnings: string[] }>("import_get_diagnostics");
}

export async function migrateFromLegacy(legacy: {
  username?: string;
  films?: unknown[];
  friends?: unknown[];
}): Promise<{
  status: string;
  migrationVersion: number;
  validationResult: string;
  coverage: LibraryCoverage;
}> {
  return invoke("migrate_from_legacy", { legacy });
}

export async function tmdbSetKey(key: string) {
  return invoke("tmdb_set_key", { key });
}

export async function tmdbClearKey() {
  return invoke("tmdb_clear_key");
}

export async function tmdbHasKey(): Promise<boolean> {
  return invoke("tmdb_has_key");
}

export async function tmdbEnrich(): Promise<number> {
  return invoke("tmdb_enrich");
}

export async function listFriends(): Promise<FriendRow[]> {
  const rows = await invoke<[string, string, string | null, string | null][]>("list_friends");
  return rows.map(([id, username, lastSyncAt, lastSyncError]) => ({
    id,
    username,
    lastSyncAt,
    lastSyncError,
  }));
}

export function formatCoverage(c: LibraryCoverage): string {
  const parts = [
    `${c.uniqueMovies} unique films`,
    `${c.totalViewings} recorded viewings`,
  ];
  if (c.fullHistoryAvailable && c.lastFullImport) {
    parts.push(`Full export: ${new Date(c.lastFullImport).toLocaleDateString()}`);
  } else {
    parts.push("Full export: not imported");
  }
  if (c.rssWindowLimit) {
    parts.push(`RSS: latest ${c.rssWindowLimit} entries, incremental only`);
  }
  return parts.join(" · ");
}
