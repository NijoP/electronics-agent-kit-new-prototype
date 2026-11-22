// Escape closes a modal wherever the focus is (opened from a menu, the focus is often on the
// body, so a key handler on the dialog element alone never sees the key). The listener runs in
// the capture phase and stops the event, so the same Escape does not also clear the selection or
// cancel an editor tool behind the modal.
import { useEffect, useRef } from "react";

export function useEscape(onClose: () => void): void {
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      close.current();
    };
    window.addEventListener("keydown", h, true);
    return () => window.removeEventListener("keydown", h, true);
  }, []);
}
