// The library's pure rules (D4c): which diagnostics are of which source, line ends, and a refusal
// in the module's own words.

import { describe, expect, it } from 'vitest';
import type { Diagnostic } from '../bridge';
import { isOfThisSource, librarySourceOf, refusalOf, sameText, WITHHELD } from '../library';

const at = (source?: string | null): Diagnostic => ({ line: 3, col: 5, end_line: 3, end_col: 9, message: 'm', source });

describe('the library rules', () => {
  it("isOfThisSource takes only this source's file", () => {
    expect(isOfThisSource(at('logic/library/Motors.st'), 'Motors')).toBe(true);
    expect(isOfThisSource(at('logic/library/Valves.st'), 'Motors')).toBe(false);
    expect(isOfThisSource(at('data/types/Motors.st'), 'Motors')).toBe(false);
    expect(isOfThisSource(at(undefined), 'Motors')).toBe(false);
    expect(isOfThisSource(at(null), 'Motors')).toBe(false);
    expect(isOfThisSource(at('Motors.st'), 'Motors')).toBe(false);
    // The module names the file in the spelling it stores; another case is another file's name.
    expect(isOfThisSource(at('logic/library/MOTORS.st'), 'Motors')).toBe(false);
    expect(isOfThisSource(at('logic/library/motors.st'), 'Motors')).toBe(false);
  });

  it('librarySourceOf names only a listed source', () => {
    const listed = ['Motors', 'Valves'];
    expect(librarySourceOf(at('logic/library/Valves.st'), listed)).toBe('Valves');
    expect(librarySourceOf(at('logic/library/Pumps.st'), listed)).toBeNull();
    expect(librarySourceOf(at('logic/library/VALVES.st'), listed)).toBeNull();
    expect(librarySourceOf(at('logic/library/Valves.st'), null)).toBeNull();
    expect(librarySourceOf(at('logic/library/Valves.st'), [])).toBeNull();
    expect(librarySourceOf(at('data/types/Valves.st'), listed)).toBeNull();
    expect(librarySourceOf(at('Tank.st'), listed)).toBeNull();
    expect(librarySourceOf(at(undefined), listed)).toBeNull();
  });

  it('sameText reads CRLF as LF', () => {
    expect(sameText('FUNCTION F : INT\r\nEND_FUNCTION\r\n', 'FUNCTION F : INT\nEND_FUNCTION\n')).toBe(true);
    expect(sameText('a\r\nb', 'a\nb')).toBe(true);
    expect(sameText('a\nb', 'a\nb ')).toBe(false);
    // A lone CR is not a line end the text area changes: it stays a difference.
    expect(sameText('a\rb', 'a\nb')).toBe(false);
  });

  it("refusalOf keeps the module's error verbatim, and says when the body was withheld", () => {
    const words = "'a-b' is not a library source name: a letter, then letters, digits or _, at most 64";
    const thrown = Object.assign(new Error(`400 Bad Request: ${words}`), { status: 400, body: { error: words } });
    expect(refusalOf(thrown)).toEqual({ status: 400, error: words, diagnostics: null, withheld: false });

    const reasons: Diagnostic[] = [at('logic/library/Motors.st')];
    const withReasons = Object.assign(new Error('400 Bad Request: no'), { status: 400, body: { error: 'no', diagnostics: reasons } });
    expect(refusalOf(withReasons).diagnostics).toEqual(reasons);

    // A body that is not the module's (`null`: not JSON) leaves the message, and is not withheld.
    const notJson = Object.assign(new Error('502 Bad Gateway: the answer is not JSON'), { status: 502, body: null });
    expect(refusalOf(notJson)).toEqual({ status: 502, error: '502 Bad Gateway: the answer is not JSON', diagnostics: null, withheld: false });

    // A workbench that threw without the answer: the message, and withheld said.
    const bare = new Error('409 Conflict: something');
    expect(refusalOf(bare)).toEqual({ status: null, error: '409 Conflict: something', diagnostics: null, withheld: true });
    expect(WITHHELD).toContain('whether it gave reasons');
  });
});
