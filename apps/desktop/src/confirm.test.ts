// Regression (F6 real-window run): in the app `window.confirm` is the dialog plugin's async
// function, so `if (!confirm(m)) return` never waited and a destructive change ran before (and
// whatever) the answer. Written, not run locally (no-build directive); CI runs it.
import { afterEach, describe, expect, it, vi } from "vitest";
import { confirmed, nativeConfirm, whenConfirmed } from "./confirm";
import { runAction } from "./actions/registry";
import type { Action } from "./actions/types";

afterEach(() => vi.restoreAllMocks());

describe("whenConfirmed", () => {
  it("runs at once on a synchronous yes and not on a no", () => {
    const then = vi.fn();
    expect(whenConfirmed("m", then, () => true)).toBe(true);
    expect(then).toHaveBeenCalledTimes(1);
    expect(whenConfirmed("m", then, () => false)).toBe(false);
    expect(then).toHaveBeenCalledTimes(1);
  });
  it("waits for an asynchronous answer and runs only on yes", async () => {
    const then = vi.fn();
    let answer!: (v: boolean) => void;
    const r = whenConfirmed("m", then, () => new Promise<boolean>((res) => (answer = res)));
    expect(then).not.toHaveBeenCalled();
    answer(false);
    expect(await r).toBe(false);
    expect(then).not.toHaveBeenCalled();
    expect(await whenConfirmed("m", then, async () => true)).toBe(true);
    expect(then).toHaveBeenCalledTimes(1);
  });
  it("treats a refused or failed dialog as a no", async () => {
    const then = vi.fn();
    expect(await whenConfirmed("m", then, () => Promise.reject(new Error("dialog.message not allowed")))).toBe(false);
    expect(whenConfirmed("m", then, () => { throw new Error("x"); })).toBe(false);
    expect(await confirmed("m", () => Promise.reject(new Error("x")))).toBe(false);
    expect(then).not.toHaveBeenCalled();
  });
});

describe("nativeConfirm", () => {
  it("calls a synchronous window.confirm as it is", () => {
    const spy = vi.spyOn(window, "confirm").mockReturnValue(true);
    expect(nativeConfirm("Delete it?")).toBe(true);
    expect(spy).toHaveBeenCalledWith("Delete it?");
  });
});

describe("runAction with the app's async confirm", () => {
  it("runs a destructive action only after the yes", async () => {
    const run = vi.fn();
    const a = { id: "clear", label: "Clear all routing", destructive: true, enabled: () => true, run } as unknown as Action;
    let answer!: (v: boolean) => void;
    const r = runAction(a, () => new Promise<boolean>((res) => (answer = res)));
    expect(run).not.toHaveBeenCalled();
    answer(true);
    expect(await r).toBe(true);
    expect(run).toHaveBeenCalledTimes(1);
  });
});
