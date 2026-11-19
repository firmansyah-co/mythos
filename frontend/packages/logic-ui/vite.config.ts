import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// The logic module's workbench UI, built as one ES module (`entry.js`) and its stylesheet
// (`logic.css`), which the controller serves at /modules/org.mythos.logic/ui/ (ADR-011 §2.3).
// Everything it uses is bundled into it, React included: no code of the workbench is shared at
// run time, so the module keeps working across rebuilds of the workbench.
export default defineConfig({
  plugins: [react()],
  // A library build leaves `process.env.NODE_ENV` as it is; a browser has no `process`.
  define: { 'process.env.NODE_ENV': JSON.stringify('production') },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    sourcemap: true,
    cssCodeSplit: false,
    lib: {
      entry: 'src/entry.tsx',
      formats: ['es'],
      fileName: () => 'entry.js',
      cssFileName: 'logic',
    },
  },
});
