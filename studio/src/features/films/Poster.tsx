import { useEffect, useState } from "react";

function artworkTone(name: string) {
  return [...name].reduce((total, character) => total + character.charCodeAt(0), 0) % 4;
}

export function Poster({
  name,
  poster,
  large = false,
  className = "",
}: {
  name: string;
  poster: string | null | undefined;
  large?: boolean;
  className?: string;
}) {
  const [failed, setFailed] = useState(false);
  useEffect(() => setFailed(false), [poster]);
  const imageAvailable = Boolean(poster) && !failed;
  return (
    <div className={`poster${large ? " is-large" : ""}${className ? ` ${className}` : ""}`} data-art-tone={artworkTone(name)}>
      {imageAvailable ? <img src={poster!} alt="" onError={() => setFailed(true)} /> : (
        <span className="poster-fallback">
          <span className="poster-fallback-mark" aria-hidden="true">ST</span>
          <strong>{name}</strong>
          <small>Artwork unavailable</small>
        </span>
      )}
    </div>
  );
}
