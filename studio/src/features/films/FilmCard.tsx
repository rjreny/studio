import type { LibraryItem } from "../../platform/types/film";
import { Poster } from "./Poster";
import { RatingDisplay } from "./RatingDisplay";

export function FilmCard({
  film,
  onSelect,
  caption,
}: {
  film: Pick<LibraryItem, "id" | "title" | "year" | "poster" | "currentRating"> & {
    viewingCount?: number;
    matchState?: string;
  };
  onSelect?: (id: string) => void;
  caption?: string;
}) {
  const body = (
    <>
      <Poster name={film.title} poster={film.poster} large />
      <strong>{film.title}</strong>
      <span className="title-year">{film.year ?? ""}</span>
      {caption ? <span className="muted">{caption}</span> : null}
      <RatingDisplay value={film.currentRating} compact />
      {film.viewingCount && film.viewingCount > 1 ? (
        <span className="rewatch-badge">{film.viewingCount} viewings</span>
      ) : null}
      {film.matchState === "ambiguous" ? <span className="source-badge">{film.matchState}</span> : null}
    </>
  );

  if (onSelect) {
    return (
      <button type="button" className="film-card" onClick={() => onSelect(film.id)}>
        {body}
      </button>
    );
  }

  return <div className="film-card">{body}</div>;
}
