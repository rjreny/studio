export function Poster({
  name,
  poster,
  large = false,
}: {
  name: string;
  poster: string | null | undefined;
  large?: boolean;
}) {
  const initials = name
    .split(/\s+/)
    .slice(0, 2)
    .map((w) => w[0])
    .join("")
    .toUpperCase();
  return (
    <div className={`poster${large ? " is-large" : ""}`}>
      {poster ? <img src={poster} alt="" /> : <span>{initials}</span>}
    </div>
  );
}
