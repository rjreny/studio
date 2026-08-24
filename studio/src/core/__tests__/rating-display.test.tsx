import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { RatingDisplay } from "../../features/films/RatingDisplay";

describe("RatingDisplay", () => {
  it("shows numeric score in compact mode", () => {
    render(<RatingDisplay value={3.5} compact />);
    expect(screen.getByText("3.5")).toBeInTheDocument();
  });

  it("renders empty stars differently from filled stars", () => {
    const { container } = render(<RatingDisplay value={2} />);
    const filled = container.querySelectorAll(".rating-star.is-filled");
    const empty = container.querySelectorAll(".rating-star:not(.is-filled)");
    expect(filled.length).toBe(2);
    expect(empty.length).toBe(3);
  });

  it("shows unrated state distinctly", () => {
    render(<RatingDisplay value={null} compact />);
    expect(screen.getByText("—")).toBeInTheDocument();
  });

  it("clips a half star instead of dimming a full star", () => {
    const { container } = render(<RatingDisplay value={3.5} />);
    expect(container.querySelectorAll(".rating-star.is-filled").length).toBe(3);
    expect(container.querySelectorAll(".rating-star.is-half").length).toBe(1);
    expect(container.querySelector(".rating-star.is-half .rating-star-fill")).toBeTruthy();
  });
});
