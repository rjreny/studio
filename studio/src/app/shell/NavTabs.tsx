import { useLayoutEffect, useRef, useState } from "react";
import type { Route } from "../../core/types";

export function NavTabs({
  items,
  active,
  onGo,
}: {
  items: { id: Route; label: string }[];
  active: Route;
  onGo: (id: Route) => void;
}) {
  const navRef = useRef<HTMLElement>(null);
  const [liquid, setLiquid] = useState({ x: 4, w: 0 });

  useLayoutEffect(() => {
    const nav = navRef.current;
    if (!nav) return;

    function place() {
      const current = navRef.current;
      if (!current) return;
      const selected = current.querySelector<HTMLElement>(".nav-pill-link.is-active");
      if (!selected) return;
      setLiquid({ x: selected.offsetLeft, w: selected.offsetWidth });
    }

    place();
    const observer = new ResizeObserver(place);
    observer.observe(nav);
    window.addEventListener("resize", place);
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", place);
    };
  }, [active, items]);

  return (
    <nav className="nav-pill glass" aria-label="Primary" ref={navRef}>
      <span className="nav-liquid" style={{ transform: `translateX(${liquid.x}px)`, width: liquid.w }} />
      {items.map((item) => (
        <button
          key={item.id}
          type="button"
          className={`nav-pill-link${active === item.id ? " is-active" : ""}`}
          aria-current={active === item.id ? "page" : undefined}
          onClick={() => onGo(item.id)}
        >
          {item.label}
        </button>
      ))}
    </nav>
  );
}
