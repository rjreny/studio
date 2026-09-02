import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { Poster } from "./Poster";

describe("Poster", () => {
  afterEach(cleanup);

  it("shows a title-led fallback when artwork is absent or cannot load", () => {
    const { container, rerender } = render(<Poster name="The Missing Picture" poster={null} large />);
    expect(screen.getByText("The Missing Picture")).toBeInTheDocument();
    expect(screen.getByText("Artwork unavailable")).toBeInTheDocument();

    rerender(<Poster name="The Missing Picture" poster="https://example.test/missing.jpg" large />);
    fireEvent.error(container.querySelector("img")!);
    expect(screen.getByText("Artwork unavailable")).toBeInTheDocument();
  });
});
