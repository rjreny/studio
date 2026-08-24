export function isHighQualityBanner(url: string | null | undefined): boolean {
  if (!url) return false;
  if (
    url.includes("image.tmdb.org/t/p/original") ||
    url.includes("image.tmdb.org/t/p/w1280") ||
    url.includes("image.tmdb.org/t/p/w1920")
  ) {
    return true;
  }
  if (url.includes("ltrbxd.com") && url.includes("/sm/upload/")) {
    const width = url.match(/-0-(\d+)-0-(\d+)/);
    return width ? Number(width[1]) >= 1000 : false;
  }
  return false;
}

export function communityRatingOutOfFive(tmdbAverage: number | null | undefined): number | null {
  if (tmdbAverage == null || Number.isNaN(tmdbAverage)) return null;
  return Math.round((tmdbAverage / 2) * 10) / 10;
}
