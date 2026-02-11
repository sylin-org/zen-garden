import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
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
