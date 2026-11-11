import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Tauri expects a fixed dev-server port and must not have the screen cleared (keeps Rust errors visible).
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**', '**/target/**'] },
  },
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  // Binary test fixtures (PVS1 buffers produced by the Rust encoder).
  assetsInclude: ['**/*.bin'],
  build: {
    target: ['es2022', 'chrome120', 'safari16'],
    sourcemap: true,
  },
  worker: { format: 'es' },
  // Pre-bundled at start-up: discovered later (first price list read / Excel export in a
  // worker), it would make Vite reload the whole page in the middle of a session.
  optimizeDeps: { include: ['exceljs'] },
});
