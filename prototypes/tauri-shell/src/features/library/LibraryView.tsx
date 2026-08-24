import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import {
  createLibrary,
  formatBytes,
  formatDate,
  type LibraryRow,
  type SortKey,
} from "./data";

const ALL = createLibrary(6000);

export function LibraryView({
  visible,
  mode,
  sort,
  onSort,
  search,
  onSearch,
  selected,
  onSelected,
  onStatus,
  onContext,
}: {
  visible: boolean;
  mode: "library" | "projects";
  sort: SortKey;
  onSort: (key: SortKey) => void;
  search: string;
  onSearch: (q: string) => void;
  selected: number[];
  onSelected: (ids: number[]) => void;
  onStatus: (text: string) => void;
  onContext: (x: number, y: number, ids: number[]) => void;
}) {
  const parentRef = useRef<HTMLDivElement>(null);
  const [anchor, setAnchor] = useState<number | null>(null);
  const [dir, setDir] = useState<"asc" | "desc">("asc");

  const rows = useMemo(() => {
    let list: LibraryRow[] = ALL;
    if (mode === "projects") list = list.filter((r) => r.kind === "Session");
    const q = search.trim().toLowerCase();
    if (q) list = list.filter((r) => r.name.toLowerCase().includes(q));
    const copy = [...list];
    copy.sort((a, b) => {
      const av = a[sort];
      const bv = b[sort];
      if (av < bv) return dir === "asc" ? -1 : 1;
      if (av > bv) return dir === "asc" ? 1 : -1;
      return 0;
    });
    return copy;
  }, [mode, search, sort, dir]);

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 32,
    overscan: 16,
  });

  useEffect(() => {
    if (!visible) return;
    const id = requestAnimationFrame(() => virtualizer.measure());
    return () => cancelAnimationFrame(id);
  }, [visible, virtualizer, rows.length]);

  const selectedSet = useMemo(() => new Set(selected), [selected]);

  function selectIndex(index: number, additive: boolean, range: boolean) {
    const row = rows[index];
    if (!row) return;
    if (range && anchor != null) {
      const start = Math.min(anchor, index);
      const end = Math.max(anchor, index);
      onSelected(rows.slice(start, end + 1).map((r) => r.id));
    } else if (additive) {
      const next = new Set(selected);
      if (next.has(row.id)) next.delete(row.id);
      else next.add(row.id);
      onSelected([...next]);
      setAnchor(index);
    } else {
      onSelected([row.id]);
      setAnchor(index);
    }
  }

  function onKeyDown(e: KeyboardEvent) {
    if (!rows.length) return;
    const current = anchor ?? 0;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const delta = e.key === "ArrowDown" ? 1 : -1;
      const next = Math.max(0, Math.min(rows.length - 1, current + delta));
      selectIndex(next, false, e.shiftKey);
      virtualizer.scrollToIndex(next, { align: "auto" });
    }
    if (e.key === "Home") {
      e.preventDefault();
      selectIndex(0, false, e.shiftKey);
      virtualizer.scrollToIndex(0);
    }
    if (e.key === "End") {
      e.preventDefault();
      selectIndex(rows.length - 1, false, e.shiftKey);
      virtualizer.scrollToIndex(rows.length - 1);
    }
    if (e.key === "a" && e.ctrlKey) {
      e.preventDefault();
      onSelected(rows.map((r) => r.id));
    }
  }

  useEffect(() => {
    onStatus(
      selected.length
        ? `${selected.length} selected · ${rows.length} rows`
        : `${rows.length} rows`,
    );
  }, [rows.length, selected.length, onStatus]);

  function toggleSort(key: SortKey) {
    if (sort === key) setDir((d) => (d === "asc" ? "desc" : "asc"));
    else onSort(key);
  }

  return (
    <>
      <div className="toolbar">
        <h1>{mode === "projects" ? "Projects" : "Library"}</h1>
        <span className="muted">
          {mode === "projects" ? "Sessions only" : "6,000 items"}
        </span>
        <div className="toolbar-spacer" />
        <input
          className="search"
          placeholder="Search"
          value={search}
          onChange={(e) => onSearch(e.target.value)}
        />
        <button type="button" className="icon-btn" title="New item" onClick={() => onStatus("Stub: new item")}>
          +
        </button>
      </div>
      <div className="list-head">
        <button type="button" onClick={() => toggleSort("name")}>
          Name {sort === "name" ? (dir === "asc" ? "↑" : "↓") : ""}
        </button>
        <button type="button" onClick={() => toggleSort("modified")}>
          Modified {sort === "modified" ? (dir === "asc" ? "↑" : "↓") : ""}
        </button>
        <button type="button" onClick={() => toggleSort("size")}>
          Size {sort === "size" ? (dir === "asc" ? "↑" : "↓") : ""}
        </button>
      </div>
      <div
        ref={parentRef}
        className="list-body"
        tabIndex={0}
        onKeyDown={onKeyDown}
        onContextMenu={(e) => {
          e.preventDefault();
          onContext(e.clientX, e.clientY, selected.length ? selected : []);
        }}
      >
        <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {virtualizer.getVirtualItems().map((item) => {
            const row = rows[item.index];
            const on = selectedSet.has(row.id);
            return (
              <div
                key={row.id}
                className={`row${on ? " is-selected" : ""}`}
                style={{
                  position: "absolute",
                  top: 0,
                  left: 0,
                  width: "100%",
                  height: item.size,
                  transform: `translateY(${item.start}px)`,
                }}
                onClick={(e) => selectIndex(item.index, e.ctrlKey || e.metaKey, e.shiftKey)}
                onContextMenu={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  const ids = selectedSet.has(row.id) ? selected : [row.id];
                  if (!selectedSet.has(row.id)) onSelected([row.id]);
                  onContext(e.clientX, e.clientY, ids.length ? ids : [row.id]);
                }}
              >
                <span>{row.name}</span>
                <span>{formatDate(row.modified)}</span>
                <span>{formatBytes(row.size)}</span>
              </div>
            );
          })}
        </div>
      </div>
    </>
  );
}
