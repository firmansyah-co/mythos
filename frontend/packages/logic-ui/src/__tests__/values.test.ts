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
  });

  it('is nothing when it is not a value of its type, never a guess', () => {
    expect(formatVariable('BOOL', 1)).toBeNull();
    expect(formatVariable('INT', true)).toBeNull();
    expect(formatVariable('INT', 1.5)).toBeNull();
    expect(formatVariable('INT', null)).toBeNull();
    expect(formatVariable('REAL', '1')).toBeNull();
    expect(formatVariable('REAL', Number.NaN)).toBeNull();
    expect(formatVariable('Motor', 1)).toBeNull();
  });

  it('knows which types have one value', () => {
    expect(['BOOL', 'SINT', 'INT', 'DINT', 'LINT', 'REAL', 'LREAL', 'TIME'].every(hasOneValue)).toBe(true);
    expect(['ARRAY [1..4] OF INT', 'TON', 'Motor', 'STRING'].some(hasOneValue)).toBe(false);
  });
});
