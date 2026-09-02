# Studio movie-detail plan

## Purpose

Turn Studio's movie-detail page into two clearly separated layers:

1. A complete, neutral reference for the film: identity, story, cast, crew, and production companies.
2. A personal decision layer: the user's documented history with people/companies involved, plus an evidence-based Taste fit.

The page must stay useful when TMDB enrichment or Taste analysis is unavailable. Personal signals must supplement factual credits; they must never hide them or be presented as objective judgments about a person or company.

## Current implementation snapshot

- Detail selection is app-local state in `studio/src/App.tsx`, not a URL route. Selecting a film sets `selectedFilmId`; Escape and the nav Back button clear it.
- `FilmDetailView.tsx` calls the Tauri `film_get` command through `getFilm` in `studio/src/platform/filmLibrary.ts`.
- `queries.rs` returns a `FilmDetail` assembled from SQLite's imported Library records and enriched TMDB movie records.
- TMDB enrichment already requests `/movie/{id}?append_to_response=credits,reviews,recommendations,similar,keywords` and stores posters, backdrop, synopsis, genres, runtime, ratings, collections, similar films, and credit JSON.
- The frontend currently receives cast/crew as flattened strings. It cannot use TMDB person IDs, characters/jobs as structured data, or profile images.
- The Taste engine already creates deterministic candidate scores and explanations from ratings, feedback, metadata, semantic similarity, and credited people. Its output is exposed for recommendation cards, not arbitrary detail pages.

## Product rules

- “Your rating,” “TMDB rating,” and “Taste fit” are different concepts and must never share an unlabeled run of star glyphs.
- Taste fit means evidence-weighted compatibility with demonstrated taste. It is not a probability and not a predicted Letterboxd rating.
- Factual information is complete by default. Long lists may be dense or collapsible, but must be discoverable without a new page.
- The global navigation must not move when a film opens.
- No network call occurs merely because a detail page rendered. Use cached/local data and explicit refresh/enrichment actions.
- Every personal signal shows its evidence: shared-title count, the user's average rating, a confidence/sample-size label, and supporting/opposing films where applicable.
- “Production company” is the accurate initial label. Do not promise distributor/studio-of-release data unless a later source can supply it reliably.

## Implementation sequence

### Step 1 — Stabilize detail navigation and selection context

**Goal:** Keep global navigation stationary and make the page-local return action truthful.

**Work**

- Replace the nav-level Back button with a detail-page Back action in the hero/content area.
- Store the route from which a film was opened (`home`, `films`, `recs`, etc.) alongside the selected film ID.
- Label the action `Back to Home`, `Back to Films`, or `Back to Taste`; retain a safe generic `Back` fallback for unknown sources.
- Preserve Escape as a keyboard close action and return focus to the originating card when practical.
- Keep the navbar tabs and centered search field at exactly the same coordinates on detail and non-detail screens.

**Likely files**

- `studio/src/App.tsx`
- `studio/src/features/films/FilmDetailView.tsx`
- `studio/src/materials.css`

**Acceptance checks**

- Open a film from Home, Films, Taste, collection, and Similar films; Back returns to the correct source view.
- The Home tab's left edge does not move when opening/closing a film.
- Escape closes a detail page without breaking command-palette Escape behavior.
- Keyboard focus is not lost after returning.

### Step 2 — Rebuild the detail hero as the cinematic identity area

**Goal:** Give film art more room than the existing short detail hero without obscuring key text.

**Work**

- Raise the detail-hero height from its current `min(42vh, 400px)` toward roughly `min(58vh, 620px)` after visual QA.
- Reuse Home's controlled lower image/scrim bleed so the image fades naturally into the page rather than ending abruptly.
- Keep the poster, title, principal creators, and essential metadata anchored to the lower-left safe area.
- Preserve a readable fallback hero for missing or low-resolution backdrops.
- Review object positioning on portrait-heavy and horizon-heavy backdrops; avoid a one-size crop that hides faces.

**Likely files**

- `studio/src/features/films/FilmDetailView.tsx`
- `studio/src/materials.css`
- `studio/src/core/images.ts` if banner-quality rules need adjustment

**Acceptance checks**

- Moana 2-style art reaches lower than Home's apparent image edge and fades cleanly into content.
- Text remains readable on bright and dark backdrops.
- Missing-backdrop detail pages remain intentional rather than visually empty.
- No horizontal overflow at desktop or narrow widths.

### Step 3 — Make rating and Taste-fit semantics unambiguous

**Goal:** Replace the current ambiguous two-star sequence with three labelled facts.

**Work**

- Present personal rating as `Your rating` with the existing editable rating control for logged films.
- Present community data as muted `TMDB` text: converted five-point value plus vote count when available.
- Do not introduce Taste fit yet unless its backend data is available; reserve a stable visual slot so Step 10 does not reshuffle the hero.
- For catalog/unlogged films, use `Not in your log` or a clear add/rate affordance—never an empty personal rating that resembles an error.

**Likely files**

- `studio/src/features/films/FilmDetailView.tsx`
- `studio/src/features/films/RatingDisplay.tsx`
- `studio/src/materials.css`

**Acceptance checks**

- A first-time user can explain what each number represents without hovering.
- A logged film has one editable personal rating; TMDB data is never interactive.
- The layout stays compact with absent TMDB data, no user rating, or a very large vote count.

### Step 4 — Establish the compact, responsive information layout

**Goal:** Use wide desktop space intentionally while keeping prose readable.

**Work**

- Replace the single vertical `detail-body` stack with a desktop primary column and a narrow supporting rail.
- Keep About/synopsis in a 65–75ch reading column.
- Place personal history and Friends in the supporting rail only when they contain real activity.
- Keep collection and similar-film shelves full-width below the factual sections.
- On narrow screens, collapse to one column in reading order: About → history/friends → credits → related shelves.
- Do not add decorative cards; use existing rules, spacing, and section dividers.

**Likely files**

- `studio/src/features/films/FilmDetailView.tsx`
- `studio/src/materials.css`

**Acceptance checks**

- Empty Friends data does not spend a full section of vertical space.
- Synopsis does not wrap prematurely or span the entire ultrawide screen.
- Related shelves keep their existing card behavior.
- Desktop, 960px, and 720px layouts remain readable and structurally consistent.

### Step 5 — Improve factual cast and crew presentation using current data

**Goal:** Make all currently available credits easy to scan before widening the data model.

**Work**

- Keep a small top-billed cast row for fast recognition.
- Add a complete cast directory beneath it, using the currently returned cast strings and a dense responsive layout.
- Group current crew records by department; preserve all available names within each group instead of showing only a few.
- Add explicit counts and expandable controls where needed, with accessible labels such as `Show all 32 cast members`.
- Do not attach Taste colors/scores to factual rows in this step.

**Likely files**

- `studio/src/features/films/FilmDetailView.tsx`
- `studio/src/materials.css`

**Acceptance checks**

- Every cast/crew item currently returned by `FilmDetail` is reachable from the page.
- Long names, characters, and multiple crew roles wrap without clipping.
- Expand/collapse works by mouse, keyboard, and screen reader.

### Step 6 — Persist structured credit and production-company data

**Goal:** Upgrade the storage model so factual credits and production entities are not flattened too early.

**Work**

- Add typed Rust models for people and production companies.
- Preserve, at minimum, person TMDB ID, name, profile path, department/job, and character where present.
- Preserve all relevant crew jobs rather than only the current curated display list; decide and document a sensible upper cap if required.
- Add a migration/column for `production_companies_json`; include TMDB ID, name, logo path when available, and origin country.
- Consider storing production countries/languages in the same pass if they are intended for the neutral About area.
- Increment the SQLite schema version and write migration coverage.

**Likely files**

- `studio/src-tauri/src/storage/schema.rs`
- `studio/src-tauri/src/storage/db.rs`
- `studio/src-tauri/src/models.rs`
- `studio/src-tauri/src/catalog/tmdb.rs`

**Acceptance checks**

- Existing databases migrate safely without loss of library, rating, or Taste data.
- New enrichment retains person IDs/profile paths and company records.
- Re-enrichment is idempotent and does not duplicate credits or companies.
- A missing TMDB field produces an empty optional value, not a failed enrichment.

### Step 7 — Expose structured credits and companies through the detail API

**Goal:** Make the new facts available to React without altering unrelated Library payloads.

**Work**

- Extend the Rust `FilmDetail` response and TypeScript `FilmDetail` type with structured cast, crew, and companies.
- Update `queries.rs` to parse structured JSON and return it for both logged and catalog films.
- Keep legacy string fields temporarily only if needed to avoid a risky all-at-once UI migration; then remove them once callers are migrated.
- Add production-company and neutral production metadata sections to the detail page.
- Use local image URLs through existing TMDB image helpers; show initials/name fallbacks for absent profile/logos.

**Likely files**

- `studio/src-tauri/src/models.rs`
- `studio/src-tauri/src/queries.rs`
- `studio/src/platform/types/film.ts`
- `studio/src/features/films/FilmDetailView.tsx`
- `studio/src/core/images.ts`

**Acceptance checks**

- Structured person cards render real photos when present and reliable text fallbacks when absent.
- Full cast/crew and production-company sections show source facts only.
- Logged and catalog films return the same neutral enrichment where available.

### Step 8 — Build local personal-history aggregation for people and companies

**Goal:** Calculate explainable user-specific signals without adding network work during page render.

**Work**

- Add a local query/service that intersects each credited person/company with the user's rated library history.
- Return a stable per-entity summary: shared-film count, average personal rating, rating distribution, confidence/sample-size band, loved/mixed/disliked title IDs, and most recent shared title.
- Center interpretation against the user's own rating baseline so a generally generous rater is not described as “loving” every entity.
- Use conservative copy for companies: association/history, not causal creative credit.
- Establish display thresholds: show neutral evidence at one shared film; show positive/negative labels only after an agreed minimum sample and separation from the user's baseline.
- Cache computed summaries by library/rating revision. They should be invalidated after import, sync, or rating changes.

**Likely files**

- `studio/src-tauri/src/queries.rs` or a new focused detail-context module
- `studio/src-tauri/src/models.rs`
- `studio/src-tauri/src/commands.rs`
- `studio/src/platform/filmLibrary.ts`
- `studio/src/platform/types/film.ts`

**Acceptance checks**

- A person with one shared film is not given a strong positive/negative verdict.
- Each displayed judgment links to concrete titles and ratings.
- Company history uses only locally known shared films until fuller company-filmography support is deliberately added.
- Re-rating a film refreshes affected detail summaries.

### Step 9 — Implement the neutral-to-personal “Your connection” section

**Goal:** Help decision-making without contaminating complete factual credits.

**Work**

- Add a dedicated section after Production, titled `Your connection to this film`.
- Summarize positive, mixed, negative, and unknown connections with text labels plus restrained semantic color.
- Show only the most decision-relevant entities initially; include `Show all connections` for the full local evidence set.
- Each entity opens an inline disclosure or portal popover with supporting/opposing shared films.
- Keep neutral Cast, Crew, and Production sections uncolored and complete.

**Likely files**

- `studio/src/features/films/FilmDetailView.tsx`
- a new small detail-context component if it keeps the view readable
- `studio/src/materials.css`

**Acceptance checks**

- A user can see every factual credit without encountering a personalized verdict.
- Personal labels are understandable with color disabled or by assistive technology.
- Opening one evidence panel does not overlap or reflow unrelated credits.

### Step 10 — Add a reusable film-detail Taste-fit backend endpoint

**Goal:** Score any enriched detail film with the existing Taste model and explanation vocabulary.

**Work**

- Add a Tauri command conceptually named `film_taste_detail(filmId)`.
- Resolve the film's local metadata and construct the same candidate shape used by Taste recommendations: TMDB ID, genres, keywords, structured credits, runtime, popularity/vote context, and watchlist state.
- Reuse the existing Taste profile, deterministic scorer, semantic signal where available, confidence calculation, and matched-feature explanation types.
- Return fit score, evidence grade, semantic coverage/fit, cited positive/negative features, evidence titles, and explicit insufficient-data states.
- Cache results by film ID plus a Taste-profile/library revision; do not run the AI reasoner when opening a movie.
- Keep the endpoint read-only and ensure it cannot alter recommendation feedback/exposure state.

**Likely files**

- `studio/src-tauri/src/taste/score.rs`
- `studio/src-tauri/src/taste/confidence.rs`
- `studio/src-tauri/src/taste/mod.rs`
- `studio/src-tauri/src/taste/retrieve.rs`
- `studio/src-tauri/src/commands.rs`
- `studio/src/platform/filmLibrary.ts`
- `studio/src/platform/types/film.ts`

**Acceptance checks**

- Detail fit is consistent with the same film when it appears in a Taste recommendation result.
- The UI never calls the result a probability or a predicted user rating.
- No Taste key/report produces a calm unavailable state with guidance to analyze Taste.
- Weak evidence is visibly described as weak rather than rendered as a confident low score.

### Step 11 — Present Taste fit as a compact decision aid

**Goal:** Surface the score without making the detail page feel like the Taste page.

**Work**

- Add a labelled `Taste fit` unit in the hero/side rail, separate from both ratings.
- Pair the score with a verbal band: `Strong fit`, `Mixed evidence`, `Weak fit`, or `Not enough evidence`.
- Use green/yellow-orange/muted-red only as a supporting semantic token; never color alone.
- Add a `Why this fit?` info trigger using the existing safe portal/popover pattern.
- Show evidence quality, top supporting signals, top counter-signals, and related personal titles in the popover.
- For watched films, make personal history primary and frame Taste fit as Studio's prediction/interpretation.

**Likely files**

- `studio/src/features/films/FilmDetailView.tsx`
- `studio/src/features/films/RecsView.tsx` if a small existing popover primitive is extracted
- `studio/src/materials.css`

**Acceptance checks**

- A user can distinguish Taste fit from both existing ratings at a glance.
- Keyboard, click-outside, Escape, focus return, and reduced-motion behavior match the Taste-page popover.
- The popover never overlaps/clips under the footer or viewport edge.

### Step 12 — Verification, migration QA, and polish

**Goal:** Ship the redesign without regressions to the local-first data model or accessibility.

**Work**

- Add focused Rust tests for migration, structured-credit parsing, company parsing, and history aggregation thresholds.
- Add frontend tests for hero labels, back behavior, expand/collapse, unavailable states, and Taste-fit semantics.
- Run TypeScript checks, focused tests, full test suite, Vite production build, and Tauri build/dev smoke tests.
- Manually inspect at least: enriched logged film, catalog film, no-TMDB film, long cast list, missing profile images, no Taste report, strong Taste fit, weak evidence, light/dark/system themes, desktop, 960px, and 720px.
- Confirm cache invalidation after re-rating, syncing, importing, and re-enriching.
- Review database migration against a copy of an existing user database before release.

**Acceptance checks**

- No current Library, Home, Taste, or Settings behavior regresses.
- No destructive database action is required to get structured credits/companies.
- The detail page remains fast when offline after data is already cached.
- All new meanings remain understandable without color or hover.

## Suggested working batches

| Request | Outcome |
| --- | --- |
| `Complete steps 1-2` | Stable navigation and a better cinematic hero. |
| `Complete steps 3-5` | Clear ratings, compact layout, and better complete credits using today’s data. |
| `Complete steps 6-7` | Schema/API/enrichment work for structured people and production companies. |
| `Complete steps 8-9` | Evidence-based people/company history and the separate personal-connections section. |
| `Complete steps 10-11` | Reusable Taste fit plus its explanatory UI. |
| `Complete step 12` | Automated checks, migration testing, and visual QA. |

## Explicit non-goals for this plan

- A public social graph or new Friends-page feature.
- A separate person-detail route before the inline evidence pattern proves insufficient.
- Continuous external TMDB requests while browsing detail pages.
- Treating company association as proof of creative authorship.
- Replacing the user's rating with algorithmic Taste output.
