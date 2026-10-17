import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  // Tauri's dev window loads this port (src-tauri/tauri.conf.json devUrl).
  server: { port: 5174, strictPort: true }
});
