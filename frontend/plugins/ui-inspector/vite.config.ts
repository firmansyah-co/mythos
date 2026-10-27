import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import path from 'path';

export default defineConfig({
  plugins: [react()],
  build: {
    lib: {
      entry: path.resolve(__dirname, 'src/index.ts'),
      name: 'MythosUiInspector',
      fileName: (format) => `index.${format === 'es' ? 'js' : 'cjs'}`,
      formats: ['es', 'cjs'],
    },
    rollupOptions: {
      // The faceplate library is a workspace sibling, not a bundled dependency: keeping it
      // external avoids shipping a second copy of the shared quality/no-data contract.
      external: ['react', 'react-dom', 'react/jsx-runtime', '@mythos/ui-faceplate-lib'],
      output: {
        globals: {
          react: 'React',
          'react-dom': 'ReactDOM',
          '@mythos/ui-faceplate-lib': 'MythosUiFaceplateLib',
        },
      },
    },
  },
  test: {
    globals: true,
    environment: 'happy-dom',
  },
});
