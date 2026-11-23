// Phase 1 C5: real-time DRC runs once per new revision after a debounce, never while disabled,
// never for a revision that already has a DRC result, and drops a run superseded by an edit.
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ercResult } from "./fixture.test-data";
import { RT_DRC_DEBOUNCE_MS, useRealtimeDrc } from "./realtimeDrc";
import type { CheckResult } from "./types";

const drcAt = (revision: number): CheckResult => ({ ...ercResult(revision), report: { ...ercResult(revision).report, check: "DRC" } });

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

function setup(enabled: boolean, revision: number, drcRevision: number | undefined) {
  const run = vi.fn(() => Promise.resolve(drcAt(revision)));
  const onResult = vi.fn();
  const onError = vi.fn();
  const hook = renderHook((p: { enabled: boolean; revision: number; drcRevision: number | undefined }) => useRealtimeDrc(p.enabled, p.revision, p.drcRevision, run, onResult, onError), {
    initialProps: { enabled, revision, drcRevision },
  });
  return { run, onResult, onError, hook };
}

describe("real-time DRC", () => {
  it("runs once after the debounce for a revision without a DRC result", async () => {
    const { run, onResult } = setup(true, 5, 4);
    act(() => vi.advanceTimersByTime(RT_DRC_DEBOUNCE_MS - 1));
    expect(run).not.toHaveBeenCalled();
    await act(async () => {
      vi.advanceTimersByTime(1);
      await Promise.resolve();
    });
    expect(run).toHaveBeenCalledTimes(1);
    expect(onResult).toHaveBeenCalledWith(drcAt(5));
  });

  it("does nothing while disabled or when the revision already has a DRC result", () => {
    const off = setup(false, 5, 4);
    act(() => vi.advanceTimersByTime(RT_DRC_DEBOUNCE_MS * 2));
    expect(off.run).not.toHaveBeenCalled();
    const current = setup(true, 5, 5);
    act(() => vi.advanceTimersByTime(RT_DRC_DEBOUNCE_MS * 2));
    expect(current.run).not.toHaveBeenCalled();
  });

  it("an edit inside the debounce restarts it: one run, for the newest revision", () => {
    const { run, hook } = setup(true, 5, 4);
    act(() => vi.advanceTimersByTime(RT_DRC_DEBOUNCE_MS / 2));
    hook.rerender({ enabled: true, revision: 6, drcRevision: 4 });
    act(() => vi.advanceTimersByTime(RT_DRC_DEBOUNCE_MS / 2));
    expect(run).not.toHaveBeenCalled();
    act(() => vi.advanceTimersByTime(RT_DRC_DEBOUNCE_MS / 2));
    expect(run).toHaveBeenCalledTimes(1);
  });

  it("reports a refused run with the revision it was for", async () => {
    const run = vi.fn(() => Promise.reject(new Error("the project moved on")));
    const onError = vi.fn();
    renderHook(() => useRealtimeDrc(true, 9, undefined, run, vi.fn(), onError));
    await act(async () => {
      vi.advanceTimersByTime(RT_DRC_DEBOUNCE_MS);
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(onError).toHaveBeenCalledWith("Real-time DRC did not run on revision 9: the project moved on");
  });
});
