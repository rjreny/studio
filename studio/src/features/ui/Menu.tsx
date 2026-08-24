import { useEffect, useRef, useState } from "react";

export type MenuOption<T extends string> = { id: T; label: string };

export function Menu<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T;
  options: MenuOption<T>[];
  onChange: (id: T) => void;
}) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const current = options.find((o) => o.id === value)?.label ?? value;

  useEffect(() => {
    if (!open) return;
    function onDoc(e: MouseEvent) {
      if (!root.current?.contains(e.target as Node)) setOpen(false);
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") setOpen(false);
    }
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div className={`menu${open ? " is-open" : ""}`} ref={root}>
      <button
        type="button"
        className="menu-trigger"
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        <span className="menu-kicker">{label}</span>
        <span className="menu-value">{current}</span>
      </button>
      {open ? (
        <ul className="menu-list glass" role="listbox">
          {options.map((option) => (
            <li key={option.id} role="none">
              <button
                type="button"
                role="option"
                aria-selected={option.id === value}
                className={option.id === value ? "is-on" : ""}
                onClick={() => {
                  onChange(option.id);
                  setOpen(false);
                }}
              >
                {option.label}
              </button>
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}
