export type LibraryCoverage = {
  uniqueMovies: number;
  totalViewings: number;
  ratingEvents: number;
  unresolvedMovies: number;
  source: "export" | "rss" | "mixed" | "none";
  fullHistoryAvailable: boolean;
  rssWindowLimit?: number;
  lastFullImport?: string;
  warnings: string[];
};

export type AppSession = {
  selfUsername: string | null;
  friendCount: number;
  hasSetup: boolean;
  coverage: LibraryCoverage;
};

export type InstallInfo = {
  version: string;
  installKind: "dev" | "installed" | "portable" | "unknown";
  appDataDir: string;
  databasePath: string;
  executablePath: string | null;
  uninstallerPath: string | null;
  logPath: string;
};

export type TmdbKeyStatus = {
  stored: boolean;
  valid: boolean | null;
  kind: string | null;
  lastError: string | null;
};

export type EnrichReport = {
  hasKey: boolean;
  keyValid: boolean | null;
  attempted: number;
  matched: number;
  posters: number;
  remainingUnmatched: number;
  remainingWithoutPoster: number;
  errors: number;
  lastError: string | null;
  logPath: string | null;
};

export type JobProgress = {
  job: string;
  label: string;
  current: number;
  total: number;
  posters: number;
  errors: number;
  done: boolean;
  enrich?: EnrichReport | null;
  import?: ImportResult | null;
};

export type LibraryItem = {
  id: string;
  title: string;
  year: number | null;
  currentRating: number | null;
  poster: string | null;
  watched: boolean;
  watchlist: boolean;
  liked: boolean;
  viewingCount: number;
  matchState: string;
  sourceType: string;
  lastWatchedAt: string | null;
};

export type LibraryPage = {
  items: LibraryItem[];
  total: number;
  coverage: LibraryCoverage;
};

export type ViewingHistoryItem = {
  id: string;
  occurredAt: string | null;
  publishedAt: string | null;
  rewatch: boolean;
  rating: number | null;
  source: string;
};

export type FriendActivityItem = {
  username: string;
  title: string;
  year: number | null;
  rating: number | null;
  review: string | null;
  watchedAt: string | null;
  publishedAt: string | null;
  poster: string | null;
};

export type FilmDetail = {
  id: string;
  title: string;
  year: number | null;
  currentRating: number | null;
  poster: string | null;
  backdrop: string | null;
  overview: string | null;
  runtime: number | null;
  genres: string[];
  matchState: string;
  sourceIdentity: string;
  yourHistory: ViewingHistoryItem[];
  friends: FriendActivityItem[];
  tmdbVoteAverage: number | null;
  tmdbVoteCount: number | null;
  tmdbReviews: string[];
  cast: string[];
  crew: string[];
  similar: LibraryItem[];
};

export type HomeViewModel = {
  coverage: LibraryCoverage;
  recent: LibraryItem[];
  topRated: LibraryItem[];
  friendFeed: FriendActivityItem[];
};

export type ImportResult = {
  importId: string;
  movies: number;
  viewings: number;
  ratings: number;
  skipped: number;
  warnings: string[];
  coverage: LibraryCoverage;
};

export type LibraryQuery = {
  search?: string;
  sort?: "recent" | "rating" | "title" | "year";
  filter?: string;
  limit?: number;
  offset?: number;
};

export type FriendRow = {
  id: string;
  username: string;
  lastSyncAt: string | null;
  lastSyncError: string | null;
};
