import { useId, useLayoutEffect, useRef, useState, type KeyboardEvent, type PointerEvent, type ReactNode } from "react";

/** Native content scrolling with a glass thumb that floats over the canvas (no gutter). */
export function ScrollArea({ children, scrollKey = "page" }: { children: ReactNode; scrollKey?: string }) {
  const id = useId();
  const viewport = useRef<HTMLDivElement>(null);
  const content = useRef<HTMLDivElement>(null);
  const rail = useRef<HTMLDivElement>(null);
  const thumb = useRef<HTMLDivElement>(null);
  const idleTimer = useRef<number | undefined>(undefined);
  const drag = useRef<{ pointerId: number; offset: number } | null>(null);
  const positions = useRef(new Map<string, number>());
  const [active, setActive] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [geometry, setGeometry] = useState({ height: 0, top: 0, range: 0, value: 0 });

  function measure() {
    const view = viewport.current;
    const track = rail.current;
    if (!view || !track) return;
    const range = Math.max(0, view.scrollHeight - view.clientHeight);
    const height = Math.min(track.clientHeight, Math.max(44, track.clientHeight * view.clientHeight / Math.max(1, view.scrollHeight)));
    const value = Math.max(0, Math.min(range, view.scrollTop));
    const top = range ? value / range * (track.clientHeight - height) : 0;
    setGeometry(previous => previous.height === height && previous.top === top && previous.range === range && previous.value === value
      ? previous : { height, top, range, value });
  }

  function showThumb() {
    setActive(true);
    window.clearTimeout(idleTimer.current);
    idleTimer.current = window.setTimeout(() => setActive(false), 900);
  }

  useLayoutEffect(() => {
    const observer = new ResizeObserver(measure);
    for (const element of [viewport.current, content.current, rail.current]) {
      if (element) observer.observe(element);
    }
    measure();
    return () => {
      observer.disconnect();
      window.clearTimeout(idleTimer.current);
    };
  }, []);

  useLayoutEffect(() => {
    if (viewport.current) viewport.current.scrollTop = positions.current.get(scrollKey) ?? 0;
    measure();
  }, [scrollKey]);

  function moveTo(clientY: number, offset: number) {
    const view = viewport.current;
    const track = rail.current;
    if (!view || !track) return;
    const travel = track.clientHeight - geometry.height;
    if (travel <= 0) return;
    view.scrollTop = Math.max(0, Math.min(1, (clientY - track.getBoundingClientRect().top - offset) / travel)) * geometry.range;
    measure();
    showThumb();
  }

  function startDrag(event: PointerEvent<HTMLDivElement>) {
    if (event.button !== 0 || geometry.range <= 0) return;
    event.preventDefault();
    event.stopPropagation();
    const offset = thumb.current ? event.clientY - thumb.current.getBoundingClientRect().top : geometry.height / 2;
    drag.current = { pointerId: event.pointerId, offset };
    setDragging(true);
    event.currentTarget.setPointerCapture(event.pointerId);
  }

  function endDrag(event: PointerEvent<HTMLDivElement>) {
    if (drag.current?.pointerId !== event.pointerId) return;
    drag.current = null;
    setDragging(false);
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
    showThumb();
  }

  function scrollWithKeyboard(event: KeyboardEvent<HTMLDivElement>) {
    const view = viewport.current;
    if (!view) return;
    const page = view.clientHeight * 0.9;
    const next: Record<string, number> = {
      ArrowDown: view.scrollTop + 40, ArrowUp: view.scrollTop - 40,
      PageDown: view.scrollTop + page, PageUp: view.scrollTop - page,
      Home: 0, End: geometry.range, " ": view.scrollTop + (event.shiftKey ? -page : page),
    };
    if (!(event.key in next)) return;
    event.preventDefault();
    view.scrollTop = next[event.key];
    measure();
    showThumb();
  }

  return (
    <div className="stage-frame">
      <div className="stage" id={id} ref={viewport} onScroll={() => {
        positions.current.set(scrollKey, viewport.current?.scrollTop ?? 0);
        measure();
        showThumb();
      }}>
        <div className="stage-content" ref={content}>{children}</div>
      </div>
      <div
        className={`scroll-rail${active ? " is-scrolling" : ""}${dragging ? " is-dragging" : ""}`}
        data-overflow={geometry.range > 0}
        ref={rail}
        onWheel={event => {
          const view = viewport.current;
          if (!view) return;
          view.scrollTop += event.deltaY * (event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? view.clientHeight : 1);
          showThumb();
        }}
      >
        <div
          className="scroll-thumb"
          ref={thumb}
          role="scrollbar"
          aria-label="Scroll page"
          aria-controls={id}
          aria-orientation="vertical"
          aria-valuemin={0}
          aria-valuemax={Math.round(geometry.range)}
          aria-valuenow={Math.round(geometry.value)}
          aria-valuetext={`${geometry.range ? Math.round(geometry.value / geometry.range * 100) : 0}% scrolled`}
          aria-hidden={geometry.range <= 0}
          tabIndex={geometry.range > 0 ? 0 : -1}
          style={{ height: geometry.height, transform: `translateY(${geometry.top}px)` }}
          onPointerDown={startDrag}
          onPointerMove={event => { if (drag.current?.pointerId === event.pointerId) moveTo(event.clientY, drag.current.offset); }}
          onPointerUp={endDrag}
          onPointerCancel={endDrag}
          onLostPointerCapture={() => { drag.current = null; setDragging(false); }}
          onKeyDown={scrollWithKeyboard}
        />
      </div>
    </div>
  );
}
