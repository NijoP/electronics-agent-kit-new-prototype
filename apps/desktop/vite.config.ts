/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Port 1420 matches `build.devUrl` in src-tauri/tauri.conf.json.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, watch: { ignored: ["**/src-tauri/**"] } },
  build: {
    // Low-memory development (docs/dev/LOW_MEMORY_DEVELOPMENT.md): no source maps in the packaged
    // app and no gzip-size pass over every chunk.
    sourcemap: false,
    reportCompressedSize: false,
  },
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.{ts,tsx}"],
    // Async test utilities get extra room on the slow shared laptop (src/test-setup.ts).
    setupFiles: ["./src/test-setup.ts"],
    // Process the app stylesheet so `styles.css?raw` is the real file (vitest stubs CSS to "" by
    // default, which made every rule lookup in layoutRegressions.test.ts fail).
    css: { include: [/\/src\/styles\.css/] },
    testTimeout: 15000,
    // Forked processes, at most 2 at once (override: `vitest run --maxWorkers=N`).
    pool: "forks",
    maxWorkers: 2,
  },
});
