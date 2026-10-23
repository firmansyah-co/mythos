import { defineConfig } from 'vitest/config';
import path from 'path';

/**
 * Workspace packages resolve to their `src`, not their built `dist`.
 *
 * Tests that resolve a sibling workspace package through `node_modules` get whatever `dist`
 * happens to be on disk, which need not match the working tree. That silently invalidates a
 * red/green result: a test can pass against a stale build of the very code under test. Aliasing
 * to source makes every test independent of build artifacts.
 */
const workspaceSrc = (name: string) => path.resolve(__dirname, 'plugins', name, 'src/index.ts');

export default defineConfig({
  resolve: {
    alias: [
      {
        find: 'monaco-editor/esm/vs/editor/editor.api.js',
        replacement: 'monaco-editor',
      },
      { find: '@mythos/ui-faceplate-lib', replacement: workspaceSrc('ui-faceplate-lib') },
      { find: '@mythos/ui-inspector', replacement: workspaceSrc('ui-inspector') },
    ],
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
