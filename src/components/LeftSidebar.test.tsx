import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { LeftSidebar } from "./LeftSidebar";

vi.mock("@/lib/api", () => ({
  api: {
    listProjects: vi.fn(async () => []),
    listAuthorizedRoots: vi.fn(async () => []),
    getAppSetting: vi.fn(async () => null),
    setAppSetting: vi.fn(async () => undefined),
    openProject: vi.fn(async () => undefined),
  },
}));

function renderSidebar() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <LeftSidebar />
    </QueryClientProvider>,
  );
}

describe("LeftSidebar", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("starts collapsed with hamburger and hides project panel", () => {
    renderSidebar();
    const toggle = screen.getByRole("button", {
      name: /anclar panel de proyectos/i,
    });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    const regionId = toggle.getAttribute("aria-controls");
    expect(regionId).toBeTruthy();
    const region = document.getElementById(regionId!);
    expect(region).toHaveAttribute("aria-hidden", "true");
    expect(region).toHaveAttribute("aria-label", "Proyectos");
    expect(screen.getByLabelText("Inicio")).toBeInTheDocument();
  });

  it("opens on hover and closes after leave delay", () => {
    const { container } = renderSidebar();
    const root = container.firstElementChild as HTMLElement;
    const toggle = screen.getByRole("button", {
      name: /anclar panel de proyectos/i,
    });
    const region = document.getElementById(toggle.getAttribute("aria-controls")!);

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
    renderSidebar();
    const toggle = screen.getByRole("button", {
      name: /anclar panel de proyectos/i,
    });
    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(localStorage.getItem("ingmanager_sidebar_pinned")).toBe("1");
    expect(
      screen.getByRole("button", { name: /desanclar panel de proyectos/i }),
    ).toBeInTheDocument();

    fireEvent.click(
      screen.getByRole("button", { name: /desanclar panel de proyectos/i }),
    );
    expect(localStorage.getItem("ingmanager_sidebar_pinned")).toBe("0");
  });

  it("restores pinned state from localStorage", () => {
    localStorage.setItem("ingmanager_sidebar_pinned", "1");
    renderSidebar();
    expect(
      screen.getByRole("button", { name: /desanclar panel de proyectos/i }),
    ).toHaveAttribute("aria-expanded", "true");
  });
});
