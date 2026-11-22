import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./design/tokens.css";
import "./styles.css";
import { loadProfile } from "./resourceProfile";
import { applyColours } from "./settings/canvasColours";

// Read the resource profile once, early, so the first 3D view and terminal use its tier.
void loadProfile();
// Canvas colour overrides (Settings > Canvas colours) apply before the first paint.
applyColours();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
