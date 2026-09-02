import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import {
  getFilm,
  getLibrary,
  invalidateDataCache,
  letterboxdFilmUrl,
  setRating,
  shouldNotifyEnrichCompletion,
} from "./filmLibrary";

const page = {
  items: [],
  total: 0,
  coverage: {
    uniqueMovies: 0,
    watchlistMovies: 0,
    totalViewings: 0,
    ratingEvents: 0,
    unresolvedMovies: 0,
    source: "none" as const,
    fullHistoryAvailable: false,
    warnings: [],
  },
};

describe("film library cache", () => {
  beforeEach(() => {
    invoke.mockReset();
    invalidateDataCache();
  });

  it("reuses an in-flight and completed library query until invalidated", async () => {
    invoke.mockResolvedValue(page);

    await Promise.all([
      getLibrary({ limit: 10000, sort: "rating" }),
      getLibrary({ limit: 10000, sort: "rating" }),
    ]);
    await getLibrary({ limit: 10000, sort: "rating" });

    expect(invoke).toHaveBeenCalledTimes(1);

    invalidateDataCache();
    await getLibrary({ limit: 10000, sort: "rating" });

    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it("keeps the updated film detail after rating", async () => {
    const updated = { id: "film-1", title: "Updated" };
    invoke.mockResolvedValue(updated);

    await setRating("film-1", 4);
    await expect(getFilm("film-1")).resolves.toBe(updated);

    expect(invoke).toHaveBeenCalledTimes(1);
  });
});

describe("Letterboxd handoff", () => {
  it("uses the stable TMDB redirect and rejects invalid ids", () => {
    expect(letterboxdFilmUrl(27205)).toBe("https://letterboxd.com/tmdb/27205/");
    expect(letterboxdFilmUrl(0)).toBeNull();
    expect(letterboxdFilmUrl(Number.NaN)).toBeNull();
  });
});

describe("enrichment completion notices", () => {
  const unchanged = {
    hasKey: true,
    keyValid: true,
    attempted: 0,
    matched: 0,
    posters: 0,
    remainingUnmatched: 0,
    remainingWithoutPoster: 0,
    errors: 0,
    lastError: null,
    logPath: null,
  };

  it("stays quiet when an enrichment pass changed nothing", () => {
    expect(shouldNotifyEnrichCompletion(unchanged)).toBe(false);
    expect(shouldNotifyEnrichCompletion({ ...unchanged, posters: 1 })).toBe(true);
    expect(shouldNotifyEnrichCompletion({ ...unchanged, matched: 1 })).toBe(true);
    expect(shouldNotifyEnrichCompletion({ ...unchanged, errors: 1 })).toBe(true);
  });
});
