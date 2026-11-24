// The logic module's answer of `blocks` for the library the compiler's tests compile the editor's
// library fixtures with (crates/mythos-logic-compiler/tests/support/mod.rs, `MOTOR_LIBRARY`):
// `editor_library_blocks_are_what_the_workbench_tests_place` there asserts that
// `library_blocks` answers exactly these pins, so the boxes placed here are the declared ones.

import type { LibraryBlock, LibraryCheck } from '../library';
import { libraryBoxes } from '../libraryblocks';

export const MOTOR: LibraryBlock = {
  name: 'Motor',
  kind: 'function_block',
  inputs: [
    { name: 'run', type: 'BOOL' },
    { name: 'speed', type: 'INT' },
  ],
  outputs: [
    { name: 'running', type: 'BOOL' },
    { name: 'rpm', type: 'INT' },
  ],
  result: null,
};

export const SCALE: LibraryBlock = {
  name: 'Scale',
  kind: 'function',
  inputs: [
    { name: 'raw', type: 'INT' },
    { name: 'factor', type: 'INT' },
  ],
  outputs: [],
  result: 'INT',
};

/** What `POST /library/check` answers for a library that checks and declares `blocks`. */
export const checksWith = (...blocks: LibraryBlock[]): LibraryCheck => ({ ok: true, diagnostics: [], blocks });

/** Motor and Scale as the editors take them. */
export function motorLibrary() {
  const known = libraryBoxes({ answer: checksWith(MOTOR, SCALE), error: null });
  if (known.boxes === null) throw new Error(known.why);
  return known.boxes;
}
