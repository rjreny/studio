import { useEffect, useRef } from "react";
import type { Route } from "../features/library/data";

const COMMANDS: { id: string; label: string; hint: string; run: string }[] = [
  { id: "home", label: "Go to Home", hint: "Ctrl+1", run: "home" },
  { id: "library", label: "Go to Library", hint: "Ctrl+2", run: "library" },
  { id: "projects", label: "Go to Projects", hint: "Ctrl+3", run: "projects" },
  { id: "settings", label: "Open Settings", hint: "Ctrl+,", run: "settings" },
  { id: "palette", label: "Command palette", hint: "Ctrl+K", run: "noop" },
];

export function CommandPalette({
  open,
  query,
  onQuery,
  onClose,
  onNavigate,
  index,
  onIndex,
}: {
  open: boolean;
  query: string;
  onQuery: (q: string) => void;
  onClose: () => void;
  onNavigate: (route: Route) => void;
  index: number;
  onIndex: (n: number) => void;
}) {
  const inputRef = useRef<HTMLInputElement>(null);
  const filtered = COMMANDS.filter((c) =>
    c.label.toLowerCase().includes(query.trim().toLowerCase()),
  );
  const active = Math.min(index, Math.max(0, filtered.length - 1));

  useEffect(() => {
    if (open) inputRef.current?.focus();
  }, [open]);

  if (!open) return null;

  function run(i: number) {
    const cmd = filtered[i];
    if (!cmd || cmd.run === "noop") {
      onClose();
      return;
    }
    onNavigate(cmd.run as Route);
    onClose();
  }

  return (
    <div className="overlay" onMouseDown={onClose}>
      <div
        className="palette"
        role="dialog"
        aria-label="Command palette"
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          if (e.key === "ArrowDown") {
            e.preventDefault();
            onIndex(Math.min(filtered.length - 1, active + 1));
          }
          if (e.key === "ArrowUp") {
            e.preventDefault();
            onIndex(Math.max(0, active - 1));
          }
          if (e.key === "Enter") {
            e.preventDefault();
            run(active);
          }
        }}
      >
        <input
          ref={inputRef}
          value={query}
          onChange={(e) => {
            onQuery(e.target.value);
            onIndex(0);
          }}
          placeholder="Type a command"
        />
        <ul>
          {filtered.map((c, i) => (
            <li key={c.id}>
              <button
                type="button"
                className={i === active ? "is-on" : ""}
                onClick={() => run(i)}
              >
                <span>{c.label}</span>
                <span className="muted">{c.hint}</span>
              </button>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
