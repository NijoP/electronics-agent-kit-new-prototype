// Yes/no questions before a destructive change.
//
// In the Tauri app `window.confirm` is asynchronous: tauri-plugin-dialog (2.x) replaces it with an
// `async` function that shows a native OK/Cancel dialog and resolves to its answer. Code written as
// `if (!window.confirm(m)) return;` then saw a Promise, which is truthy, and went on with the change
// without waiting for the answer (found by the F6 real-window run: Route › Clear all routing cleared
// the board at once). In a browser and in jsdom tests `confirm` is synchronous. These helpers accept
// both.

// The app's capability must grant `dialog:allow-message`: without it the plugin's dialog call is
// refused, and a refused question counts as a no here.
import { confirm as dialogConfirm } from "@tauri-apps/plugin-dialog";

/** A confirm function: a synchronous answer, or a promise of one (the Tauri dialog plugin). */
export type Confirm = (message: string) => boolean | Promise<boolean>;

/** Title of the native OK/Cancel window (the real-window evidence finds the window by it). */
export const CONFIRM_TITLE = "EAK: please confirm";

/**
 * `window.confirm`, typed as what it really is in the app. When it is the dialog plugin's async
 * replacement, the plugin's own `confirm` is called instead: the same native OK/Cancel window, with
 * a title and a warning icon. A synchronous `window.confirm` (browser, jsdom, a test's mock) is
 * called as it is.
 */
export const nativeConfirm: Confirm = (message) =>
  window.confirm.constructor.name === "AsyncFunction"
    ? dialogConfirm(message, { title: CONFIRM_TITLE, kind: "warning" })
    : (window.confirm(message) as unknown as boolean | Promise<boolean>);

/** The answer to `message`, awaited whichever kind of confirm it is. A failed dialog is a no. */
export async function confirmed(message: string, confirm: Confirm = nativeConfirm): Promise<boolean> {
  try {
    return (await confirm(message)) === true;
  } catch {
    return false;
  }
}

/**
 * Runs `then` only after a yes to `message`: at once for a synchronous answer (so callers and tests
 * that need the change to happen in the same tick still get it), after the dialog for an async one.
 * Returns the answer, or a promise of it.
 */
export function whenConfirmed(message: string, then: () => void, confirm: Confirm = nativeConfirm): boolean | Promise<boolean> {
  let answer: boolean | Promise<boolean>;
  try {
    answer = confirm(message);
  } catch {
    return false;
  }
  if (typeof answer === "boolean") {
    if (answer) then();
    return answer;
  }
  return Promise.resolve(answer).then(
    (yes) => {
      if (yes === true) then();
      return yes === true;
    },
    () => false,
  );
}
