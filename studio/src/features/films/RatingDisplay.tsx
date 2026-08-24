export function RatingDisplay({
  value,
  compact = false,
}: {
  value: number | null | undefined;
  compact?: boolean;
}) {
  if (value == null) {
    return (
      <span className="rating-display is-empty" aria-label="Unrated">
        {compact ? "—" : "Unrated"}
      </span>
    );
  }

  const full = Math.floor(value);
  const half = value - full >= 0.5;

  return (
    <span className="rating-display" aria-label={`Rated ${value} out of 5`}>
      <span className="rating-stars" aria-hidden>
        {Array.from({ length: 5 }, (_, i) => {
          const filled = i < full;
          const isHalf = i === full && half;
          return (
            <span
              key={i}
              className={`rating-star${filled ? " is-filled" : ""}${isHalf ? " is-half" : ""}`}
            >
              ★
            </span>
          );
        })}
      </span>
      <span className="rating-number">{value.toFixed(1)}</span>
    </span>
  );
}

export function RatingControl({
  value,
  onChange,
}: {
  value: number | null;
  onChange: (rating: number) => void;
}) {
  return (
    <div className="rating-control">
      <RatingDisplay value={value} compact />
      <div className="rating-control-buttons">
        {[0.5, 1, 1.5, 2, 2.5, 3, 3.5, 4, 4.5, 5].map((n) => (
          <button key={n} type="button" onClick={() => onChange(n)} title={`Rate ${n}`}>
            {n}
          </button>
        ))}
      </div>
    </div>
  );
}
