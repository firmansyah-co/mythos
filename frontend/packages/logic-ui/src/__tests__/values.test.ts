import { describe, expect, it } from 'vitest';
import { formatVariable, hasOneValue } from '../values';

describe('a watched value as text', () => {
  it('is written in its type', () => {
    expect(formatVariable('BOOL', true)).toBe('TRUE');
    expect(formatVariable('BOOL', false)).toBe('FALSE');
    expect(formatVariable('SINT', -5)).toBe('-5');
    expect(formatVariable('DINT', 0)).toBe('0');
    // A REAL carried as a double shows the digits a REAL has.
    expect(formatVariable('REAL', 0.10000000149011612)).toBe('0.1');
    expect(formatVariable('LREAL', 0.1)).toBe('0.1');
    // A TIME is nanoseconds.
    expect(formatVariable('TIME', 1_500_000_000)).toBe('1500 ms');
    expect(formatVariable('TIME', 2_500)).toBe('0.003 ms');
    // A STRING as Structured Text writes it: quoted, with its $ combinations.
    expect(formatVariable('STRING[12]', 'level 20')).toBe("'level 20'");
    expect(formatVariable('STRING[4]', '')).toBe("''");
    expect(formatVariable('STRING[12]', "it's $5")).toBe("'it$'s $$5'");
    expect(formatVariable('STRING[4]', 'a\né')).toBe("'a$0A$E9'");
  });

  it('is nothing when it is not a value of its type, never a guess', () => {
    expect(formatVariable('BOOL', 1)).toBeNull();
    expect(formatVariable('INT', true)).toBeNull();
    expect(formatVariable('INT', 1.5)).toBeNull();
    expect(formatVariable('INT', null)).toBeNull();
    expect(formatVariable('REAL', '1')).toBeNull();
    expect(formatVariable('REAL', Number.NaN)).toBeNull();
    expect(formatVariable('Motor', 1)).toBeNull();
    // Text where a STRING is, no more characters than it holds, and each one a byte.
    expect(formatVariable('STRING[4]', 4)).toBeNull();
    expect(formatVariable('STRING[4]', 'abcde')).toBeNull();
    expect(formatVariable('STRING[4]', '→')).toBeNull();
    expect(formatVariable('INT', '4')).toBeNull();
  });

  it('knows which types have one value', () => {
    expect(['BOOL', 'SINT', 'INT', 'DINT', 'LINT', 'REAL', 'LREAL', 'TIME', 'STRING[254]'].every(hasOneValue)).toBe(
      true
    );
    expect(['ARRAY [1..4] OF INT', 'TON', 'Motor', 'STRING', 'ARRAY [1..2] OF STRING[4]'].some(hasOneValue)).toBe(
      false
    );
  });
});
