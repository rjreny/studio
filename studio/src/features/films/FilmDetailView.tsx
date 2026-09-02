import { useEffect, useMemo, useState } from "react";
import { communityRatingOutOfFive, isHighQualityBanner } from "../../core/images";
import { log } from "../../platform/log";
import { getFilm } from "../../platform/filmLibrary";
import type {
  FilmCastMember,
  FilmConnection,
  FilmCrewMember,
  FilmDetail,
  LibraryItem,
  ProductionCompany,
  ViewingHistoryItem,
} from "../../platform/types/film";
import { FilmCard } from "./FilmCard";
import { RatingDisplay } from "./RatingDisplay";
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

function initials(name: string) {
  return name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0])
    .join("")
    .toUpperCase();
}

function PeopleAvatar({ name, image }: { name: string; image: string | null }) {
  return image ? <img className="credit-avatar" src={image} alt="" /> : <span className="credit-avatar is-fallback" aria-hidden="true">{initials(name)}</span>;
}

function CastDirectory({ cast }: { cast: FilmCastMember[] }) {
  const [expanded, setExpanded] = useState(false);
  if (!cast.length) return <p className="muted">Not listed yet.</p>;
  const visibleCast = expanded ? cast : cast.slice(0, 16);
  const remaining = cast.length - visibleCast.length;
  return (
    <div className="credit-directory">
      <ul className="credit-cards" aria-label={`Cast directory, ${cast.length} members`}>
        {visibleCast.map((member) => (
          <li key={`${member.tmdbId ?? member.name}-${member.order ?? ""}`}>
            <PeopleAvatar name={member.name} image={member.profile} />
            <span><strong>{member.name}</strong>{member.character ? <small>{member.character}</small> : null}</span>
          </li>
        ))}
      </ul>
      {remaining > 0 ? (
        <button type="button" className="detail-expand" aria-expanded={expanded} onClick={() => setExpanded((value) => !value)}>
          {expanded ? "Show less cast" : `Show all ${cast.length} cast members`}
        </button>
      ) : null}
    </div>
  );
}

function groupCrew(crew: FilmCrewMember[]) {
  const groups = new Map<string, FilmCrewMember[]>();
  for (const member of crew) {
    const key = member.department || "Crew";
    const group = groups.get(key) ?? [];
    if (!group.some((existing) => existing.tmdbId === member.tmdbId && existing.job === member.job && existing.name === member.name)) group.push(member);
    groups.set(key, group);
  }
  return [...groups.entries()].map(([department, members]) => ({ department, members })).sort((a, b) => a.department.localeCompare(b.department));
}

function CrewDirectory({ crew }: { crew: FilmCrewMember[] }) {
  const [expanded, setExpanded] = useState(false);
  const groups = useMemo(() => groupCrew(crew), [crew]);
  if (!groups.length) return <p className="muted">Not listed yet.</p>;
  const visibleGroups = expanded ? groups : groups.slice(0, 8);
  const isLong = crew.length > 32 || groups.length > 8;
  return (
    <div className="crew-directory">
      <dl className="crew-groups">
        {visibleGroups.map((group) => {
          const members = expanded ? group.members : group.members.slice(0, 4);
          return (
            <div key={group.department} className="crew-group">
              <dt>{group.department}</dt>
              <dd>
                {members.map((member) => <span key={`${member.tmdbId ?? member.name}-${member.job}`}><strong>{member.name}</strong><small>{member.job}</small></span>)}
                {!expanded && group.members.length > members.length ? <em>+{group.members.length - members.length} more</em> : null}
              </dd>
            </div>
          );
        })}
      </dl>
      {isLong ? <button type="button" className="detail-expand" aria-expanded={expanded} onClick={() => setExpanded((value) => !value)}>{expanded ? "Show less crew" : `Show all ${crew.length} crew credits`}</button> : null}
    </div>
  );
}

function Production({ companies }: { companies: ProductionCompany[] }) {
  if (!companies.length) return <p className="muted">Production companies are not listed yet.</p>;
  return (
    <ul className="company-list">
      {companies.map((company) => (
        <li key={company.tmdbId ?? company.name}>
          {company.logo ? <img src={company.logo} alt="" /> : <span className="company-mark" aria-hidden="true">{initials(company.name)}</span>}
          <span><strong>{company.name}</strong>{company.originCountry ? <small>{company.originCountry}</small> : null}</span>
        </li>
      ))}
    </ul>
  );
}

function LastRating({ viewing }: { viewing: ViewingHistoryItem }) {
  return (
    <aside className="detail-last-rating" aria-label="Your latest rating">
      <h2>Last rating</h2>
      <div className="detail-last-rating-value"><strong>{formatWatchDate(viewing.occurredAt ?? viewing.publishedAt)}</strong><RatingDisplay value={viewing.rating} compact /></div>
      <div className="detail-last-rating-source">{viewing.rewatch ? <span className="rewatch-badge">Rewatch</span> : null}<span className="muted">{sourceLabel(viewing.source)}</span></div>
    </aside>
  );
}

function ConnectionSection({ connections }: { connections: FilmConnection[] }) {
  const [expanded, setExpanded] = useState(false);
  if (!connections.length) return null;
  const visible = expanded ? connections : connections.slice(0, 6);
  return (
    <section className="detail-block detail-connections">
      <div className="detail-heading"><h2>Your connection to this film</h2><span className="muted">From your rated history</span></div>
      <ul className="connection-list">
        {visible.map((connection) => (
          <li key={connection.entityId} className={`connection-row is-${connection.tone}`}>
            <details>
              <summary>
                <span><strong>{connection.name}</strong><small>{connection.roles.join(" · ")}</small></span>
                <span className="connection-summary"><b>{connection.tone === "unknown" ? "Limited evidence" : connection.tone}</b><em>{connection.sharedCount} shared · {connection.averageRating.toFixed(1)}</em></span>
              </summary>
              <div className="connection-evidence">
                <p>{connection.confidence} · your average is based on {connection.sharedCount} shared film{connection.sharedCount === 1 ? "" : "s"}.</p>
                <ul>{connection.evidence.map((film) => <li key={film.id}><span>{film.title}</span><RatingDisplay value={film.rating} compact /></li>)}</ul>
              </div>
            </details>
          </li>
        ))}
      </ul>
      {connections.length > visible.length ? <button type="button" className="detail-expand" aria-expanded={expanded} onClick={() => setExpanded((value) => !value)}>Show all {connections.length} connections</button> : null}
    </section>
  );
}

function RelatedShelf({ title, source, films, onSelect }: { title: string; source: string; films: LibraryItem[]; onSelect?: (id: string) => void }) {
  if (!films.length) return null;
  return <section className="detail-block detail-related"><Shelf title={title}>{films.map((film) => <FilmCard key={film.id} film={film} onSelect={onSelect} />)}</Shelf><p className="section-source">{source}</p></section>;
}

export function FilmDetailView({ filmId, onBack, backLabel, onStatus, onSelectFilm }: {
  filmId: string;
  onBack: () => void;
  backLabel: string;
  onStatus: (s: string) => void;
  onSelectFilm?: (id: string) => void;
}) {
  const [film, setFilm] = useState<FilmDetail | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setFilm(null);
    setError(null);
    void getFilm(filmId).then((next) => !cancelled && setFilm(next)).catch((err) => {
      log("error", "film load failed", err);
      if (!cancelled) {
        setError(err instanceof Error ? err.message : String(err));
        onStatus("Could not load film");
      }
    });
    return () => { cancelled = true; };
  }, [filmId, onStatus]);

  if (error) return <div className="pad page-pad"><button type="button" className="detail-back" onClick={onBack}>{backLabel}</button><p className="muted">Could not load this film. {error}</p></div>;
  if (!film) return <p className="muted pad">Loading…</p>;

  const image = isHighQualityBanner(film.backdrop) ? film.backdrop : null;
  const directors = film.directors?.join(" · ");
  const runtime = runtimeLabel(film.runtime);
  const community = communityRatingOutOfFive(film.tmdbVoteAverage);
  const latestViewing = film.yourHistory[0] ?? null;

  return (
    <article className="film-detail">
      <header className={`hero detail-hero${image ? "" : " is-empty-hero"}`}>
        {image ? <img className="hero-image" src={image} alt="" /> : <div className="hero-image is-empty" />}
        <div className="hero-scrim" />
        <div className="hero-copy">
          <button type="button" className="detail-back" onClick={onBack}>{backLabel}</button>
          <div className="detail-identity">
            {film.poster ? <img className="detail-poster" src={film.poster} alt="" /> : null}
            <div className="hero-copy-text">
              {directors ? <p className="hero-cast">{directors}</p> : null}
              <h1>{film.title}</h1>
              <p className="hero-meta"><span>{film.year ?? "Year unknown"}</span>{runtime ? <span>{runtime}</span> : null}{film.genres[0] ? <span>{film.genres[0]}</span> : null}</p>
              <div className="detail-ratings" aria-label="TMDB rating">
                <div className="rating-unit"><span className="rating-label">TMDB</span>{community != null ? <RatingDisplay value={community} compact /> : <span className="rating-unavailable">Not rated</span>}{film.tmdbVoteCount ? <small>{film.tmdbVoteCount.toLocaleString()} votes</small> : null}</div>
              </div>
              {film.tagline ? <p className="hero-lede">{film.tagline}</p> : null}
            </div>
          </div>
        </div>
      </header>

      <div className="detail-body">
        <div className="detail-layout">
          <div className="detail-overview-row">
            <section className="detail-block detail-about"><h2>About</h2>{film.overview ? <p className="detail-overview">{film.overview}</p> : <p className="muted">Not enriched yet.</p>}{film.genres.length ? <p className="genre-row">{film.genres.join(" · ")}</p> : null}</section>
            {latestViewing ? <LastRating viewing={latestViewing} /> : null}
          </div>
          {film.friends.length ? <section className="detail-block detail-friends"><h2>Friends</h2><ul className="friend-chips">{film.friends.map((friend, index) => <li key={`${friend.username}-${index}`}><strong>@{friend.username}</strong><RatingDisplay value={friend.rating} compact />{friend.review ? <span className="muted">{friend.review}</span> : null}</li>)}</ul></section> : null}
          <section className="detail-block detail-cast"><div className="detail-heading"><h2>Cast</h2><span className="muted">{film.cast.length} credited</span></div><CastDirectory cast={film.cast} /></section>
          <section className="detail-block detail-crew"><div className="detail-heading"><h2>Crew</h2><span className="muted">{film.crew.length} credits</span></div><CrewDirectory crew={film.crew} /></section>
          <section className="detail-block detail-production"><h2>Production</h2><Production companies={film.companies} /></section>
          <ConnectionSection connections={film.connections} />
        </div>
        <RelatedShelf title={film.collectionName || "Related films"} source="TMDB collection / sequels" films={film.collection ?? []} onSelect={onSelectFilm} />
        <RelatedShelf title="Similar films" source="TMDB recommendations" films={film.similar} onSelect={onSelectFilm} />
      </div>
    </article>
  );
}
