import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { LeftSidebar } from "./LeftSidebar";

describe("LeftSidebar", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("starts collapsed with hamburger and hides navigation drawer", () => {
    render(<LeftSidebar />);
    const toggle = screen.getByRole("button", {
      name: /anclar navegación/i,
    });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    const regionId = toggle.getAttribute("aria-controls");
    expect(regionId).toBeTruthy();
    const region = document.getElementById(regionId!);
    expect(region).toHaveAttribute("aria-hidden", "true");
    expect(region).toHaveAttribute("aria-label", "Navegación");
    // Labeled entries exist in the DOM but are hidden until hover/pin.
    expect(screen.getByText("Inicio")).toBeInTheDocument();
    expect(screen.getByText("Bandeja")).toBeInTheDocument();
    expect(screen.getByText("Ajustes")).toBeInTheDocument();
  });

  it("opens on hover and closes after leave delay", () => {
    const { container } = render(<LeftSidebar />);
    const root = container.firstElementChild as HTMLElement;
    const toggle = screen.getByRole("button", {
      name: /anclar navegación/i,
    });
    const region = document.getElementById(
      toggle.getAttribute("aria-controls")!,
    );

    fireEvent.mouseEnter(root);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(region).toHaveAttribute("aria-hidden", "false");

    fireEvent.mouseLeave(root);
    act(() => {
      vi.advanceTimersByTime(199);
    });
    expect(toggle).toHaveAttribute("aria-expanded", "true");

    act(() => {
      vi.advanceTimersByTime(2);
    });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(region).toHaveAttribute("aria-hidden", "true");
  });

  it("pins open on click and persists to localStorage", () => {
    render(<LeftSidebar />);
    const toggle = screen.getByRole("button", {
      name: /anclar navegación/i,
    });
    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(localStorage.getItem("ingmanager_nav_pinned")).toBe("1");
    expect(
      screen.getByRole("button", { name: /desanclar navegación/i }),
    ).toBeInTheDocument();

    fireEvent.click(
      screen.getByRole("button", { name: /desanclar navegación/i }),
    );
    expect(localStorage.getItem("ingmanager_nav_pinned")).toBe("0");
  });

  it("restores pinned state from localStorage", () => {
    localStorage.setItem("ingmanager_nav_pinned", "1");
    render(<LeftSidebar />);
    expect(
      screen.getByRole("button", { name: /desanclar navegación/i }),
    ).toHaveAttribute("aria-expanded", "true");
  });
});
