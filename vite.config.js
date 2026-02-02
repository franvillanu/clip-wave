import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  server: {
    // Don't open a browser; Tauri opens the app in its own window.
    open: false,
  },
})
