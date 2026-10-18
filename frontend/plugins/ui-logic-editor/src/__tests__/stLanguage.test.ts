import { describe, it, expect } from 'vitest';
import { ST_LANGUAGE_ID, stLanguageDefinition } from '../stLanguage';

describe('IEC 61131-3 Structured Text Language Definition', () => {
  it('has valid language id', () => {
    expect(ST_LANGUAGE_ID).toBe('structured-text');
  });

  it('contains mandatory IEC 61131-3 keywords', () => {
    const keywords = stLanguageDefinition.keywords as string[];
    expect(keywords).toContain('PROGRAM');
    expect(keywords).toContain('FUNCTION_BLOCK');
    expect(keywords).toContain('VAR');
    expect(keywords).toContain('VAR_INPUT');
    expect(keywords).toContain('VAR_OUTPUT');
    expect(keywords).toContain('END_VAR');
    expect(keywords).toContain('IF');
    expect(keywords).toContain('THEN');
    expect(keywords).toContain('ELSIF');
    expect(keywords).toContain('ELSE');
    expect(keywords).toContain('END_IF');
    expect(keywords).toContain('CASE');
    expect(keywords).toContain('FOR');
    expect(keywords).toContain('WHILE');
  });

  it('contains IEC standard data types', () => {
    const typeKeywords = stLanguageDefinition.typeKeywords as string[];
    expect(typeKeywords).toContain('BOOL');
    expect(typeKeywords).toContain('INT');
    expect(typeKeywords).toContain('DINT');
    expect(typeKeywords).toContain('REAL');
    expect(typeKeywords).toContain('TIME');
    expect(typeKeywords).toContain('STRING');
  });
});
