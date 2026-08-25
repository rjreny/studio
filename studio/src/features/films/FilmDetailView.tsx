import { useEffect, useMemo, useState } from "react";
import { communityRatingOutOfFive, isHighQualityBanner } from "../../core/images";
import { log } from "../../platform/log";
import { getFilm, setRating } from "../../platform/filmLibrary";
import type { FilmDetail, LibraryItem, ViewingHistoryItem } from "../../platform/types/film";
import { FilmCard } from "./FilmCard";
import { RatingControl, RatingDisplay } from "./RatingDisplay";
import { Shelf } from "./Shelf";

function runtimeLabel(minutes: number | null) {
  if (!minutes) return null;
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  if (h <= 0) return `${m} min`;
  return m ? `${h}h ${m}m` : `${h}h`;
}

function formatWatchDate(iso: string | null | undefined) {
  if (!iso) return "Unknown date";
  const day = iso.slice(0, 10);
  const d = new Date(`${day}T12:00:00`);
  if (Number.isNaN(d.getTime())) return day || iso;
  return d.toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
}

function sourceLabel(source: string) {
  const raw = source.replace(/^\d+\./, "").replace(/_/g, " ").trim();
  if (/letterboxd export/i.test(raw)) return "Letterboxd export";
  if (/letterboxd rss/i.test(raw)) return "Letterboxd RSS";
  if (/local/i.test(raw)) return "Studio";
  return raw.replace(/\b\w/g, (c) => c.toUpperCase());
}

function parseCredit(item: string): { name: string; role: string } {
  const job = item.match(/^(.*) \((.+)\)$/);
  if (job) return { name: job[1], role: job[2] };
  const as = item.match(/^(.*) as (.*)$/i);
  if (as) return { name: as[1], role: as[2] };
  return { name: item, role: "" };
}

const CREW_ORDER = [
  "Director",
  "Writer",
  "Screenplay",
  "Original Screenplay",
  "Story",
  "Novel",
  "Characters",
  "Director of Photography",
  "Cinematography",
  "Original Music Composer",
  "Music",
  "Editor",
  "Production Design",
  "Art Direction",
  "Costume Design",
  "Casting",
  "Sound Designer",
  "Sound Mixer",
  "Visual Effects Supervisor",
  "Producer",
];

const CREW_LABEL: Record<string, string> = {
  "Director of Photography": "Cinematography",
  "Original Music Composer": "Composer",
  Music: "Composer",
};

function groupCrew(items: string[]) {
  const groups = new Map<string, string[]>();
  items.forEach((item) => {
    const credit = parseCredit(item);
    const key = credit.role || "Crew";
    const names = groups.get(key) ?? [];
    if (!names.includes(credit.name)) names.push(credit.name);
    groups.set(key, names);
  });
  const known = CREW_ORDER.filter((job) => groups.has(job)).map((job) => ({
    job,
    label: CREW_LABEL[job] ?? job,
    names: groups.get(job) ?? [],
  }));
  return known;
}

function CastList({ items }: { items: string[] }) {
  if (!items.length) return <p className="muted">Not listed yet.</p>;
  const shown = items.slice(0, 10);
  const extra = items.length - shown.length;
  return (
    <ul className="credit-inline">
      {shown.map((item) => {
        const credit = parseCredit(item);
        return (
          <li key={item}>
            <strong>{credit.name}</strong>
            {credit.role ? <span>{credit.role}</span> : null}
          </li>
        );
      })}
      {extra > 0 ? <li className="credit-more">+{extra} more</li> : null}
    </ul>
  );
}

function CrewList({ items }: { items: string[] }) {
  const groups = useMemo(() => groupCrew(items), [items]);
  if (!groups.length) return <p className="muted">Not listed yet.</p>;
  return (
    <dl className="crew-groups">
      {groups.map((group) => {
        const shown = group.names.slice(0, 4);
        const extra = group.names.length - shown.length;
        return (
          <div key={group.job} className="crew-group">
            <dt>{group.label}</dt>
            <dd>
              {shown.join(", ")}
              {extra > 0 ? <span className="muted"> +{extra}</span> : null}
            </dd>
          </div>
        );
      })}
    </dl>
  );
}

function HistoryList({ items }: { items: ViewingHistoryItem[] }) {
  return (
    <ul className="history-list">
      {items.map((v) => (
        <li key={v.id} className="history-row">
          <strong>{formatWatchDate(v.occurredAt ?? v.publishedAt)}</strong>
          <RatingDisplay value={v.rating} compact />
          {v.rewatch ? <span className="rewatch-badge">Rewatch</span> : null}
          <span className="muted">{sourceLabel(v.source)}</span>
        </li>
      ))}
    </ul>
  );
}

function RelatedShelf({
  title,
  source,
  films,
  onSelect,
}: {
  title: string;
  source: string;
  films: LibraryItem[];
  onSelect?: (id: string) => void;
}) {
  if (!films.length) return null;
  return (
    <section className="detail-block">
      <Shelf title={title}>
        {films.map((film) => (
          <FilmCard key={film.id} film={film} onSelect={onSelect} />
        ))}
      </Shelf>
      <p className="section-source">{source}</p>
    </section>
  );
}

export function FilmDetailView({
  filmId,
  onUpdated,
  onStatus,
  onSelectFilm,
}: {
  filmId: string;
  onBack: () => void;
  onUpdated: () => Promise<void>;
  onStatus: (s: string) => void;
  onSelectFilm?: (id: string) => void;
}) {
  const [film, setFilm] = useState<FilmDetail | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setFilm(null);
    setError(null);
    void getFilm(filmId)
      .then(setFilm)
      .catch((err) => {
        log("error", "film load failed", err);
        setError(err instanceof Error ? err.message : String(err));
        onStatus("Could not load film");
      });
  }, [filmId, onStatus]);

  if (error) {
    return (
      <div className="pad page-pad">
        <p className="muted">Could not load this film. {error}</p>
      </div>
    );
  }

  if (!film) return <p className="muted pad">Loading…</p>;

  async function rate(value: number) {
    if (!film) return;
    const next = await setRating(filmId, value);
    setFilm(next);
    await onUpdated();
    onStatus(`Rated ${next.title} ${value}`);
  }

  const image = isHighQualityBanner(film.backdrop) ? film.backdrop : null;
  const directors = film.directors?.length ? film.directors : [];
  const directorLine = directors.join("  ").toUpperCase();
  const runtime = runtimeLabel(film.runtime);
  const community = communityRatingOutOfFive(film.tmdbVoteAverage);
  const canRate = film.matchState !== "catalog";

  return (
    <article className="film-detail">
      <header className="hero detail-hero">
        {image ? <img className="hero-image" src={image} alt="" /> : <div className="hero-image is-empty" />}
        <div className="hero-scrim" />
        <div className="hero-copy">
          {film.poster ? <img className="detail-poster" src={film.poster} alt="" /> : null}
          <div className="hero-copy-text">
            {directorLine ? <p className="hero-cast">{directorLine}</p> : null}
            <h1>{film.title}</h1>
            <p className="hero-meta">
              <span>{film.year ?? "Year unknown"}</span>
              {runtime ? <span>{runtime}</span> : null}
              {film.genres[0] ? <span>{film.genres[0]}</span> : null}
              <RatingDisplay value={community} compact />
              {canRate ? (
                <RatingControl value={film.currentRating} onChange={(v) => void rate(v)} />
              ) : (
                <span className="muted">Not in your log</span>
              )}
            </p>
            {film.tagline ? <p className="hero-lede">{film.tagline}</p> : null}
          </div>
        </div>
      </header>

      <div className="detail-body">
        {canRate ? (
          <section className="detail-block">
            <h2>Your history</h2>
            {film.yourHistory.length ? (
              <HistoryList items={film.yourHistory} />
            ) : (
              <p className="muted">No diary entries for this film yet.</p>
            )}
          </section>
        ) : null}

        <section className="detail-block">
          <h2>Friends</h2>
          {film.friends.length ? (
            <ul className="friend-chips">
              {film.friends.map((f, idx) => (
                <li key={`${f.username}-${idx}`}>
                  <strong>@{f.username}</strong>
                  <RatingDisplay value={f.rating} compact />
                  {f.review ? <span className="muted">{f.review}</span> : null}
                </li>
              ))}
            </ul>
          ) : (
            <p className="muted">No friend activity for this film yet.</p>
          )}
        </section>

        <section className="detail-block">
          <h2>About</h2>
          {film.overview ? <p className="detail-overview">{film.overview}</p> : <p className="muted">Not enriched yet.</p>}
          {film.genres.length ? <p className="genre-row">{film.genres.join(" · ")}</p> : null}
        </section>

        <section className="detail-block">
          <h2>Cast</h2>
          <CastList items={film.cast} />
        </section>

        <section className="detail-block">
          <h2>Crew</h2>
          <CrewList items={film.crew} />
        </section>

        <RelatedShelf
          title={film.collectionName ? film.collectionName : "Related films"}
          source="TMDB collection / sequels"
          films={film.collection ?? []}
          onSelect={onSelectFilm}
        />

        <RelatedShelf
          title="Similar films"
          source="TMDB recommendations"
          films={film.similar}
          onSelect={onSelectFilm}
        />
      </div>
    </article>
  );
}
