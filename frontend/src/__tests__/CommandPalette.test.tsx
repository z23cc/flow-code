import { describe, it, expect, mock, beforeEach } from "bun:test";
import { render, screen, fireEvent, cleanup } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import CommandPalette from "../components/CommandPalette";

// Mock SWR to return empty tasks
mock.module("swr", () => ({
  default: () => ({ data: { count: 0, tasks: [] } }),
}));

function renderPalette() {
  return render(
    <MemoryRouter>
      <CommandPalette />
    </MemoryRouter>,
  );
}

describe("CommandPalette", () => {
  beforeEach(() => {
    cleanup();
  });

  it("renders when Cmd+K is pressed", () => {
    renderPalette();
    expect(
      screen.queryByPlaceholderText("Type a command..."),
    ).not.toBeInTheDocument();

    fireEvent.keyDown(document, { key: "k", metaKey: true });
    expect(
      screen.getByPlaceholderText("Type a command..."),
    ).toBeInTheDocument();
  });

  it("closes on Escape", () => {
    renderPalette();

    fireEvent.keyDown(document, { key: "k", metaKey: true });
    expect(
      screen.getByPlaceholderText("Type a command..."),
    ).toBeInTheDocument();

    const input = screen.getByPlaceholderText("Type a command...");
    fireEvent.keyDown(input, { key: "Escape" });
    expect(
      screen.queryByPlaceholderText("Type a command..."),
    ).not.toBeInTheDocument();
  });

  it("filters results when typing", () => {
    renderPalette();
    fireEvent.keyDown(document, { key: "k", metaKey: true });

    expect(screen.getByText("go Dashboard")).toBeInTheDocument();
    expect(screen.getByText("go Agents")).toBeInTheDocument();

    fireEvent.change(screen.getByPlaceholderText("Type a command..."), {
      target: { value: "dash" },
    });

    expect(screen.getByText("go Dashboard")).toBeInTheDocument();
    expect(screen.queryByText("go Agents")).not.toBeInTheDocument();
  });
});
