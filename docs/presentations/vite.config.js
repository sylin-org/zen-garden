import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  server: {
    open: false, // We handle this in run.bat/run.sh
    port: 5173
  }
})
