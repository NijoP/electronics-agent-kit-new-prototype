import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// The production bundle is static files in dist/, served by the Rust dashboard-host
// (firmware/dashboard-host). `npm run dev` proxies the API to a running dashboard-host.
const host = "http://127.0.0.1:8080";

export default defineConfig({
  plugins: [react()],
  build: { outDir: "dist", emptyOutDir: true, sourcemap: false, assetsInlineLimit: 0 },
  server: { proxy: { "/api": host, "/display.txt": host } },
});
