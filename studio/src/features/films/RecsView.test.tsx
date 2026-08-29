import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { TastePick, TasteState } from "../../platform/types/film";
import { RecsView } from "./RecsView";

const { listen, tasteAnalyze, tasteFeedbackSet, tasteGet } = vi.hoisted(() => ({
  listen: vi.fn(),
  tasteAnalyze: vi.fn(),
  tasteFeedbackSet: vi.fn(),
  tasteGet: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({ listen }));
vi.mock("../../platform/filmLibrary", () => ({ tasteAnalyze, tasteFeedbackSet, tasteGet }));

const feature = {
  featureKey: "mood:neo-noir",
  name: "neo-noir",
  family: "mood",
  appearances: 6,
  recommendationMean: 0.8,
  scoringAffinity: 0.8,
  confidence: 0.8,
  portability: 0.8,
  citeable: true,
  cited: true,
};

function pick(title: string, tmdbId: number): TastePick {
  return {
    title,
    year: 2024,
    poster: null,
    why: "A good fit.",
    rhymesWith: [],
    filmId: `tmdb:${tmdbId}`,
    tmdbId,
    source: "tmdb",
    matchScore: 74,
    attribution: {
      exposureId: `exposure-${tmdbId}`,
      runId: "run-1",
      tmdbId,
      title,
      evidenceGrade: "strong",
      citedPositive: [feature],
      citedNegative: [],
      seedFilms: [],
      semanticFit: 0.8,
      diversityAdjustment: 0,
      retrievalSource: "tmdb",
      rankingRationale: [],
      moodSignature: { modes: ["noir", "tense"], thematicKeywords: [] },
      priorCandidateExposures: 0,
      priorFeatureExposures: [],
    },
  };
}

function state(): TasteState {
  return {
    key: { stored: true, valid: true, lastError: null, model: "model", web: false, models: [] },
    snapshot: { ratedCount: 10, lovedCount: 5, hatedCount: 1, avgRating: 4, genres: [], decades: [], directors: [] },
    feedback: [],
    report: {
      title: "Taste",
      summary: "",
      affinities: [],
      aversions: [],
      dimensions: [],
      newPicks: [pick("New recommendation", 101)],
      watchlistPicks: [pick("Watchlist recommendation", 202)],
      picks: [],
      model: "model",
      generatedAt: "2026-08-28T00:00:00Z",
      ratedCount: 10,
    },
  };
}

describe("RecsView feedback", () => {
  afterEach(cleanup);

  beforeEach(() => {
    vi.clearAllMocks();
    listen.mockResolvedValue(() => {});
    tasteGet.mockResolvedValue(state());
    tasteFeedbackSet.mockResolvedValue({});
  });

  it("sends the selected Pass reason and target feature to Taste", async () => {
    render(<RecsView onSelectFilm={vi.fn()} onOpenSettings={vi.fn()} />);

    const card = (await screen.findByText("New recommendation")).closest("article");
    expect(card).not.toBeNull();
    fireEvent.click(within(card!).getByRole("button", { name: "Give recommendation feedback" }));
    fireEvent.click(await screen.findByRole("button", { name: "That connection doesn't fit" }));

    await waitFor(() => {
      expect(tasteFeedbackSet).toHaveBeenCalledWith(101, "rejected", {
        exposureId: "exposure-101",
        reason: "wrong_connection",
        targetFeatureKey: "mood:neo-noir",
      });
    });
  });

  it("labels positive feedback as interest instead of a duplicate save", async () => {
    render(<RecsView onSelectFilm={vi.fn()} onOpenSettings={vi.fn()} />);

    const newCard = (await screen.findByText("New recommendation")).closest("article");
    const watchlistCard = screen.getByText("Watchlist recommendation").closest("article");
    expect(within(newCard!).getByRole("button", { name: "Tell Taste you are interested in this" })).toBeInTheDocument();
    expect(within(watchlistCard!).getByRole("button", { name: "Tell Taste you are interested in this" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Save" })).not.toBeInTheDocument();
  });
});
