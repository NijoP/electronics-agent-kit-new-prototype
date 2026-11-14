// The shared laptop is slow under load: give async test utilities room so flows fail on behaviour, not latency.
import { vi } from "vitest";
import { configure } from "@testing-library/react";

configure({ asyncUtilTimeout: 5000 });

// The Terminal workspace needs a real canvas (xterm.js) and a PTY backend, so app-level tests
// run without it; `terminal.test.tsx` covers it directly against a mocked IPC.
vi.mock("./workspaces/terminal.workspace", () => ({}));
