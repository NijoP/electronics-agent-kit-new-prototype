// AI panel states: no provider (plain notice + settings, offline label), AI steps with
// Accept / Reject / Rollback, and the provider readiness rule. No model is called.
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fixture } from "./fixture.test-data";
import { AiPanel, stepSummary, type ChatMsg } from "./shell/AiPanel";
import { readyProvider } from "./shell/ai";
import type { AiStep, ProviderView } from "./types";

afterEach(cleanup);

const step: AiStep = {
  id: 2,
  proposal_id: 1,
  command: { type: "Annotate" },
  class: "reversible_local",
  checks_to_run: ["ERC"],
  preview: {
    op_id: "op",
    command: "Annotate",
    class: "reversible_local",
    accepted: true,
    revision_before: 7,
    revision_after: 8,
    created: [],
    affected: ["sym-1"],
    warnings: [],
    error: null,
    validation_note: "",
  },
};

function panel(messages: ChatMsg[], modelLabel: string | null) {
  const h = { onSend: vi.fn(), onAccept: vi.fn(), onReject: vi.fn(), onRollback: vi.fn(), onOpenSettings: vi.fn(), onClose: vi.fn() };
  render(<AiPanel messages={messages} snapshot={fixture()} library={[]} contextLabel={null} busy={false} modelLabel={modelLabel} focusNonce={0} {...h} />);
  return h;
}

describe("AI panel", () => {
  it("says plainly when no model is configured and offers the settings", () => {
    const h = panel([{ id: 1, role: "assistant", source: "offline", text: "Annotate schematic." }], null);
    expect(screen.getByTestId("ai-mode").textContent).toContain("No AI model is configured");
    expect(screen.getByText("Offline command (no AI)")).toBeTruthy();
    fireEvent.click(screen.getByText("Open model settings"));
    expect(h.onOpenSettings).toHaveBeenCalled();
  });

  it("shows an agent step preview with Accept and Reject", () => {
    const m: ChatMsg = { id: 1, role: "assistant", source: "ai", text: "", step, status: "pending" };
    const h = panel([m], "Claude · x");
    expect(screen.getByTestId("ai-mode").textContent).toContain("AI: Claude · x");
    expect(screen.queryByText("Offline command (no AI)")).toBeNull();
    expect(screen.getByRole("group", { name: "AI step: Annotate" }).textContent).toContain("checks to re-run after applying: ERC");
    fireEvent.click(screen.getByText("Accept"));
    expect(h.onAccept).toHaveBeenCalledWith(m);
    fireEvent.click(screen.getByText("Reject"));
    expect(h.onReject).toHaveBeenCalledWith(m);
  });

  it("offers Rollback for an accepted step and labels rejected / rolled-back steps", () => {
    const accepted: ChatMsg = { id: 1, role: "assistant", source: "ai", text: "", step, status: "accepted", opId: "op-9" };
    const h = panel(
      [accepted, { id: 2, role: "assistant", source: "ai", text: "", step: { ...step, id: 3 }, status: "rejected" }, { id: 3, role: "assistant", source: "ai", text: "", step: { ...step, id: 4 }, status: "rolled_back" }],
      "Claude · x",
    );
    expect(screen.getByText("Applied through the engine as agent:assistant")).toBeTruthy();
    expect(screen.getByText("Rejected — nothing changed")).toBeTruthy();
    expect(screen.getByText("Rolled back (undone)")).toBeTruthy();
    fireEvent.click(screen.getByText("Rollback"));
    expect(h.onRollback).toHaveBeenCalledWith(accepted);
  });

  it("reports a refused dry run instead of hiding it", () => {
    const bad = { ...step, preview: { ...step.preview, accepted: false, error: { code: "invalid", message: "no such symbol" } } };
    expect(stepSummary(bad).join("\n")).toContain("dry run refused");
  });

  it("treats a provider as ready only with a model and a key when one is needed", () => {
    const base: ProviderView = { id: "p", label: "Claude", kind: "anthropic", base_url: null, model: "m", has_key: false, needs_key: true, key_error: null };
    expect(readyProvider([base])).toBeNull();
    expect(readyProvider([{ ...base, model: null, has_key: true }])).toBeNull();
    expect(readyProvider([{ ...base, has_key: true }])).toEqual({ id: "p", label: "Claude · m" });
    expect(readyProvider([{ ...base, kind: "ollama", needs_key: false }])).toEqual({ id: "p", label: "Claude · m" });
  });
});
