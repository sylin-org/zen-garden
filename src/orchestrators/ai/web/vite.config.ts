import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    port: 5173,
    proxy: {
      '/api': 'http://localhost:7190',
      '/health': 'http://localhost:7190',
    },
  },
  build: {
    outDir: 'dist',
    assetsDir: 'assets',
  },
})
