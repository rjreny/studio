import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";

export function AnchoredPopover({
  id,
  anchor,
  title,
  openedWithKeyboard,
  onClose,
  children,
}: {
  id: string;
  anchor: HTMLButtonElement;
  title: string;
  openedWithKeyboard: boolean;
  onClose: () => void;
  children: ReactNode;
}) {
  const panel = useRef<HTMLDivElement>(null);
  const closeButton = useRef<HTMLButtonElement>(null);
  const [position, setPosition] = useState({ left: -9999, top: -9999 });

  function close() {
    anchor.focus();
    onClose();
  }

  useLayoutEffect(() => {
    function updatePosition() {
      const anchorRect = anchor.getBoundingClientRect();
      const panelRect = panel.current?.getBoundingClientRect();
      const width = panelRect?.width ?? 360;
      const height = panelRect?.height ?? 280;
      const gap = 10;
      const left = Math.min(Math.max(12, anchorRect.left), window.innerWidth - width - 12);
      const hasSpaceBelow = window.innerHeight - anchorRect.bottom >= Math.min(height + gap, 240);
      const top = hasSpaceBelow
        ? anchorRect.bottom + gap
        : Math.max(12, anchorRect.top - height - gap);
      setPosition({ left, top });
    }
    updatePosition();
    window.addEventListener("resize", updatePosition);
    window.addEventListener("scroll", updatePosition, true);
    return () => {
      window.removeEventListener("resize", updatePosition);
      window.removeEventListener("scroll", updatePosition, true);
    };
  }, [anchor]);

  useEffect(() => {
    function onPointerDown(event: PointerEvent) {
      if (!panel.current?.contains(event.target as Node) && !anchor.contains(event.target as Node)) close();
    }
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") close();
    }
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    if (openedWithKeyboard) closeButton.current?.focus();
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [anchor, openedWithKeyboard]);

  return createPortal(
    <div
      id={id}
      ref={panel}
      className="taste-popover glass"
      role="dialog"
      aria-modal="false"
      aria-label={title}
      style={position}
    >
      <div className="taste-popover-head">
        <strong>{title}</strong>
        <button ref={closeButton} type="button" aria-label={`Close ${title}`} onClick={close}>
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="m7 7 10 10M17 7 7 17" /></svg>
        </button>
      </div>
      {children}
    </div>,
    document.body,
  );
}
