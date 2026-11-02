import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { TopBar } from "./TopBar";

describe("TopBar", () => {
  it("renders brand and enables global search trigger", () => {
    const client = new QueryClient();
    render(
      <QueryClientProvider client={client}>
        <TopBar rootPath="/tmp/projects" />
      </QueryClientProvider>,
    );
    expect(screen.getByText("IngManager")).toBeInTheDocument();
    const search = screen.getByTitle("Búsqueda global (Ctrl+K)");
    expect(search).toBeEnabled();
  });
});
