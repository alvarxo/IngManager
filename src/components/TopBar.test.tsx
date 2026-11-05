import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { TopBar } from "./TopBar";

describe("TopBar", () => {
  it("renders brand, search and custom window controls", () => {
    const client = new QueryClient();
    render(
      <QueryClientProvider client={client}>
        <TopBar rootPath="/tmp/projects" />
      </QueryClientProvider>,
    );
    expect(screen.getByText("IngManager")).toBeInTheDocument();
    const search = screen.getByTitle("Búsqueda global (Ctrl+K)");
    expect(search).toBeEnabled();
    expect(search.getAttribute("data-tauri-drag-region")).toBe("false");
    expect(screen.getByLabelText("Minimizar")).toBeInTheDocument();
    expect(screen.getByLabelText("Maximizar")).toBeInTheDocument();
    expect(screen.getByLabelText("Cerrar")).toBeInTheDocument();
  });
});
