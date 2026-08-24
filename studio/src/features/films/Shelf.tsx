import type { ReactNode } from "react";

export function Shelf({
  title,
  action,
  children,
  empty,
}: {
  title: string;
  action?: ReactNode;
  children: ReactNode;
  empty?: ReactNode;
}) {
  return (
    <section className="shelf">
      <header className="shelf-head">
        <h2>{title}</h2>
        {action}
      </header>
      {empty ? empty : <div className="shelf-track">{children}</div>}
    </section>
  );
}
