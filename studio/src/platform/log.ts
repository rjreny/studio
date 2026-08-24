type Level = "info" | "warn" | "error";

export function log(level: Level, message: string, extra?: unknown): void {
  const line = `[studio] ${message}`;
  if (level === "error") console.error(line, extra ?? "");
  else if (level === "warn") console.warn(line, extra ?? "");
  else console.info(line, extra ?? "");
}
