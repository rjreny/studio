pub const SCHEMA_VERSION: i32 = 2;

pub const MIGRATION_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS app_meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS source_movie_records (
  id TEXT PRIMARY KEY,
  source_type TEXT NOT NULL,
  source_record_key TEXT NOT NULL UNIQUE,
  external_id TEXT,
  normalized_title TEXT NOT NULL,
  release_year INTEGER,
  raw_identity TEXT NOT NULL,
  cached_poster_url TEXT,
  poster_fetch_failed INTEGER NOT NULL DEFAULT 0,
  on_watchlist INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS movies (
  id TEXT PRIMARY KEY,
  canonical_title TEXT NOT NULL,
  release_year INTEGER,
  tmdb_id INTEGER,
  poster_path TEXT,
  backdrop_path TEXT,
  overview TEXT,
  runtime INTEGER,
  vote_average REAL,
  vote_count INTEGER,
  genres_json TEXT,
  cast_json TEXT,
  crew_json TEXT,
  similar_json TEXT,
  reviews_json TEXT,
  enriched_at TEXT
);

CREATE TABLE IF NOT EXISTS movie_links (
  source_movie_record_id TEXT PRIMARY KEY REFERENCES source_movie_records(id),
  movie_id TEXT REFERENCES movies(id),
  match_state TEXT NOT NULL,
  match_method TEXT,
  confidence REAL,
  confirmed_at TEXT
);

CREATE TABLE IF NOT EXISTS movie_aliases (
  movie_id TEXT NOT NULL REFERENCES movies(id),
  normalized_title TEXT NOT NULL,
  release_year INTEGER,
  PRIMARY KEY (movie_id, normalized_title, release_year)
);

CREATE TABLE IF NOT EXISTS viewings (
  id TEXT PRIMARY KEY,
  source_movie_record_id TEXT NOT NULL REFERENCES source_movie_records(id),
  source_record_key TEXT NOT NULL UNIQUE,
  occurred_at TEXT,
  published_at TEXT,
  observed_at TEXT NOT NULL,
  imported_at TEXT,
  source_type TEXT NOT NULL,
  import_id TEXT,
  diary_entry_id TEXT,
  rewatch INTEGER NOT NULL DEFAULT 0,
  raw_payload TEXT
);

CREATE TABLE IF NOT EXISTS rating_events (
  id TEXT PRIMARY KEY,
  source_movie_record_id TEXT NOT NULL REFERENCES source_movie_records(id),
  source_record_key TEXT NOT NULL UNIQUE,
  rating REAL NOT NULL,
  occurred_at TEXT,
  published_at TEXT,
  observed_at TEXT NOT NULL,
  imported_at TEXT,
  source_type TEXT NOT NULL,
  import_id TEXT
);

CREATE TABLE IF NOT EXISTS user_movie_state (
  source_movie_record_id TEXT PRIMARY KEY REFERENCES source_movie_records(id),
  movie_id TEXT REFERENCES movies(id),
  watched INTEGER NOT NULL DEFAULT 0,
  watchlist INTEGER NOT NULL DEFAULT 0,
  liked INTEGER NOT NULL DEFAULT 0,
  current_rating REAL,
  last_watched_at TEXT,
  projection_updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS imports (
  id TEXT PRIMARY KEY,
  source_type TEXT NOT NULL,
  content_hash TEXT NOT NULL,
  imported_at TEXT NOT NULL,
  status TEXT NOT NULL,
  raw_manifest TEXT
);

CREATE TABLE IF NOT EXISTS import_entries (
  id TEXT PRIMARY KEY,
  import_id TEXT NOT NULL REFERENCES imports(id),
  source_path TEXT NOT NULL,
  row_number INTEGER,
  entity_type TEXT NOT NULL,
  status TEXT NOT NULL,
  warning TEXT
);

CREATE TABLE IF NOT EXISTS friends (
  id TEXT PRIMARY KEY,
  username TEXT NOT NULL UNIQUE,
  enabled INTEGER NOT NULL DEFAULT 1,
  last_sync_at TEXT,
  last_sync_error TEXT
);

CREATE TABLE IF NOT EXISTS friend_activity (
  id TEXT PRIMARY KEY,
  friend_id TEXT NOT NULL REFERENCES friends(id),
  source_movie_record_id TEXT REFERENCES source_movie_records(id),
  source_record_key TEXT NOT NULL UNIQUE,
  activity_type TEXT NOT NULL,
  published_at TEXT,
  watched_at TEXT,
  rating REAL,
  review TEXT,
  source_guid TEXT,
  raw_payload TEXT,
  poster_url TEXT
);

CREATE INDEX IF NOT EXISTS idx_viewings_source_movie ON viewings(source_movie_record_id);
CREATE INDEX IF NOT EXISTS idx_rating_events_source_movie ON rating_events(source_movie_record_id);
CREATE INDEX IF NOT EXISTS idx_source_movie_title ON source_movie_records(normalized_title, release_year);
CREATE INDEX IF NOT EXISTS idx_movie_links_state ON movie_links(match_state);
"#;
