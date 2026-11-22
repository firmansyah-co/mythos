// A program compiled with other sources of the project (D3, ADR-014): the project's user types
// (`data/types/<Name>.st`) and its library (`logic/library/<Name>.st`). A diagnostic of one of
// them is about a line of that other file, so it is listed with its source and never drawn on
// the program; the program's own diagnostics (no source, or its file name) are drawn as before.
// When the project's types and data blocks do not check, `project_problems` says why, listed
// apart whether the program compiled or not. Every assertion reads a rendered element.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { isOfThisProgram, type CheckResult, type Diagnostic } from '../bridge';
import { ProgramEditor } from '../ProgramEditor';
import {
  byLabel,
  byTestId,
  endTest,
  entryOf,
  ladderOf,
  project,
  render,
  resetStores,
  runtimesOf,
  server,
  settle,
  texts,
  until,
  useController,
  useSession,
  useShell,
  timeScale,
  type Backend,
} from './harness';

/** Line 3 has nothing of the program's own diagnostics on it: only the library's names line 3. */
const WRONG = 'PROGRAM Tank\nVAR a : INT; END_VAR\n  a := 1;\nEND_PROGRAM';

const LIBRARY: Diagnostic = { line: 3, col: 5, end_line: 3, end_col: 9, message: 'MOTOR_START has no input SPEED', source: 'logic/library/Motors.st' };
const OWN_NAMED: Diagnostic = { line: 2, col: 5, end_line: 2, end_col: 6, message: 'a is not used', source: 'Tank.st' };
const OWN_BARE: Diagnostic = { line: 4, col: 1, end_line: 4, end_col: 12, message: 'the program has no body' };

const PROJECT_PROBLEMS: Diagnostic[] = [
  { line: 2, col: 3, end_line: 2, end_col: 8, message: 'Speed is declared twice', source: 'data/types/Shared.st' },
  { line: 0, col: 0, end_line: 0, end_col: 0, message: 'data/blocks.json cannot be read as the project\'s data blocks: expected a list' },
];

const compiles = (over: Partial<CheckResult> = {}): CheckResult => ({
  ok: true,
  diagnostics: [],
  variables: [],
  code_size: 64,
  memory_size: 8,
  binding_problems: [],
  ...over,
});

const tank = (check: () => CheckResult, source = WRONG): Backend => ({
  programs: [{ entry: entryOf('Tank'), source }],
  check,
  runtimes: runtimesOf(),
});

/** The workbench has `plant` open and the editor of `name` checked its saved text once. */
async function opened(backend: Backend, name = 'Tank') {
  server(backend);
  useShell.setState({ project: project('plant'), view: 'project' });
  useController.setState({ activeProjectId: 'plant' });
  await settle();
  const container = render(<ProgramEditor name={name} />);
  await settle();
  await until('the check is shown', () => byTestId(container, 'check-result'));
  return container;
}

vi.setConfig({ testTimeout: 20_000 * timeScale });

beforeEach(() => {
  resetStores();
});

afterEach(() => {
  endTest();
});

describe('isOfThisProgram', () => {
  it.each([
    [undefined, true],
    [null, true],
    ['', true],
    ['Tank.st', true],
    ['Tank.xml', true],
    ['logic/library/Motors.st', false],
    ['data/types/Shared.st', false],
  ])('source %j is of this program: %s', (source, ours) => {
    expect(isOfThisProgram({ line: 1, col: 1, end_line: 1, end_col: 1, message: 'm', source })).toBe(ours);
  });
});

describe('a diagnostic of another source', () => {
  it('is listed with its source and not drawn on the program\'s text; the program\'s own are, as before', async () => {
    const container = await opened(tank(() => ({ ok: false, diagnostics: [LIBRARY, OWN_NAMED, OWN_BARE] })));
    const editor = byTestId(container, 'source-editor') as HTMLElement;
    expect(editor).not.toBeNull();

    // The program's own two are underlined and marked at their lines; line 3, the library's, is not.
    const marks = [...editor.querySelectorAll('[data-testid="source-error-mark"]')].map((m) => m.textContent);
    expect(marks).toEqual(['a', 'END_PROGRAM']);
    const lines = [...editor.querySelectorAll('[data-testid="source-error-line"]')];
    expect(lines.map((l) => l.textContent)).toEqual(['2', '4']);
    expect(lines.map((l) => l.getAttribute('title'))).toEqual(['a is not used', 'the program has no body']);
    expect(editor.querySelector('[title*="MOTOR_START"]')).toBeNull();

    // The list: the program's own as buttons, as today; the library's once, plain, with its source.
    expect(texts(container, 'diagnostic')).toEqual(['2:5a is not used', '4:1the program has no body']);
    expect([...container.querySelectorAll('[data-testid="diagnostic"]')].every((d) => d.tagName === 'BUTTON')).toBe(true);
    const elsewhere = [...container.querySelectorAll('[data-testid="diagnostic-elsewhere"]')];
    expect(elsewhere).toHaveLength(1);
    expect(elsewhere[0]?.tagName).toBe('LI');
    expect(elsewhere[0]?.querySelector('button')).toBeNull();
    expect(elsewhere[0]?.querySelector('.logic-diagnostic__at')?.textContent).toBe('logic/library/Motors.st 3:5');
    expect(elsewhere[0]?.textContent).toBe('logic/library/Motors.st 3:5MOTOR_START has no input SPEED');
  });
});

describe('problems of the project\'s types and data blocks', () => {
  const listed = (container: HTMLElement) => byLabel(container, 'Problems in the project\'s types and data blocks');

  it('are listed apart, each with its place, when the program compiles', async () => {
    const container = await opened(tank(() => compiles({ project_problems: PROJECT_PROBLEMS }), 'PROGRAM Tank\nEND_PROGRAM\n'));
    expect(byTestId(container, 'check-ok')?.textContent).toBe('COMPILES');
    expect(byTestId(container, 'project-problems-note')?.textContent).toBe(
      'The project\'s types and data blocks do not check, so this program was compiled without them.'
    );
    expect(listed(container)?.tagName).toBe('UL');
    const items = [...container.querySelectorAll('[data-testid="project-problem"]')];
    expect(items.map((i) => i.querySelector('.logic-diagnostic__at')?.textContent)).toEqual(['data/types/Shared.st 2:3', '']);
    expect(items.map((i) => i.textContent)).toEqual([
      'data/types/Shared.st 2:3Speed is declared twice',
      'data/blocks.json cannot be read as the project\'s data blocks: expected a list',
    ]);
  });

  it('are listed when the program does not compile too', async () => {
    const container = await opened(tank(() => ({ ok: false, diagnostics: [OWN_BARE], project_problems: PROJECT_PROBLEMS })));
    expect(texts(container, 'diagnostic')).toEqual(['4:1the program has no body']);
    expect(listed(container)).not.toBeNull();
    expect(texts(container, 'project-problem')).toEqual([
      'data/types/Shared.st 2:3Speed is declared twice',
      'data/blocks.json cannot be read as the project\'s data blocks: expected a list',
    ]);
  });

  it.each([
    ['absent', compiles()],
    ['empty', compiles({ project_problems: [] })],
  ])('are not listed when the key is %s', async (_how, answer) => {
    const container = await opened(tank(() => answer, 'PROGRAM Tank\nEND_PROGRAM\n'));
    expect(byTestId(container, 'check-ok')).not.toBeNull();
    expect(listed(container)).toBeNull();
    expect(byTestId(container, 'project-problems-note')).toBeNull();
    expect(byTestId(container, 'project-problem')).toBeNull();
  });
});

describe('a diagnostic of another source on a Ladder Diagram', () => {
  const contact = (id: number, y: number, variable: string) =>
    `<contact localId="${id}"><position x="40" y="${y}"/><connectionPointIn><connection refLocalId="1"/></connectionPointIn><connectionPointOut/><variable>${variable}</variable></contact>`;
  const MAIN = ladderOf('Main', [contact(10, 20, 'a'), contact(11, 60, 'b')].join('\n'));

  it('marks no element of the diagram', async () => {
    // A viewer: the read-only view.
    useSession.setState({ phase: 'signed-in', user: { username: 'vera', display_name: 'Vera', role: 'viewer', via: 'session' } });
    const container = await opened(
      {
        programs: [{ entry: entryOf('Main', { language: 'ld' }), source: MAIN }],
        check: () => ({
          ok: false,
          diagnostics: [
            // The library's, carrying no element, as the library's ST gives it.
            { ...LIBRARY },
            // One of another source naming an element of the same name and id: still not this
            // program's, so still not drawn on it.
            { ...LIBRARY, message: 'from the library, with an element', element: { pou: 'Main', local_id: 11 } },
            // The program's own, at its element.
            { line: 9, col: 1, end_line: 9, end_col: 2, message: 'a is not declared', element: { pou: 'Main', local_id: 10 } },
          ],
        }),
        runtimes: runtimesOf(),
      },
      'Main'
    );
    await until('the Ladder Diagram is drawn', () => byTestId(container, 'ld-diagram'));
    const marks = [...container.querySelectorAll('[data-testid="ld-diagnostic"]')];
    expect(marks.map((m) => m.getAttribute('data-local-id'))).toEqual(['10']);
    expect(texts(container, 'diagnostic-elsewhere')).toHaveLength(2);
  });
});
