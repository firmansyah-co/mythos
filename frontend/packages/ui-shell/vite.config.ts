import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import path from 'path';

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: [
      {
        find: '@mythos/ui-logic-editor',
        replacement: path.resolve(__dirname, '../../plugins/ui-logic-editor/src/index.ts'),
      },
      {
        find: 'monaco-editor/esm/vs/editor/editor.api.js',
        replacement: 'monaco-editor',
      },
    ],
  },
  server: {
    port: 5173,
    proxy: {
      '/api': {
        target: 'http://localhost:8080',
        changeOrigin: true,
      },
    },
  },
  build: {
    outDir: 'dist',
    sourcemap: true,
  },
  test: {
    globals: true,
    environment: 'happy-dom',
    server: {
      deps: {
        inline: ['y-monaco'],
      },
    },
  },
});
