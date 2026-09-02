import { invoke } from "@tauri-apps/api/core";
import type {
  AppSession,
  EnrichReport,
  FilmDetail,
  FilmTasteFit,
  FriendRow,
  HomeViewModel,
  ImportResult,
  InstallInfo,
  LibraryCoverage,
  LibraryPage,
  LibraryQuery,
  StatsSnapshot,
  TasteFeedback,
  TasteKeyStatus,
  TasteState,
  TmdbKeyStatus,
} from "./types/film";

const dataCache = new Map<string, Promise<unknown>>();

function cached<T>(key: string, load: () => Promise<T>): Promise<T> {
  const existing = dataCache.get(key);
  if (existing) return existing as Promise<T>;

  const request = load().catch((error) => {
    if (dataCache.get(key) === request) dataCache.delete(key);
    throw error;
  });
  dataCache.set(key, request);
  return request;
}

function libraryKey(query: LibraryQuery) {
  return [query.search, query.sort, query.filter, query.limit, query.offset].join("\u0000");
}

export function invalidateDataCache() {
  dataCache.clear();
}

export function invalidateTasteCache() {
  dataCache.delete("taste");
}

export function letterboxdFilmUrl(tmdbId: number): string | null {
  if (!Number.isSafeInteger(tmdbId) || tmdbId <= 0) return null;
  return `https://letterboxd.com/tmdb/${tmdbId}/`;
}

export async function getSession(): Promise<AppSession> {
  return cached("session", () => invoke<AppSession>("get_session"));
}

export async function setSelfUsername(username: string): Promise<void> {
  await invoke("set_self_username", { username });
  dataCache.delete("session");
}

export async function getInstallInfo(): Promise<InstallInfo> {
  return invoke("get_install_info");
}

export async function resetAllData(): Promise<void> {
  await invoke("reset_all_data");
  invalidateDataCache();
}

export async function launchUninstaller(): Promise<void> {
  return invoke("launch_uninstaller");
}

export async function getCoverage(): Promise<LibraryCoverage> {
  return cached("coverage", () => invoke<LibraryCoverage>("get_coverage"));
}

export async function getLibrary(query: LibraryQuery = {}): Promise<LibraryPage> {
  return cached(`library:${libraryKey(query)}`, () => invoke<LibraryPage>("library_get", { query }));
}

export async function getStats(): Promise<StatsSnapshot> {
  return cached("stats", () => invoke<StatsSnapshot>("stats_get"));
}

export async function getFilm(id: string): Promise<FilmDetail> {
  return cached(`film:${id}`, () => invoke<FilmDetail>("film_get", { id }));
}

export async function getFilmTasteDetail(id: string): Promise<FilmTasteFit> {
  return cached(`film-taste:${id}`, () => invoke<FilmTasteFit>("film_taste_detail", { id }));
}

export async function getHome(): Promise<HomeViewModel> {
  return cached("home", () => invoke<HomeViewModel>("home_get"));
}

export async function importExportZip(path: string): Promise<void> {
  await invoke("import_export_zip", { path });
  invalidateDataCache();
}

export async function syncSelf(username: string): Promise<void> {
  await invoke("sync_self", { username });
  invalidateDataCache();
}

export async function syncFriends(): Promise<void> {
  await invoke("sync_friends");
  invalidateDataCache();
}

export async function syncFeeds(force = false): Promise<boolean> {
  const started = await invoke<boolean>("sync_feeds", { force });
  if (started) invalidateDataCache();
  return started;
}

export async function importFriendUsernames(text: string): Promise<number> {
  const added = await invoke<number>("import_friend_usernames", { text });
  invalidateDataCache();
  return added;
}

export async function removeFriend(id: string): Promise<string> {
  const username = await invoke<string>("remove_friend", { id });
  invalidateDataCache();
  return username;
}

export async function setRating(id: string, rating: number): Promise<FilmDetail> {
  const film = await invoke<FilmDetail>("film_set_rating", { input: { id, rating } });
  invalidateDataCache();
  dataCache.set(`film:${id}`, Promise.resolve(film));
  return film;
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
  const result = await invoke<{
    status: string;
    migrationVersion: number;
    validationResult: string;
    coverage: LibraryCoverage;
  }>("migrate_from_legacy", { legacy });
  invalidateDataCache();
  return result;
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
  await invoke("tmdb_enrich");
  invalidateDataCache();
}

export async function tasteKeyStatus(): Promise<TasteKeyStatus> {
  return invoke("taste_key_status");
}

export async function tasteSetKey(key: string): Promise<TasteKeyStatus> {
  const status = await invoke<TasteKeyStatus>("taste_set_key", { key });
  invalidateTasteCache();
  return status;
}

export async function tasteClearKey(): Promise<TasteKeyStatus> {
  const status = await invoke<TasteKeyStatus>("taste_clear_key");
  invalidateTasteCache();
  return status;
}

export async function tasteSetModel(model: string): Promise<TasteKeyStatus> {
  const status = await invoke<TasteKeyStatus>("taste_set_model", { model });
  invalidateTasteCache();
  return status;
}

export async function tasteSetWeb(enabled: boolean): Promise<TasteKeyStatus> {
  const status = await invoke<TasteKeyStatus>("taste_set_web", { enabled });
  invalidateTasteCache();
  return status;
}

export async function tasteGet(): Promise<TasteState> {
  return cached("taste", () => invoke<TasteState>("taste_get"));
}

export async function tasteAnalyze(forceRefresh = false): Promise<void> {
  await invoke("taste_analyze", { forceRefresh });
  invalidateTasteCache();
}

export async function tasteFeedbackSet(
  tmdbId: number,
  action: "interested" | "rejected" | "seen",
  options: {
    reason?: string | null;
    exposureId?: string | null;
    targetFeatureKey?: string | null;
    moodScope?: "this_movie_only" | "this_kind_right_now" | null;
  },
): Promise<TasteFeedback> {
  const feedback = await invoke<TasteFeedback>("taste_feedback_set", {
    tmdbId,
    action,
    reason: options.reason ?? null,
    exposureId: options.exposureId ?? null,
    targetFeatureKey: options.targetFeatureKey ?? null,
    moodScope: options.moodScope ?? null,
  });
  invalidateTasteCache();
  return feedback;
}

export async function tasteFeedbackClear(tmdbId: number): Promise<void> {
  await invoke("taste_feedback_clear", { tmdbId });
  invalidateTasteCache();
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
  return cached("friends", async () => {
    const rows = await invoke<[string, string, string | null, string | null][]>("list_friends");
    return rows.map(([id, username, lastSyncAt, lastSyncError]) => ({
      id,
      username,
      lastSyncAt,
      lastSyncError,
    }));
  });
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
