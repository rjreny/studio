export function Stars({ value, compact = false }: { value: number | null | undefined; compact?: boolean }) {
  if (value == null) return <span className="stars is-empty">{compact ? "" : "Unrated"}</span>;
  const full = Math.floor(value);
  const half = value - full >= 0.5;
  return (
    <span className="stars" title={`${value} / 5`}>
      {Array.from({ length: 5 }, (_, i) => {
        const filled = i < full;
        const isHalf = i === full && half;
        return (
          <span key={i} className={filled ? "is-on" : isHalf ? "is-half" : ""}>
            ★
          </span>
        );
      })}
      {!compact ? <em>{value.toFixed(1)}</em> : null}
    </span>
  );
}
