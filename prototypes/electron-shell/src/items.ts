export type Route = "home" | "library" | "projects" | "settings";
export type Theme = "system" | "dark" | "light";
export type Accent = "app" | "system";
export type SortKey = "name" | "modified" | "size";

export interface Item {
  id: number;
  name: string;
  kind: string;
  modified: number;
  size: number;
}

const KINDS = ["Track", "Clip", "Asset", "Mix", "Session"] as const;

export const CATALOG: Item[] = Array.from({ length: 6000 }, (_, i) => ({
  id: i + 1,
  name: `${KINDS[i % 5]} ${String(i + 1).padStart(4, "0")}`,
  kind: KINDS[i % 5],
  modified: Date.UTC(2026, 0, 1) + i * 37 * 60 * 1000,
  size: 80_000 + ((i * 7919) % 18_000_000),
}));

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

export function formatDate(ms: number): string {
  return new Date(ms).toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

export function resolveTheme(theme: Theme): "dark" | "light" {
  if (theme !== "system") return theme;
  return window.matchMedia("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}
