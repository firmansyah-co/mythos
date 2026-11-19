import { describe, expect, it } from 'vitest';
import { offsetOf, rangeOf, tokenize } from '../st';

const SOURCE = `PROGRAM Tank (* a tank
  over two lines *)
VAR level : INT := 16#FF; t : TIME := T#1m30s; s : STRING := 'it$'s'; END_VAR
  // a line comment with IF in it
  IF level < 5.0E-1 THEN speed := 0; END_IF;
END_PROGRAM`;

describe('painting Structured Text', () => {
  it('paints exactly the source: every character in one token, in order', () => {
    for (const text of [SOURCE, '', '(* never closed', "'never closed", 'x := 1..4;', 'a\r\n\tb']) {
      const tokens = tokenize(text);
      expect(tokens.map((t) => t.text).join('')).toBe(text);
      let at = 0;
      for (const token of tokens) {
        expect(token.offset).toBe(at);
        at += token.text.length;
      }
    }
  });

  it('tells keywords, types, comments, strings and numbers apart, whatever their case', () => {
    const of = (kind: string) => tokenize(SOURCE).filter((t) => t.kind === kind).map((t) => t.text);
    expect(of('comment')).toEqual(['(* a tank\n  over two lines *)', '// a line comment with IF in it']);
    expect(of('string')).toEqual(["'it$'s'"]);
    expect(of('number')).toEqual(['16#FF', 'T#1m30s', '5.0E-1', '0']);
    expect(of('type')).toEqual(['INT', 'TIME', 'STRING']);
    expect(of('keyword')).toEqual(['PROGRAM', 'VAR', 'END_VAR', 'IF', 'THEN', 'END_IF', 'END_PROGRAM']);
    expect(tokenize('if x then end_if').filter((t) => t.kind === 'keyword').map((t) => t.text)).toEqual(['if', 'then', 'end_if']);
    // A name that only starts like a keyword is a name.
    expect(tokenize('IFx := VARIABLE;').some((t) => t.kind === 'keyword')).toBe(false);
  });

  it('finds the place of a line and column, and none where the source has no such place', () => {
    expect(offsetOf('ab\ncd', 1, 1)).toBe(0);
    expect(offsetOf('ab\ncd', 2, 2)).toBe(4);
    // One past the last character of a line is where an error at its end points.
    expect(offsetOf('ab\ncd', 1, 3)).toBe(2);
    expect(offsetOf('ab\ncd', 1, 4)).toBeNull();
    expect(offsetOf('ab\ncd', 3, 1)).toBeNull();
    expect(offsetOf('ab', 0, 1)).toBeNull();
  });

  it('counts a column in characters, as the compiler does, not in UTF-16 units', () => {
    // The emoji is one character and two units: the x after it is column 9, offset 9.
    const text = '(* \u{1F600} *) x';
    expect(offsetOf(text, 1, 9)).toBe(text.indexOf('x'));
    expect(offsetOf(text, 1, 10)).toBe(text.length);
    expect(offsetOf(text, 1, 11)).toBeNull();
  });

  it('finds the stretch a diagnostic is about: its range, or the word at a point', () => {
    const text = 'PROGRAM P\n  a := (oops +\n    1);\nEND_PROGRAM';
    const at = (line: number, col: number, end_line: number, end_col: number) =>
      rangeOf(text, { line, col, end_line, end_col });
    const slice = (r: { from: number; to: number } | null) => (r === null ? null : text.slice(r.from, r.to));
    // A range, over lines.
    expect(slice(at(2, 8, 3, 6))).toBe('(oops +\n    1');
    // A point: the word there, or the one character.
    expect(slice(at(2, 9, 2, 9))).toBe('oops');
    expect(slice(at(2, 14, 2, 14))).toBe('+');
    // An end before the start is no range: the point at the start.
    expect(slice(at(2, 9, 1, 1))).toBe('oops');
    // About the whole program, about no place, or about the very end: nothing to underline.
    expect(at(0, 0, 0, 0)).toBeNull();
    expect(at(9, 1, 9, 2)).toBeNull();
    expect(at(4, 12, 4, 12)).toBeNull();
  });
});
