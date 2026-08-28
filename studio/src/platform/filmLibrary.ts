import { invoke } from "@tauri-apps/api/core";
import type {
  AppSession,
  EnrichReport,
  FilmDetail,
  FriendRow,
  HomeViewModel,
  ImportResult,
  InstallInfo,
  LibraryCoverage,
  LibraryPage,
  LibraryQuery,
  TasteFeedback,
  TasteKeyStatus,
  TasteState,
  TmdbKeyStatus,
} from "./types/film";

export async function getSession(): Promise<AppSession> {
  return invoke("get_session");
}

export async function setSelfUsername(username: string): Promise<void> {
  return invoke("set_self_username", { username });
}

export async function getInstallInfo(): Promise<InstallInfo> {
  return invoke("get_install_info");
}

export async function resetAllData(): Promise<void> {
  return invoke("reset_all_data");
}

export async function launchUninstaller(): Promise<void> {
  return invoke("launch_uninstaller");
}

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

export async function importExportZip(path: string): Promise<void> {
  return invoke("import_export_zip", { path });
}

export async function syncSelf(username: string): Promise<void> {
  return invoke("sync_self", { username });
}

export async function syncFriends(): Promise<void> {
  return invoke("sync_friends");
}

export async function syncFeeds(force = false): Promise<boolean> {
  return invoke("sync_feeds", { force });
}

export async function importFriendUsernames(text: string): Promise<number> {
  return invoke("import_friend_usernames", { text });
}

export async function removeFriend(id: string): Promise<string> {
  return invoke("remove_friend", { id });
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

export async function tmdbSetKey(key: string): Promise<TmdbKeyStatus> {
  return invoke("tmdb_set_key", { key });
}

export async function tmdbClearKey(): Promise<TmdbKeyStatus> {
  return invoke("tmdb_clear_key");
}

export async function tmdbHasKey(): Promise<boolean> {
  return invoke("tmdb_has_key");
}

export async function tmdbKeyStatus(): Promise<TmdbKeyStatus> {
  return invoke("tmdb_key_status");
}

export async function tmdbEnrich(): Promise<void> {
  return invoke("tmdb_enrich");
}

export async function tasteKeyStatus(): Promise<TasteKeyStatus> {
  return invoke("taste_key_status");
}

export async function tasteSetKey(key: string): Promise<TasteKeyStatus> {
  return invoke("taste_set_key", { key });
}

export async function tasteClearKey(): Promise<TasteKeyStatus> {
  return invoke("taste_clear_key");
}

export async function tasteSetModel(model: string): Promise<TasteKeyStatus> {
  return invoke("taste_set_model", { model });
}

export async function tasteSetWeb(enabled: boolean): Promise<TasteKeyStatus> {
  return invoke("taste_set_web", { enabled });
}

export async function tasteGet(): Promise<TasteState> {
  return invoke("taste_get");
}

export async function tasteAnalyze(forceRefresh = false): Promise<void> {
  return invoke("taste_analyze", { forceRefresh });
}

export async function tasteFeedbackSet(
  tmdbId: number,
  action: "interested" | "rejected" | "seen",
  reason?: string | null,
): Promise<TasteFeedback> {
  return invoke("taste_feedback_set", { tmdbId, action, reason: reason ?? null });
}

export async function tasteFeedbackClear(tmdbId: number): Promise<void> {
  return invoke("taste_feedback_clear", { tmdbId });
}

export function formatEnrich(r: EnrichReport): string {
  if (r.keyValid === false) {
    return r.lastError ?? "TMDB rejected the saved key";
  }
  const parts = [
    `matched ${r.matched}/${r.attempted}`,
    `${r.posters} posters`,
    `${r.remainingUnmatched} unmatched`,
    `${r.remainingWithoutPoster} still without a poster`,
  ];
  if (!r.hasKey) parts.unshift("no TMDB key");
  if (r.errors) parts.push(`${r.errors} errors`);
  if (r.lastError) parts.push(r.lastError);
  return parts.join(" · ");
}

export function formatImport(r: ImportResult): string {
  return `Imported ${r.movies} films · ${r.viewings} viewings · ${r.ratings} ratings · ${r.skipped} already present`;
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

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = value >= 10 || unit === 0 ? 0 : 1;
  return `${value.toFixed(digits)} ${units[unit]}`;
}

export function formatLibrarySummary(c: LibraryCoverage): string {
  const parts = [
    `${c.uniqueMovies} films`,
    `${c.watchlistMovies} on watchlist`,
    `${c.totalViewings} viewings`,
  ];
  if (c.fullHistoryAvailable && c.lastFullImport) {
    parts.push(`export ${new Date(c.lastFullImport).toLocaleDateString()}`);
  } else {
    parts.push("no full export");
  }
  if (c.rssWindowLimit) {
    parts.push(`RSS last ${c.rssWindowLimit}`);
  }
  return parts.join(" · ");
}

export function formatRssSyncAt(iso: string | null | undefined): string {
  if (!iso) return "Not yet";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "Not yet";
  return date.toLocaleString();
}
