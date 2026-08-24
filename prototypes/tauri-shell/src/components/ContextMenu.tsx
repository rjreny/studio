export function ContextMenu({
  x,
  y,
  ids,
  onClose,
  onAction,
}: {
  x: number;
  y: number;
  ids: number[];
  onClose: () => void;
  onAction: (action: string) => void;
}) {
  return (
    <div className="overlay" onMouseDown={onClose}>
      <ul
        className="menu"
        style={{ top: y, left: x }}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <li>
          <button
            type="button"
            onClick={() => {
              onAction(`Reveal ${ids.length || 1} item(s)`);
              onClose();
            }}
          >
            Reveal in explorer
          </button>
        </li>
        <li>
          <button
            type="button"
            onClick={() => {
              onAction("Stub: duplicate");
              onClose();
            }}
          >
            Duplicate
          </button>
        </li>
      </ul>
    </div>
  );
}
