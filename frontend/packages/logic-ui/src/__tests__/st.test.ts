import { describe, expect, it } from 'vitest';
import { offsetOf, tokenize } from '../st';

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
});
