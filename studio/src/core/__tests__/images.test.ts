import { describe, expect, it } from "vitest";
import { communityRatingOutOfFive, isHighQualityBanner } from "../images";

describe("hero banners", () => {
  it("accepts original TMDB backdrops and rejects tiny Letterboxd posters", () => {
    expect(isHighQualityBanner("https://image.tmdb.org/t/p/original/abc.jpg")).toBe(true);
    expect(
      isHighQualityBanner(
        "https://a.ltrbxd.com/resized/film-poster/1/2/3/inception-0-230-0-345-crop.jpg",
      ),
    ).toBe(false);
  });

  it("maps TMDB ten-point scores onto a five-star scale", () => {
    expect(communityRatingOutOfFive(7.4)).toBe(3.7);
    expect(communityRatingOutOfFive(null)).toBeNull();
  });
});
