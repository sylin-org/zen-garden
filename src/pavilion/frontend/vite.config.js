import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
// Tauri expects a fixed port; if the port is occupied, Tauri fails.
// Tauri injects TAURI_DEV_HOST in dev mode for hot-module reload over the LAN.
var host = process.env.TAURI_DEV_HOST;
export default defineConfig({
    plugins: [react()],
    clearScreen: false,
    server: {
        port: 5173,
        strictPort: true,
        host: host || false,
        hmr: host
            ? {
                protocol: "ws",
                host: host,
                port: 5174,
            }
            : undefined,
        watch: {
            ignored: ["**/src-tauri/**"],
        },
    },
    build: {
        target: "es2022",
        sourcemap: true,
    },
});
