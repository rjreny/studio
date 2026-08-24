import { filmKey, type Friend, type FriendEntry } from "./types";

function tag(xml: string, name: string): string | null {
  const re = new RegExp(`<(?:[a-zA-Z0-9]+:)?${name}[^>]*>([\\s\\S]*?)</(?:[a-zA-Z0-9]+:)?${name}>`, "i");
  const m = xml.match(re);
  return m ? decode(m[1].trim()) : null;
}

function decode(value: string): string {
  return value
    .replace(/<!\[CDATA\[([\s\S]*?)\]\]>/g, "$1")
    .replace(/&amp;/g, "&")
    .replace(/&quot;/g, '"')
    .replace(/&apos;/g, "'")
    .replace(/&#39;/g, "'")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/★/g, "");
}

function items(xml: string): string[] {
  return xml.split(/<item>/i).slice(1).map((chunk) => chunk.split(/<\/item>/i)[0] ?? "");
}

function starsFromTitle(title: string): number | null {
  const stars = title.match(/★+/g);
  if (!stars) return null;
  const half = title.includes("½") || title.includes("1/2");
  return stars[0].length + (half ? 0.5 : 0);
}

function parseItem(xml: string): FriendEntry | null {
  const filmTitle = tag(xml, "filmTitle");
  const title = tag(xml, "title") ?? "";
  if (!filmTitle && !title.includes(" - ")) return null;
  const name = filmTitle || title.replace(/\s*[-–]\s*.*$/, "").replace(/,\s*\d{4}$/, "").trim();
  if (!name) return null;
  const yearRaw = tag(xml, "filmYear");
  const year = yearRaw ? Number.parseInt(yearRaw, 10) : Number.parseInt(title.match(/(\d{4})/)?.[1] ?? "", 10);
  const ratingRaw = tag(xml, "memberRating");
  const poster = xml.match(/<img[^>]+src="([^"]+)"/i)?.[1] ?? null;
  return {
    name,
    year: Number.isFinite(year) ? year : null,
    rating: ratingRaw ? Number.parseFloat(ratingRaw) : starsFromTitle(title),
    liked: /<letterboxd:liked>\s*true/i.test(xml),
    watchedDate: tag(xml, "watchedDate"),
    tmdbId: tag(xml, "movieId") ?? tag(xml, "filmId"),
    poster,
    link: tag(xml, "link") ?? "",
  };
}

export function parseLetterboxdRss(xml: string, username: string): Friend {
  const entries = items(xml)
    .map(parseItem)
    .filter((e): e is FriendEntry => Boolean(e));
  return { username, entries, fetchedAt: Date.now() };
}

export function rssUrl(username: string): string {
  const clean = username.trim().replace(/^@/, "").toLowerCase();
  return `https://letterboxd.com/${encodeURIComponent(clean)}/rss/`;
}

export function friendFilmKey(entry: FriendEntry): string {
  return filmKey(entry.name, entry.year);
}
