/**
 * SYS-REQ-096 class-level guard: no workbench surface may default to sample content.
 *
 * The four surfaces fixed in this change (ladder/FBD/SFC, variable table, data types, network
 * canvas) all failed the same way: a `DEFAULT_*` sample was wired in as a *parameter default*, so
 * any caller that passed nothing — and `LogicEditorPlugin` passes nothing — rendered an invented
 * PLC program, invented declarations and a fabricated rack at 192.168.1.10.
 *
 * A guard that lists the values we shipped would only catch yesterday's defect. This one matches
 * the SHAPE that produced it: a content-carrying default in a component's props, or a sample-data
 * constant exported from production source. A literal nobody has seen yet still trips it.
 *
 * The DOM tests in each plugin prove the rendering path is empty; this proves the mechanism that
 * made it non-empty is gone.
 *
 * The sweep this guard encodes (run from `frontend/`, expects zero production hits):
 *
 *   grep -rn "= DEFAULT_\|{ \.\.\.DEFAULT_" --include="*.tsx" --include="*.ts" \
 *     packages plugins src | grep -v node_modules | grep -v /dist/ | grep -v __tests__
 */

import { describe, it, expect } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';

const ROOT = process.cwd();
const SOURCE_ROOTS = ['packages', 'plugins', 'src'];

/**
 * Files another open change already owns, which carry their own equivalent guard. This is a
 * named, visible carry-out, not a hole: the exemption disappears with the PR that closes it.
 *
 * - `plugins/ui-inspector/src/InspectorDrawer.tsx` is fixed by PR #27, which ships
 *   `plugins/ui-inspector/tests/noFabricatedSource.test.ts`. Remove this entry once #27 merges.
 */
/**
 * Sample data that is deliberately retained because it is never a fallback: the opt-in demo
 * project behind the explicit `loadSampleProject()` action. Reaching it takes a user action, so
 * it asserts nothing about a real plant. Listed by name, not by pattern, so new samples cannot
 * inherit the exemption.
 */
const SAMPLE_CONSTANT_EXEMPT = new Set(['SAMPLE_PROJECT_METADATA', 'SAMPLE_PROJECT_BLOCKS', 'SAMPLE_SCREENS']);

const FIXED_ELSEWHERE = new Map<string, string>([
  [
    'plugins/ui-inspector/src/InspectorDrawer.tsx',
    'fixed by PR #27, which ships plugins/ui-inspector/tests/noFabricatedSource.test.ts',
  ],
]);

/**
 * Props whose default is configuration, not authored content: a view mode, a label, a class name,
 * a language vocabulary. These are declared by the code, not by the project, and inventing them
 * asserts nothing about the plant. Everything else must have no default.
 */
const CONFIG_DEFAULT_PROPS = new Set([
  'className',
  'readOnly',
  'viewMode',
  'initialMode',
  'targetBlock',
  'language',
  'theme',
  'title',
  'label',
  'scope',
  'dataType',
  'kind',
  'status',
  'id',
  'name',
  'availableDataTypes',
  'placeholder',
  'type',
  'tab',
  'defaultValue',
]);

function* walk(dir: string): Generator<string> {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === 'node_modules' || entry.name === 'dist') continue;
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === '__tests__' || entry.name === '__mocks__') continue;
      yield* walk(full);
    } else if (/\.tsx?$/.test(entry.name) && !/\.(test|spec)\.tsx?$/.test(entry.name)) {
      yield full;
    }
  }
}

function productionFiles(): string[] {
  const files: string[] = [];
  for (const root of SOURCE_ROOTS) {
    const abs = path.join(ROOT, root);
    if (fs.existsSync(abs)) files.push(...walk(abs));
  }
  return files.filter((f) => !FIXED_ELSEWHERE.has(path.relative(ROOT, f)));
}

/**
 * True when the right-hand side of a default is *sample content* rather than a value derived at
 * render time.
 *
 * The distinction the guard needs:
 *   content     — a non-empty collection literal, or a reference to a module-level collection
 *                 constant, however that constant is spelled (`sampleRungs` as readily as
 *                 `DEFAULT_RUNGS`).
 *   not content — a scalar literal, an empty collection, an expression computed from other props
 *                 (`tripped ? 'CRITICAL' : 'NORMAL'`), or a bare reference to another prop
 *                 (`alarmActive = tripped`).
 *
 * Derived *values* are a different defect class from fabricated *content*, and are covered by the
 * `||`/fallback shape guard rather than here. An earlier version matched only upper-case seeds, so
 * `= sampleRungs` walked straight through.
 */
function isContentDefault(expr: string, source: string): boolean {
  const value = expr.trim();
  if (value === '[]' || value === '{}' || value === 'new Map()' || value === 'new Set()') return false;
  // Scalar literals are knobs, not content.
  if (/^(?:'[^']*'|"[^"]*"|`[^`]*`|-?\d+(?:\.\d+)?|true|false|null|undefined)$/.test(value)) {
    return false;
  }
  if (/[?:]/.test(value) || /\|\||&&/.test(value)) return false;
  // Non-empty collection literal is content, whatever is inside it.
  if (/^\[/.test(value) || /^\{/.test(value)) return true;

  const identifier = /^([A-Za-z_$][A-Za-z0-9_$.]*)$/.exec(value)?.[1];
  if (identifier) {
    const root = identifier.split('.')[0];
    // Does the file define this name as a collection constant? Then the default is a sample.
    const collectionDef = new RegExp(
      `(?:const|let|var)\\s+${root}\\b[^=]*=\\s*(?:\\[|\\{)`
    );
    if (collectionDef.test(source)) return true;
    // Otherwise it is another prop: a derived value, not sample content.
    return false;
  }

  // A call that builds content, when it is named like sample data.
  if (/^[A-Za-z_$][A-Za-z0-9_$]*\(/.test(value)) {
    return /default|sample|fixture|demo|mock/i.test(value);
  }
  return false;
}

/** Extract `prop = expr` pairs from every destructured component parameter list in a file. */
function propDefaults(source: string): Array<{ line: number; prop: string; expr: string }> {
  const found: Array<{ line: number; prop: string; expr: string }> = [];
  // A component signature: `... = ({ ... })` or `function X({ ... })`.
  const signature = /= \(\{|function\s+[A-Za-z0-9_]*\s*(?:<[^>]*>)?\s*\(\{/g;
  let match: RegExpExecArray | null;
  while ((match = signature.exec(source)) !== null) {
    const open = source.indexOf('{', match.index + match[0].length - 1);
    let depth = 0;
    let end = -1;
    for (let i = open; i < source.length; i += 1) {
      if (source[i] === '{') depth += 1;
      else if (source[i] === '}') {
        depth -= 1;
        if (depth === 0) {
          end = i;
          break;
        }
      }
    }
    if (end === -1) continue;
    const body = source.slice(open + 1, end);
    // Only top-level entries of the destructure (depth 0 relative to the braces).
    let entry = '';
    let level = 0;
    const flush = () => {
      const text = entry.trim();
      entry = '';
      if (!text) return;
      const idx = text.indexOf('=');
      if (idx === -1) return;
      const prop = text.slice(0, idx).trim().replace(/^\.\.\./, '');
      const expr = text.slice(idx + 1).trim();
      if (!prop || prop === '') return;
      const line = source.slice(0, open + 1).split('\n').length;
      found.push({ line, prop, expr });
    };
    for (const ch of body) {
      if (ch === '{' || ch === '[' || ch === '(') level += 1;
      if (ch === '}' || ch === ']' || ch === ')') level -= 1;
      if (ch === ',' && level === 0) flush();
      else entry += ch;
    }
    flush();
  }
  return found;
}

describe('SYS-REQ-096: no workbench surface defaults to fabricated content', () => {
  const files = productionFiles();

  it('finds production source to guard (the sweep is not silently empty)', () => {
    expect(files.length).toBeGreaterThan(20);
  });

  it('has no content-carrying parameter default anywhere in production source', () => {
    const offenders: string[] = [];
    for (const file of files) {
      const source = fs.readFileSync(file, 'utf8');
      for (const { prop, expr } of propDefaults(source)) {
        if (CONFIG_DEFAULT_PROPS.has(prop)) continue;
        if (isContentDefault(expr, source)) {
          offenders.push(`${path.relative(ROOT, file)}: ${prop} = ${expr}`);
        }
      }
    }
    expect(offenders, `fabricated content default reintroduced:\n${offenders.join('\n')}`).toEqual([]);
  });

  it('exports no sample-data constant from production source', () => {
    // A `DEFAULT_*` scalar (a quality, a colour) is fine; a `DEFAULT_*` array or object is a
    // sample waiting to be wired into a default again.
    const offenders: string[] = [];
    for (const file of files) {
      const source = fs.readFileSync(file, 'utf8');
      // Any exported name, any casing, on one line or many. A single-line export was previously
      // invisible because the pattern required a newline before the closing bracket.
      const re =
        /export const ([A-Za-z0-9_]+)\s*(?::[^=]+)?=\s*(\[[^\]]*\]|\{(?:[^{}]|\{[^{}]*\})*\})\s*;/g;
      let m: RegExpExecArray | null;
      while ((m = re.exec(source)) !== null) {
        const name = m[1];
        // Empty collections carry no sample content.
        const body = m[2].replace(/\s/g, '');
        if (body === '[]' || body === '{}') continue;
        // Case-insensitive, so `sampleRungs` is caught as readily as `SAMPLE_RUNGS`.
        if (!/(?:^|_)(?:default|sample|fixture|demo|mock)/i.test(name)) continue;
        if (SAMPLE_CONSTANT_EXEMPT.has(name)) continue;
        offenders.push(`${path.relative(ROOT, file)}: ${name}`);
      }
    }
    expect(offenders, `sample data exported from production source:\n${offenders.join('\n')}`).toEqual([]);
  });

  it('imports no fixture module from production source', () => {
    const offenders: string[] = [];
    for (const file of files) {
      const source = fs.readFileSync(file, 'utf8');
      if (/from\s+['"][^'"]*fixtures?\//.test(source)) {
        offenders.push(path.relative(ROOT, file));
      }
    }
    expect(offenders, `fixture module imported by production source:\n${offenders.join('\n')}`).toEqual([]);
  });

  it('leaves every fixed surface consuming the shared contract with no sample vocabulary', () => {
    const surfaces = [
      'plugins/ui-logic-editor/src/LadderCanvas.tsx',
      'plugins/ui-logic-editor/src/fbd/FbdCanvas.tsx',
      'plugins/ui-logic-editor/src/sfc/SfcCanvas.tsx',
      'plugins/ui-logic-editor/src/variables/VariableTableGrid.tsx',
      'plugins/ui-logic-editor/src/variables/DataTypeEditor.tsx',
      'plugins/ui-logic-editor/src/MonacoStEditor.tsx',
      'plugins/ui-topology/src/NetworkCanvas.tsx',
    ];
    const offenders: string[] = [];
    for (const surface of surfaces) {
      const source = fs.readFileSync(path.join(ROOT, surface), 'utf8');
      if (!/from '@mythos\/ui-empty-state'/.test(source) && !surface.endsWith('MonacoStEditor.tsx')) {
        offenders.push(`${surface}: does not use the shared empty-surface contract`);
      }
      // Fixture vocabulary must not be reachable from a component.
      const sample = source.match(/\b(?:DEFAULT|SAMPLE|FIXTURE)_[A-Z0-9_]{2,}\b/);
      if (sample && !/DEFAULT_(ON|OFF)/.test(sample[0])) {
        offenders.push(`${surface}: references ${sample[0]}`);
      }
    }
    expect(offenders, `surface guard failed:\n${offenders.join('\n')}`).toEqual([]);
  });
});
