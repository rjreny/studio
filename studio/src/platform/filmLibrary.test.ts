import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { getFilm, getLibrary, invalidateDataCache, setRating } from "./filmLibrary";

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
