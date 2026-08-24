import { useEffect, useRef } from "react";
import { newNote, type Note } from "../../core/types";
import { pickAndReadText } from "../../platform/files";
import { log } from "../../platform/log";

export function NotesView({
  notes,
  activeId,
  onChange,
  onActive,
  onStatus,
}: {
  notes: Note[];
  activeId: string | null;
  onChange: (notes: Note[]) => void;
  onActive: (id: string) => void;
  onStatus: (s: string) => void;
}) {
  const active = notes.find((n) => n.id === activeId) ?? notes[0];
  const saveTimer = useRef<number | undefined>(undefined);

  function update(patch: Partial<Note>) {
    if (!active) return;
    const next = notes.map((n) =>
      n.id === active.id ? { ...n, ...patch, updated: Date.now() } : n,
    );
    onChange(next);
    window.clearTimeout(saveTimer.current);
    saveTimer.current = window.setTimeout(() => onStatus("Saved"), 400);
  }

  useEffect(() => () => window.clearTimeout(saveTimer.current), []);

  async function importFile() {
    try {
      const file = await pickAndReadText();
      if (!file) return;
      const note = newNote();
      note.title = file.name.replace(/\.[^.]+$/, "");
      note.body = file.text;
      onChange([note, ...notes]);
      onActive(note.id);
      onStatus(`Imported ${file.name}`);
    } catch (err) {
      log("error", "import failed", err);
      onStatus("Import failed");
    }
  }

  return (
    <div className="notes">
      <aside className="note-list">
        <div className="note-toolbar">
          <button
            type="button"
            className="primary"
            onClick={() => {
              const note = newNote();
              onChange([note, ...notes]);
              onActive(note.id);
            }}
          >
            New
          </button>
          <button type="button" className="primary" onClick={() => void importFile()}>
            Import file
          </button>
        </div>
        {notes.map((n) => (
          <button
            key={n.id}
            type="button"
            className={`note-item${n.id === active?.id ? " is-on" : ""}`}
            onClick={() => onActive(n.id)}
          >
            {n.title || "Untitled"}
            <small>{new Date(n.updated).toLocaleString()}</small>
          </button>
        ))}
      </aside>
      {active ? (
        <div className="note-editor">
          <input value={active.title} onChange={(e) => update({ title: e.target.value })} />
          <textarea
            value={active.body}
            onChange={(e) => update({ body: e.target.value })}
            placeholder="Write…"
          />
        </div>
      ) : (
        <div className="home">
          <p className="muted">Create a note to start.</p>
        </div>
      )}
    </div>
  );
}
