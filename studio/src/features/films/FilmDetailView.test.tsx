import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { FilmDetail } from "../../platform/types/film";
import { FilmDetailView } from "./FilmDetailView";

const { getFilm } = vi.hoisted(() => ({
  getFilm: vi.fn(),
}));

vi.mock("../../platform/filmLibrary", () => ({ getFilm }));
vi.mock("../../platform/log", () => ({ log: vi.fn() }));

const cast = Array.from({ length: 18 }, (_, index) => ({
  tmdbId: index + 1,
  name: `Cast member ${index + 1}`,
  profile: null,
  character: index ? null : "Lead",
  order: index,
}));

const film: FilmDetail = {
  id: "tmdb:1",
  title: "Example film",
  year: 2024,
  currentRating: 4,
  poster: null,
  backdrop: null,
  overview: "A precise test of the detail presentation.",
  runtime: 110,
  genres: ["Drama"],
  matchState: "confirmed",
  sourceIdentity: "letterboxd_export",
  yourHistory: [],
  friends: [],
  tmdbId: 1,
  tmdbVoteAverage: 8.4,
  tmdbVoteCount: 1200,
  tmdbReviews: [],
  tagline: null,
  directors: ["A Director"],
  cast,
  crew: [],
  companies: [],
  keywords: [],
  connections: [],
  collectionName: null,
  collection: [],
  similar: [],
};

function renderDetail(onBack = vi.fn()) {
  return render(
    <FilmDetailView
      filmId={film.id}
      onBack={onBack}
      backLabel="Back to Home"
      onStatus={vi.fn()}
    />,
  );
}

describe("FilmDetailView", () => {
  afterEach(cleanup);

  beforeEach(() => {
    vi.clearAllMocks();
    getFilm.mockResolvedValue(film);
  });

  it("keeps a truthful page-local return action and only the community rating in the hero", async () => {
    const onBack = vi.fn();
    renderDetail(onBack);

    const back = await screen.findByRole("button", { name: "Back to Home" });
    expect(screen.getByText("TMDB")).toBeInTheDocument();
    expect(screen.queryByText("Your rating")).not.toBeInTheDocument();
    expect(screen.queryByText("Taste fit")).not.toBeInTheDocument();
    fireEvent.click(back);
    expect(onBack).toHaveBeenCalledOnce();
  });

  it("makes every returned cast credit reachable", async () => {
    renderDetail();
    const expand = await screen.findByRole("button", { name: "Show all 18 cast members" });
    expect(screen.getByText("Lead")).toBeInTheDocument();
    expect(screen.queryByText("Cast member 18")).not.toBeInTheDocument();
    fireEvent.click(expand);
    expect(screen.getByText("Cast member 18")).toBeInTheDocument();
  });

  it("keeps only the latest viewing beside the overview", async () => {
    getFilm.mockResolvedValue({
      ...film,
      yourHistory: [
        { id: "latest", occurredAt: "2026-08-31", publishedAt: null, rewatch: false, rating: 3, source: "letterboxd_rss" },
        { id: "older", occurredAt: "2026-08-01", publishedAt: null, rewatch: false, rating: 4, source: "local" },
      ],
    });
    renderDetail();

    expect(await screen.findByRole("heading", { name: "Last rating" })).toBeInTheDocument();
    expect(screen.getByText("Letterboxd RSS")).toBeInTheDocument();
    expect(screen.queryByText("Studio")).not.toBeInTheDocument();
  });

});
