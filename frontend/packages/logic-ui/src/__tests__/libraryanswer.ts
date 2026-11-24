// The logic module's answer of `blocks` for the library the compiler's tests compile the editor's
// library fixtures with (crates/mythos-logic-compiler/tests/support/mod.rs, `MOTOR_LIBRARY`).
// `ANSWER` is read by `editor_library_blocks_are_what_the_workbench_tests_place` there, which
// parses it and holds it equal to what `library_blocks` answers for that library, in the shape
// the logic module writes it: a pin of the fixture changed here turns that test red, so the
// boxes placed here are the declared ones.

import type { LibraryBlock, LibraryCheck } from '../library';
import { libraryBoxes } from '../libraryblocks';

/** JSON, as `POST /library/check` answers `blocks` for `MOTOR_LIBRARY`; read by the compiler's test (keep it JSON). */
const ANSWER = `[
  {
    "name": "Motor",
    "kind": "function_block",
    "inputs": [{ "name": "run", "type": "BOOL" }, { "name": "speed", "type": "INT" }],
    "outputs": [{ "name": "running", "type": "BOOL" }, { "name": "rpm", "type": "INT" }],
    "result": null
  },
  {
    "name": "Scale",
    "kind": "function",
    "inputs": [{ "name": "raw", "type": "INT" }, { "name": "factor", "type": "INT" }],
    "outputs": [],
    "result": "INT"
  }
]`;

export const [MOTOR, SCALE] = JSON.parse(ANSWER) as [LibraryBlock, LibraryBlock];

/** What `POST /library/check` answers for a library that checks and declares `blocks`. */
export const checksWith = (...blocks: LibraryBlock[]): LibraryCheck => ({ ok: true, diagnostics: [], blocks });

/** Motor and Scale as the editors take them. */
export function motorLibrary() {
  const known = libraryBoxes({ answer: checksWith(MOTOR, SCALE), error: null });
  if (known.boxes === null) throw new Error(known.why);
  return known.boxes;
}
