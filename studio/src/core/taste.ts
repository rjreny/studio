import type { Film, Library } from "./types";

export interface TasteProfile {
  title: string;
  summary: string;
  likes: string[];
  dislikes: string[];
  avgRating: number | null;
  ratedCount: number;
  watchedCount: number;
  histogram: { score: number; count: number }[];
  decades: { label: string; count: number }[];
}

const STARS = [0.5, 1, 1.5, 2, 2.5, 3, 3.5, 4, 4.5, 5];

function rated(films: Film[]): Film[] {
  return films.filter((f) => f.rating != null);
}

function decade(year: number | null): string | null {
  if (!year) return null;
  return `${Math.floor(year / 10) * 10}s`;
}

export function tasteProfile(library: Library): TasteProfile {
  const films = library.films;
  const withRating = rated(films);
  const likes = withRating.filter((f) => (f.rating ?? 0) >= 4).sort((a, b) => (b.rating ?? 0) - (a.rating ?? 0));
  const dislikes = withRating.filter((f) => (f.rating ?? 0) <= 2.5).sort((a, b) => (a.rating ?? 0) - (b.rating ?? 0));
  const avg =
    withRating.length === 0
      ? null
      : Math.round((withRating.reduce((s, f) => s + (f.rating ?? 0), 0) / withRating.length) * 100) / 100;

  const hist = STARS.map((score) => ({
    score,
    count: withRating.filter((f) => f.rating === score).length,
  }));

  const decadeMap = new Map<string, number>();
  for (const f of films) {
    const d = decade(f.year);
    if (!d) continue;
    decadeMap.set(d, (decadeMap.get(d) ?? 0) + 1);
  }
  const decades = [...decadeMap.entries()]
    .map(([label, count]) => ({ label, count }))
    .sort((a, b) => b.count - a.count);

  const { title, summary } = nameType(withRating, likes, dislikes, avg, decades);

  return {
    title,
    summary,
    likes: likes.slice(0, 6).map((f) => f.name),
    dislikes: dislikes.slice(0, 6).map((f) => f.name),
    avgRating: avg,
    ratedCount: withRating.length,
    watchedCount: films.filter((f) => f.watched).length,
    histogram: hist,
    decades: decades.slice(0, 8),
  };
}

function nameType(
  ratedFilms: Film[],
  likes: Film[],
  dislikes: Film[],
  avg: number | null,
  decades: { label: string; count: number }[],
): { title: string; summary: string } {
  if (ratedFilms.length < 5) {
    return {
      title: "Still forming",
      summary: "Import a Letterboxd export or rate more films and the type will sharpen.",
    };
  }
  const topDecade = decades[0];
  const share = topDecade ? topDecade.count / Math.max(ratedFilms.length, 1) : 0;
  const fives = ratedFilms.filter((f) => f.rating === 5).length;
  const harsh = (avg ?? 3) < 3.1;
  const generous = (avg ?? 3) >= 3.9;
  const contrast = likes.length > 3 && dislikes.length > 3;

  if (share > 0.38 && topDecade) {
    return {
      title: `${topDecade.label} loyalist`,
      summary: `A third or more of your log sits in the ${topDecade.label}. You return to a period until it feels like home.`,
    };
  }
  if (harsh && fives <= 2) {
    return {
      title: "High priest of no",
      summary: "Low average, almost no perfect scores. You treat five stars like a public commitment.",
    };
  }
  if (generous && fives >= 8) {
    return {
      title: "Open-hearted omnivore",
      summary: "You rate like someone who wants films to work. Enthusiasm is the point, not scarcity.",
    };
  }
  if (contrast) {
    return {
      title: "Split-screen critic",
      summary: `You go to five for ${likes[0]?.name ?? "a few titles"} and down to earth for ${dislikes[0]?.name ?? "the rest"}. Taste with edges.`,
    };
  }
  return {
    title: "Measured cinephile",
    summary: `Average ${avg?.toFixed(2)} across ${ratedFilms.length} ratings. You rank carefully and keep the middle of the scale honest.`,
  };
}

export interface Recommendation {
  name: string;
  year: number | null;
  poster: string | null;
  rating?: number | null;
  voteAverage?: number | null;
  why: string;
  source: "friend" | "watchlist" | "tmdb";
}

export function localRecommendations(library: Library): Recommendation[] {
  const seen = new Set(library.films.filter((f) => f.watched || f.rating).map((f) => f.key));
  const loved = library.films.filter((f) => (f.rating ?? 0) >= 4);
  const out: Recommendation[] = [];

  for (const film of library.films.filter((f) => f.watchlist && !f.watched)) {
    const kin = loved.find((l) => l.year && film.year && Math.abs(l.year - film.year) <= 4);
    out.push({
      name: film.name,
      year: film.year,
      poster: film.poster,
      voteAverage: film.voteAverage,
      why: kin
        ? `On your watchlist, and close in time to ${kin.name}, which you rated ${kin.rating}.`
        : "Already on your watchlist. You told yourself this one mattered.",
      source: "watchlist",
    });
  }

  const friendHits = new Map<string, { entry: (typeof library.friends)[0]["entries"][0]; who: string[] }>();
  for (const friend of library.friends) {
    for (const entry of friend.entries) {
      if ((entry.rating ?? 0) < 4) continue;
      const key = `${entry.name.toLowerCase()}|${entry.year ?? ""}`;
      if (seen.has(key)) continue;
      const hit = friendHits.get(key);
      if (hit) hit.who.push(friend.username);
      else friendHits.set(key, { entry, who: [friend.username] });
    }
  }
  for (const { entry, who } of friendHits.values()) {
    out.push({
      name: entry.name,
      year: entry.year,
      poster: entry.poster,
      rating: entry.rating,
      why:
        who.length > 1
          ? `${who.join(", ")} all rated this ${entry.rating}. You have not logged it.`
          : `${who[0]} rated this ${entry.rating}. You have not logged it.`,
      source: "friend",
    });
  }

  return out.slice(0, 24);
}
