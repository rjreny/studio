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
  const seen = film.viewingCount && film.viewingCount > 1 ? `Seen ${film.viewingCount}×` : "";

  const body = (
    <>
      <div className="film-card-poster">
        <Poster name={film.title} poster={film.poster} large />
      </div>
      <strong className="film-card-title" title={film.title}>
        {film.title}
      </strong>
      <span className="title-year">{caption || (film.year ?? "")}</span>
      <RatingDisplay value={film.currentRating} starsOnly />
      <span className={`film-card-seen${seen ? "" : " is-empty"}`}>{seen || "\u00a0"}</span>
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
