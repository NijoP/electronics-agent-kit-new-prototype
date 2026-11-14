import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./design/tokens.css";
import "./styles.css";
import { loadProfile } from "./resourceProfile";

// Read the resource profile once, early, so the first 3D view and terminal use its tier.
void loadProfile();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
