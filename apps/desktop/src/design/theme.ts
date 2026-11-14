// Dark is the default; the choice is remembered per machine in localStorage.
export type Theme = "dark" | "light";
const KEY = "eak.theme";

export function loadTheme(): Theme {
  try {
    return localStorage.getItem(KEY) === "light" ? "light" : "dark";
  } catch {
    return "dark";
  }
}

export function applyTheme(t: Theme): void {
  document.documentElement.dataset.theme = t;
  try {
    localStorage.setItem(KEY, t);
  } catch {
    /* storage unavailable: the theme still applies for this session */
  }
}
