import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  resolve: {
    // The repo lives behind a junction on Windows
    // (F:\Files\... -> F:\Replica\NAS\Files\...). Without this, Rollup
    // realpath-resolves index.html and tries to emit it with a relative
    // path that climbs out of the project root, failing the build.
    preserveSymlinks: true,
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    rollupOptions: {
      external: ["three"],
    },
  },
  server: {
    proxy: {
      "/api": "http://localhost:7186",
      "/health": "http://localhost:7186",
    },
  },
});
