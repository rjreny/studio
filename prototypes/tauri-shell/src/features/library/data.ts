export type Route = "home" | "library" | "projects" | "settings";
export type Theme = "system" | "dark" | "light";
export type Accent = "app" | "system";
export type SortKey = "name" | "modified" | "size";

export interface LibraryRow {
  id: number;
  name: string;
  kind: string;
  modified: number;
  size: number;
}

const KINDS = ["Track", "Clip", "Asset", "Mix", "Session"] as const;

export function createLibrary(count = 6000): LibraryRow[] {
  const rows: LibraryRow[] = new Array(count);
  for (let i = 0; i < count; i += 1) {
    const kind = KINDS[i % KINDS.length];
    rows[i] = {
      id: i + 1,
      name: `${kind} ${String(i + 1).padStart(4, "0")}`,
      kind,
      modified: Date.UTC(2026, 0, 1) + i * 37 * 60 * 1000,
      size: 80_000 + ((i * 7919) % 18_000_000),
    };
  }
  return rows;
}

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
