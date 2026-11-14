// The shared laptop is slow under load: give async test utilities room so flows fail on behaviour, not latency.
import { vi } from "vitest";
import { configure } from "@testing-library/react";

configure({ asyncUtilTimeout: 5000 });

// The Terminal workspace needs a real canvas (xterm.js) and a PTY backend, so app-level tests
// run without it; `terminal.test.tsx` covers it directly against a mocked IPC.
vi.mock("./workspaces/terminal.workspace", () => ({}));
// Same for the 3D viewer (needs WebGL): app-level tests register their own stand-in where needed;
// `viewer3d/scene.test.ts` covers the scene builder directly.
vi.mock("./workspaces/viewer3d.workspace", () => ({}));
