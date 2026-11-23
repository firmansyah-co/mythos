import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { boundaryViolations, type PackageInfo } from '../boundaries';

const PACKAGES = resolve(__dirname, '../../..');

function sources(dir: string): { path: string; text: string }[] {
  const out: { path: string; text: string }[] = [];
  for (const entry of readdirSync(dir)) {
    if (entry === 'node_modules' || entry === 'dist') continue;
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) out.push(...sources(path));
    else if (/\.[cm]?[jt]sx?$/.test(entry)) out.push({ path: path.replaceAll('\\', '/'), text: readFileSync(path, 'utf8') });
  }
  return out;
}

function workspacePackages(): PackageInfo[] {
  return readdirSync(PACKAGES)
    .map((entry) => join(PACKAGES, entry))
    .filter((dir) => statSync(join(dir, 'package.json'), { throwIfNoEntry: false })?.isFile())
    .map((dir) => ({
      dir: dir.replaceAll('\\', '/'),
      manifest: JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8')),
      files: sources(join(dir, 'src')),
    }));
}

describe('module boundaries in the workbench (ADR-011 §2.6)', () => {
  it('hold for every package of the workspace', () => {
    const packages = workspacePackages();
    const names = packages.map((p) => p.manifest.name);
    // The check means nothing if it found nothing to check.
    expect(names).toEqual(expect.arrayContaining(['@mythos/ui-shell', '@mythos/workbench-sdk', '@mythos/hmi-ui']));
    expect(packages.find((p) => p.manifest.name === '@mythos/hmi-ui')?.files.length).toBeGreaterThan(5);
    expect(boundaryViolations(packages)).toEqual([]);
  });

  it('every module UI is typechecked by the root lint and built by the root build', () => {
    // The root scripts name their packages: one left out is neither typechecked by `pnpm lint`
    // nor built by `pnpm build`, and nothing says so.
    const root = JSON.parse(readFileSync(resolve(PACKAGES, '../package.json'), 'utf8')) as { scripts: Record<string, string> };
    const filtered = (script: string) =>
      [...(root.scripts[script] ?? '').matchAll(/--filter\s+(\S+)/g)].map((match) => match[1]);
    const moduleUis = workspacePackages()
      .filter((p) => p.manifest.mythos?.module)
      .map((p) => p.manifest.name);
    // The check means nothing if it found nothing to check.
    expect(moduleUis).toEqual(expect.arrayContaining(['@mythos/hmi-ui', '@mythos/logic-ui', '@mythos/data-ui']));
    for (const script of ['lint', 'build']) {
      expect({ script, missing: moduleUis.filter((name) => !filtered(script).includes(name)) }).toEqual({ script, missing: [] });
    }
  });

  it('names every kind of break', () => {
    const file = (path: string, text: string) => ({ path, text });
    const found = boundaryViolations([
      {
        dir: '/w/ui-shell',
        manifest: { name: '@mythos/ui-shell', dependencies: { '@mythos/hmi-ui': 'workspace:*' } },
        files: [file('/w/ui-shell/src/App.tsx', "import { activate } from '@mythos/hmi-ui';")],
      },
      {
        dir: '/w/workbench-sdk',
        manifest: { name: '@mythos/workbench-sdk' },
        files: [file('/w/workbench-sdk/src/index.ts', "export * from '../../hmi-ui/src/entry';")],
      },
      {
        dir: '/w/hmi-ui',
        manifest: {
          name: '@mythos/hmi-ui',
          mythos: { module: 'org.mythos.hmi' },
          dependencies: { '@mythos/workbench-sdk': 'workspace:*', '@mythos/ui-shell': 'workspace:*', react: '^19' },
        },
        files: [
          file('/w/hmi-ui/src/entry.tsx', [
            "import { createRoot } from 'react-dom/client';",
            "import type { WorkbenchHost } from '@mythos/workbench-sdk';",
            "import { viewTag } from '@mythos/workbench-sdk/quality';",
            "import { useTelemetry } from '@mythos/ui-shell/src/data/telemetry';",
            "import { api } from '../../ui-shell/src/api/client';",
            "const lazy = () => import('@mythos/notes-ui');",
            "import './hmi.css';",
          ].join('\n')),
          // A test may use the shell's real host.
          file('/w/hmi-ui/src/__tests__/harness.tsx', "import { createHost } from '../../../ui-shell/src/modules/host';"),
        ],
      },
      { dir: '/w/notes-ui', manifest: { name: '@mythos/notes-ui', mythos: { module: 'org.example.notes' } }, files: [] },
    ]);
    expect(found).toEqual([
      '@mythos/ui-shell depends on @mythos/hmi-ui',
      '/w/ui-shell/src/App.tsx imports @mythos/hmi-ui',
      '/w/workbench-sdk/src/index.ts imports ../../hmi-ui/src/entry, outside @mythos/workbench-sdk',
      '@mythos/hmi-ui depends on @mythos/ui-shell',
      '/w/hmi-ui/src/entry.tsx imports @mythos/ui-shell/src/data/telemetry',
      '/w/hmi-ui/src/entry.tsx imports ../../ui-shell/src/api/client, outside @mythos/hmi-ui',
      '/w/hmi-ui/src/entry.tsx imports @mythos/notes-ui',
    ]);
  });
});
