import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { WindowControls } from "./WindowControls";

describe("WindowControls", () => {
  it("renders minimize, maximize and close buttons", () => {
    render(<WindowControls />);
    expect(screen.getByLabelText("Minimizar")).toBeInTheDocument();
    expect(screen.getByLabelText("Maximizar")).toBeInTheDocument();
    expect(screen.getByLabelText("Cerrar")).toBeInTheDocument();
  });
});
