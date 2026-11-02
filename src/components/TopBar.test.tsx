import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { TopBar } from "./TopBar";

describe("TopBar", () => {
  it("renders brand and disabled search stub", () => {
    render(<TopBar rootPath="/tmp/projects" />);
    expect(screen.getByText("IngManager")).toBeInTheDocument();
    expect(screen.getByTitle("Búsqueda global (Fase 5)")).toBeDisabled();
  });
});
