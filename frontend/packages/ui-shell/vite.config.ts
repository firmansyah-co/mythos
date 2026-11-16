import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  server: {
    // Bind the IPv4 loopback explicitly. Left to the default, Vite listens on whatever
    // `localhost` resolves to first, which on a host that lists `::1` first is IPv6 only, and
    // anything that then probes 127.0.0.1 finds nothing there.
    host: '127.0.0.1',
    port: 5173,
    proxy: {
      // The API server. Server-sent events pass through unchanged.
      '/api': {
        target: process.env.MYTHOS_API ?? 'http://localhost:8080',
        changeOrigin: true,
      },
      // The workbench UIs of modules (ADR-011 §2.3), served by the same server.
      '/modules': {
        target: process.env.MYTHOS_API ?? 'http://localhost:8080',
        changeOrigin: true,
      },
    },
  },
  build: {
    outDir: 'dist',
    sourcemap: true,
  },
});
