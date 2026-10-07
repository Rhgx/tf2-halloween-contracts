import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [react()],
  // /api is served by src/server (npm run server) during development.
  server: { port: 5174, proxy: { '/api': 'http://127.0.0.1:2715' } }
});
