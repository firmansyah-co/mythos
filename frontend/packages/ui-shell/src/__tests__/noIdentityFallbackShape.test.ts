/**
 * SYS-REQ-096 shape guard: no `||`/`??` fallback may supply a device identity.
 *
 * The systemic-fabrication guard (`noFabricatedDefaultsSweep.test.ts`) matches fabricated
 * *content* in parameter defaults. It cannot see this class, and it said so: a fallback is a
 * different shape. A `||` on an identity property does not announce an absence — it supplies a
 * value, so the surface reports a device, tag or address that nothing discovered:
 *
 *     const tag = comp.tag || comp.id || 'DEV-01';                        // fabricates a tag
 *     name: selectedContextItem?.name || 'FP_Motor_VFD_01'                // fabricates a device
 *     hardwareAddress: ... || '%QX0.0..%QX0.3'                            // fabricates an address
 *     udtArchetype: comp.udtArchetype || `FB_${compType}_UDT`             // synthesises a type
 *
 * WHERE THE GUARD SITS, and why that matters (the reviewer's point on this row): a guard on the
 * component's own props does not protect a literal in a *parent* at the call site. So this one
 * scans the whole production tree for the fallback shape wherever it appears, not one component.
 *
 * WHY IT IS SHAPE-FIRST: matching the known literals (`DEV-01`, `192.168.1.20`, …) would only
 * catch what has already shipped. This matches the *pattern* — an identity property whose
 * right-hand operand is a concrete string — so an unseen literal still trips it. Proven by
 * injecting one below.
 *
 * SCOPE: production source only. `workbenchPanelsInteractive.test.tsx` uses `'192.168.1.10'` as
 * legitimate test INPUT (a device the test constructs) and must survive; test files are excluded,
 * and a case below asserts that exclusion is real rather than assumed.
 */

import { describe, it, expect } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';

const ROOT = process.cwd();
const SOURCE_ROOTS = ['packages', 'plugins', 'src'];

/** Properties that name a device, its type or its address. */
const IDENTITY_PROPS = [
  'name',
  'tag',
  'label',
  'hardwareAddress',
  'udtArchetype',
  'ipAddress',
  'articleNumber',
  'articleNo',
  'address',
  'deviceName',
  'deviceType',
  'deviceId',
  'subType',
  'stationName',
  'resolvedVersion',
];

function* walk(dir: string): Generator<string> {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === 'node_modules' || entry.name === 'dist') continue;
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === '__tests__' || entry.name === '__mocks__' || entry.name === 'tests') continue;
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
  return files;
}

/**
 * An operand that asserts a concrete identity: a non-empty string literal, or a template literal
 * that interpolates a name into an identifier shape (`FB_${compType}_UDT`).
 */
const IDENTITY_OPERAND = String.raw`(?:\`[^\`]*\$\{[^\`]*\}[^\`]*\`|'[^']+'|"[^"]+")`;

const PROP_ALT = IDENTITY_PROPS.join('|');

/**
 * A machine token, as opposed to prose.
 *
 * This is the discrimination that makes the guard usable. `peer.user?.name || 'Peer'` and
 * `currentProject?.name || '(No Project Loaded)'` are display labels; `comp.tag || 'DEV-01'`
 * reports a tag. Naming the property alone conflates them, and a guard that needs an exemption per
 * legitimate use gets widened into uselessness the first time it is inconvenient.
 *
 * A device identity in this codebase is a token, not a sentence: no whitespace, and either a digit
 * or a separator (`_`, `-`, `%`, `.`, `/`). `FP_Motor_VFD_01`, `FB_Motor_UDT`, `%QX0.0..%QX0.3`,
 * `DEV-01`, `XXQ9.9` and `FB_${compType}_UDT` all qualify. `Peer`, `No Project Loaded` and
 * `MODULE_SLOT` do not read as an instance identifier in the same way, and the two prose ones are
 * genuinely labels.
 */
function isMachineToken(value: string): boolean {
  const raw = value.replace(/^['"`]|['"`]$/g, '');
  if (raw.length === 0 || raw.length > 60) return false;
  // A bare placeholder glyph ('-', '--', '----') is an announced absence, not an identity.
  if (!/[A-Za-z0-9]/.test(raw)) return false;
  // A PHRASE is normally a label, but a phrase that carries a number is naming an instance:
  // 'ET 200SP Rack 0', 'Conveyor Motor 1', 'PLC Rack 01'. Exempting all whitespace let a phrase
  // through, and the value this first missed was `selectedContextItem?.name || 'ET 200SP Rack 0'`
  // — the exact fabricated rack this mission removed from Zone2ProjectTree, and one two tests in
  // this repo already forbid. '(No Project Loaded)' and 'Peer' carry no digit and stay exempt.
  if (/\s/.test(raw)) return /\d/.test(raw);
  return /\d/.test(raw) || /[_%./-]/.test(raw) || /\$\{/.test(raw);
}

/** A quoted string literal, or a template literal. */
const LITERAL = String.raw`(?:'[^']*'|"[^"]*"|\`[^\`]*\`)`;

/**
 * The fallback shape: `prop: x || <literal>` or `const p = x?.prop || <literal>`.
 *
 * Flagged only when the operand is DEVICE-SHAPED, so a chain that falls back to an empty string or
 * an ordinary label is left alone. `||` on an identity property is the defect only when it supplies
 * an identity: it does not announce an absence, it asserts a value.
 */
function fallbackOffenders(source: string): string[] {
  const offenders: string[] = [];
  const patterns = [
    new RegExp(String.raw`\b(?:${PROP_ALT})\s*[:=][^,;{}\n]*(?:\|\||\?\?)\s*(${LITERAL})`, 'g'),
    new RegExp(String.raw`=\s*[^=;\n]*\b(?:${PROP_ALT})\b\s*(?:\|\||\?\?)\s*(${LITERAL})`, 'g'),
  ];
  for (const re of patterns) {
    let m: RegExpExecArray | null;
    while ((m = re.exec(source)) !== null) {
      if (isMachineToken(m[1])) offenders.push(m[0].replace(/\s+/g, ' ').trim().slice(0, 160));
    }
  }
  offenders.push(...hardcodedIdentityOffenders(source));
  return offenders;
}

/**
 * The hardcoded shape: a selection payload that states device identity outright, with no fallback
 * operator at all — `setSelectedContextItem({ name: 'ET 200SP Rack 0', hardwareAddress:
 * '192.168.1.20', ... })`.
 *
 * Flagged on either signal: a device-shaped value, or a payload whose identity fields are mostly
 * literals (a fully specified device nobody discovered). One literal identity field describing a
 * real selection — `subType: 'MODULE_SLOT'` for a slot the operator clicked — is structural, not a
 * fabricated device, and is left alone.
 */
function hardcodedIdentityOffenders(source: string): string[] {
  const offenders: string[] = [];
  const payloadRe = new RegExp(
    String.raw`setSelectedContextItem\s*\([^)]*\)|selectedContextItem\s*=\s*\{[^}]*\}`,
    'gs'
  );
  const fieldRe = new RegExp(String.raw`\b(?:${PROP_ALT})\s*:\s*(${LITERAL})`, 'g');
  // A COMPUTED value is not an assertion. `name: \`Slot ${slotNumber}\`` describes a real
  // selection; only a plain string literal states an identity the code decided by itself.
  const plainLiteral = (value: string) => !value.includes('${');
  let payload: RegExpExecArray | null;
  while ((payload = payloadRe.exec(source)) !== null) {
    const fields: Array<{ text: string; value: string }> = [];
    fieldRe.lastIndex = 0;
    let f: RegExpExecArray | null;
    while ((f = fieldRe.exec(payload[0])) !== null) {
      fields.push({ text: f[0].replace(/\s+/g, ' ').trim(), value: f[1] });
    }
    if (fields.length === 0) continue;
    // A fully specified device nobody discovered: several identity fields, all plain literals.
    const stated = fields.filter((field) => plainLiteral(field.value));
    // One stated identity field describes a real selection (`subType: 'MODULE_SLOT'` for a slot
    // the operator clicked). Several state a whole device nobody discovered.
    if (stated.length < 2) continue;
    for (const field of stated) offenders.push(field.text.slice(0, 160));
  }
  return offenders;
}

describe('SYS-REQ-096: no identity fallback asserts a device that was not discovered', () => {
  const files = productionFiles();

  it('finds production source to guard (the sweep is not silently empty)', () => {
    expect(files.length).toBeGreaterThan(20);
  });

  it('tests are excluded, so a test may legitimately construct a device address', () => {
    // The carve-out this row requires: '192.168.1.10' is legitimate test INPUT. Assert the
    // exclusion is real, so a later scope change cannot silently start scanning tests.
    const testFile = path.join(
      ROOT,
      'packages/ui-shell/src/__tests__/workbenchPanelsInteractive.test.tsx'
    );
    expect(fs.existsSync(testFile), 'the carve-out test file moved').toBe(true);
    expect(fs.readFileSync(testFile, 'utf8')).toContain('192.168.1.10');
    expect(files, 'test files are being scanned').not.toContain(testFile);
  });

  it('calibrates the value-shape rule, including the phrase case it first missed', () => {
    // The rule is a judgement call, so its boundary is asserted rather than described. The first
    // version exempted every value containing whitespace, which let `name || 'ET 200SP Rack 0'`
    // through - the exact fabricated rack this mission removed from Zone2ProjectTree, and a value
    // two other tests in this repo forbid. A phrase carrying a number names an instance.
    const flagged = [
      'FP_Motor_VFD_01',
      'FB_Motor_UDT',
      '%QX0.0..%QX0.3',
      'DEV-01',
      'XXQ9.9',
      'ET 200SP Rack 0',
      'Conveyor Motor 1',
      'PLC Rack 01',
      'FB_${compType}_UDT',
    ];
    const exempt = [
      'Peer',                 // a display label: no digit, no separator
      '(No Project Loaded)',  // a phrase with no number
      'No selection',
      '—',                    // absence markers, not identities
      '-',
      '----',
    ];
    for (const value of flagged) {
      expect(isMachineToken(value), `should be flagged as an identity: ${value}`).toBe(true);
    }
    for (const value of exempt) {
      expect(isMachineToken(value), `should be exempt as a label/absence: ${value}`).toBe(false);
    }
  });

  it('has no identity fallback in production source', () => {
    const offenders: string[] = [];
    for (const file of files) {
      for (const hit of fallbackOffenders(fs.readFileSync(file, 'utf8'))) {
        offenders.push(`${path.relative(ROOT, file)}: ${hit}`);
      }
    }
    expect(offenders, `identity fallback reintroduced:\n${offenders.join('\n')}`).toEqual([]);
  });
});
