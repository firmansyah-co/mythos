// The boundaries between the workbench shell, this SDK and the modules' UIs (ADR-011 §2.6):
//
// - A module's UI uses the workbench only through this SDK: it never imports the shell, another
//   module's UI, or a file outside its own package, and never lists them as dependencies.
// - The shell and this SDK never import a module's UI: the workbench runs with none.
//
// Tests are exempt: a module's tests run it against the shell's real host on purpose.
// `scripts/check.sh` runs this through `boundaries.test.ts`; kept free of Node so the predicate
// can be fed made-up packages.

export interface PackageInfo {
  /** Its directory, absolute, with `/` separators. */
  dir: string;
  /** Its package.json. */
  manifest: {
    name: string;
    mythos?: { module?: string };
    dependencies?: Record<string, string>;
    devDependencies?: Record<string, string>;
    peerDependencies?: Record<string, string>;
  };
  /** Its source files: path (absolute, `/` separators) and text. */
  files: { path: string; text: string }[];
}

export const SHELL = '@mythos/ui-shell';
export const SDK = '@mythos/workbench-sdk';

const isTest = (path: string) => /\/__tests__\/|\.test\.[cm]?[jt]sx?$/.test(path);

/** What a source file imports: static, side-effect and dynamic imports, and re-exports. */
export function importsOf(text: string): string[] {
  const found: string[] = [];
  const pattern = /(?:\bfrom\s*|\bimport\s*\(?\s*)(['"])([^'"]+)\1/g;
  for (const match of text.matchAll(pattern)) found.push(match[2]!);
  return found;
}

/** Where a relative import lands, `/`-separated; `null` for a package import. */
function resolveRelative(from: string, specifier: string): string | null {
  if (!specifier.startsWith('.')) return null;
  const parts = from.split('/').slice(0, -1);
  for (const segment of specifier.split('/')) {
    if (segment === '..') parts.pop();
    else if (segment !== '.') parts.push(segment);
  }
  return parts.join('/');
}

const packageOf = (specifier: string) =>
  specifier.startsWith('@') ? specifier.split('/').slice(0, 2).join('/') : specifier.split('/')[0]!;

/** Every broken boundary, each saying where. */
export function boundaryViolations(packages: PackageInfo[]): string[] {
  const moduleUis = new Set(
    packages.filter((p) => p.manifest.mythos?.module).map((p) => p.manifest.name)
  );
  const workspace = new Set(packages.map((p) => p.manifest.name));
  const found: string[] = [];

  for (const pkg of packages) {
    const name = pkg.manifest.name;
    const isModuleUi = moduleUis.has(name);
    // What this package may never use.
    const forbidden = (other: string) =>
      other !== name &&
      (moduleUis.has(other) || (isModuleUi && workspace.has(other) && other !== SDK));

    const declared = {
      ...pkg.manifest.dependencies,
      ...pkg.manifest.devDependencies,
      ...pkg.manifest.peerDependencies,
    };
    for (const dependency of Object.keys(declared)) {
      if (forbidden(dependency)) found.push(`${name} depends on ${dependency}`);
    }

    for (const file of pkg.files.filter((f) => !isTest(f.path))) {
      for (const specifier of importsOf(file.text)) {
        const landing = resolveRelative(file.path, specifier);
        if (landing !== null) {
          if (landing !== pkg.dir && !landing.startsWith(`${pkg.dir}/`)) {
            found.push(`${file.path} imports ${specifier}, outside ${name}`);
          }
        } else if (forbidden(packageOf(specifier))) {
          found.push(`${file.path} imports ${specifier}`);
        }
      }
    }
  }
  return found;
}
