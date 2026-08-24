import type { ReactNode } from "react";
import type { Route } from "../../features/library/data";
import "./shell.css";

const ITEMS: { id: Route; label: string; icon: ReactNode }[] = [
  {
    id: "home",
    label: "Home",
    icon: (
      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4">
        <path d="M2.5 7.5 8 2.5l5.5 5V14H9.5V10H6.5v4H2.5z" />
      </svg>
    ),
  },
  {
    id: "library",
    label: "Library",
    icon: (
      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4">
        <rect x="2.5" y="2.5" width="11" height="11" rx="1" />
        <path d="M5 6h6M5 8.5h6M5 11h4" />
      </svg>
    ),
  },
  {
    id: "projects",
    label: "Projects",
    icon: (
      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4">
        <path d="M2.5 4.5h4l1 1.5h6v7h-11z" />
      </svg>
    ),
  },
  {
    id: "settings",
    label: "Settings",
    icon: (
      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4">
        <circle cx="8" cy="8" r="2.2" />
        <path d="M8 2.5v1.6M8 11.9v1.6M2.5 8h1.6M11.9 8h1.6M4.1 4.1l1.1 1.1M10.8 10.8l1.1 1.1M4.1 11.9l1.1-1.1M10.8 5.2l1.1-1.1" />
      </svg>
    ),
  },
];

export function Sidebar({
  route,
  onNavigate,
  collapsed,
  onToggle,
}: {
  route: Route;
  onNavigate: (next: Route) => void;
  collapsed: boolean;
  onToggle: () => void;
}) {
  return (
    <nav className="sidebar" aria-label="Main">
      <div className="nav">
        {ITEMS.map((item) => (
          <button
            key={item.id}
            type="button"
            className={`nav-item${route === item.id ? " is-active" : ""}`}
            onClick={() => onNavigate(item.id)}
            title={item.label}
          >
            {item.icon}
            <span>{item.label}</span>
          </button>
        ))}
      </div>
      <div className="sidebar-foot">
        <button
          type="button"
          className="icon-btn"
          onClick={onToggle}
          title={collapsed ? "Expand sidebar" : "Collapse sidebar"}
        >
          <svg width="14" height="14" viewBox="0 0 14 14" aria-hidden>
            <path
              fill="none"
              stroke="currentColor"
              d={collapsed ? "M5 3l4 4-4 4" : "M9 3L5 7l4 4"}
            />
          </svg>
        </button>
      </div>
    </nav>
  );
}
