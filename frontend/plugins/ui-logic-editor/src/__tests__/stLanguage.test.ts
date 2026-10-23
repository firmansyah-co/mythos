import { describe, it, expect, vi } from 'vitest';
import {
  ST_LANGUAGE_ID,
  stLanguageDefinition,
  stLanguageConfiguration,
  ST_KEYWORD_DOCS,
  registerStructuredTextLanguage,
} from '../stLanguage';

describe('TSK-701: IEC 61131-3 Structured Text (ST) Language Suite', () => {
  it('defines valid language ID as structured-text', () => {
    expect(ST_LANGUAGE_ID).toBe('structured-text');
  });

  it('contains all standard IEC 61131-3 keywords and control structures', () => {
    const keywords = stLanguageDefinition.keywords as string[];
    const expected = [
      'PROGRAM', 'END_PROGRAM',
      'FUNCTION', 'END_FUNCTION',
      'FUNCTION_BLOCK', 'END_FUNCTION_BLOCK',
      'CONFIGURATION', 'END_CONFIGURATION',
      'RESOURCE', 'ON', 'END_RESOURCE',
      'VAR', 'VAR_INPUT', 'VAR_OUTPUT', 'VAR_IN_OUT', 'VAR_GLOBAL', 'VAR_TEMP', 'VAR_STAT', 'VAR_EXTERNAL', 'END_VAR',
      'CONSTANT', 'RETAIN', 'NON_RETAIN', 'AT',
      'TYPE', 'STRUCT', 'END_STRUCT', 'END_TYPE',
      'IF', 'THEN', 'ELSIF', 'ELSE', 'END_IF',
      'CASE', 'OF', 'END_CASE',
      'FOR', 'TO', 'BY', 'DO', 'END_FOR',
      'WHILE', 'END_WHILE',
      'REPEAT', 'UNTIL', 'END_REPEAT',
      'EXIT', 'RETURN',
      'AND', 'OR', 'XOR', 'NOT', 'MOD',
      'TRUE', 'FALSE',
    ];
    for (const kw of expected) {
      expect(keywords).toContain(kw);
    }
  });

  it('contains full spectrum of IEC 61131-3 elementary data types', () => {
    const typeKeywords = stLanguageDefinition.typeKeywords as string[];
    const expectedTypes = [
      'BOOL', 'SINT', 'INT', 'DINT', 'LINT',
      'USINT', 'UINT', 'UDINT', 'ULINT',
      'REAL', 'LREAL',
      'TIME', 'TOD', 'TIME_OF_DAY', 'DATE', 'DT', 'DATE_AND_TIME',
      'STRING', 'WSTRING',
      'BYTE', 'WORD', 'DWORD', 'LWORD',
    ];
    for (const tk of expectedTypes) {
      expect(typeKeywords).toContain(tk);
    }
  });

  it('defines brackets, comments and auto-closing configuration', () => {
    expect(stLanguageConfiguration.comments?.lineComment).toBe('//');
    expect(stLanguageConfiguration.comments?.blockComment).toEqual(['(*', '*)']);
    expect(stLanguageConfiguration.brackets).toEqual([
      ['(', ')'],
      ['[', ']'],
    ]);
    expect(stLanguageConfiguration.autoClosingPairs).toContainEqual(
      expect.objectContaining({ open: '(', close: ')' })
    );
    expect(stLanguageConfiguration.autoClosingPairs).toContainEqual(
      expect.objectContaining({ open: '[', close: ']' })
    );
    expect(stLanguageConfiguration.autoClosingPairs).toContainEqual(
      expect.objectContaining({ open: '(*', close: '*)' })
    );
  });

  it('provides documentation dictionary for standard keywords', () => {
    expect(ST_KEYWORD_DOCS['PROGRAM']).toContain('Program block');
    expect(ST_KEYWORD_DOCS['FUNCTION_BLOCK']).toContain('Function Block');
    expect(ST_KEYWORD_DOCS['VAR_INPUT']).toContain('incoming parameters');
    expect(ST_KEYWORD_DOCS['VAR_OUTPUT']).toContain('outgoing parameters');
    expect(ST_KEYWORD_DOCS['IF']).toContain('Conditional branch');
    expect(ST_KEYWORD_DOCS['CASE']).toContain('Multi-way');
  });

  it('registers language, token provider, snippets autocomplete and hover documentation in Monaco', () => {
    let completionProvider: any = null;
    let hoverProvider: any = null;

    const mockMonaco = {
      languages: {
        getLanguages: vi.fn().mockReturnValue([]),
        register: vi.fn(),
        setMonarchTokensProvider: vi.fn(),
        setLanguageConfiguration: vi.fn(),
        registerCompletionItemProvider: vi.fn().mockImplementation((id, provider) => {
          completionProvider = provider;
        }),
        registerHoverProvider: vi.fn().mockImplementation((id, provider) => {
          hoverProvider = provider;
        }),
        CompletionItemKind: { Keyword: 14, TypeParameter: 25, Snippet: 27 },
        CompletionItemInsertTextRule: { InsertAsSnippet: 4 },
        Range: class {
          constructor(public a: number, public b: number, public c: number, public d: number) {}
        },
      },
    };

    registerStructuredTextLanguage(mockMonaco as any);

    expect(mockMonaco.languages.register).toHaveBeenCalledWith({ id: ST_LANGUAGE_ID });
    expect(mockMonaco.languages.setMonarchTokensProvider).toHaveBeenCalledWith(
      ST_LANGUAGE_ID,
      stLanguageDefinition
    );
    expect(mockMonaco.languages.setLanguageConfiguration).toHaveBeenCalledWith(
      ST_LANGUAGE_ID,
      stLanguageConfiguration
    );
    expect(mockMonaco.languages.registerCompletionItemProvider).toHaveBeenCalled();
    expect(mockMonaco.languages.registerHoverProvider).toHaveBeenCalled();

    // Test Autocomplete Suggestions
    expect(completionProvider).toBeTruthy();
    const completions = completionProvider.provideCompletionItems({}, {});
    expect(completions.suggestions.length).toBeGreaterThan(30);

    const ifSnippet = completions.suggestions.find((s: any) => s.label === 'IF-THEN');
    expect(ifSnippet).toBeTruthy();
    expect(ifSnippet.insertText).toContain('IF ${1:condition} THEN');

    const varSnippet = completions.suggestions.find((s: any) => s.label === 'VAR-BLOCK');
    expect(varSnippet).toBeTruthy();
    expect(varSnippet.insertText).toContain('END_VAR');

    // Test Hover Provider
    expect(hoverProvider).toBeTruthy();
    const mockModel = {
      getWordAtPosition: vi.fn().mockReturnValue({ word: 'PROGRAM', startColumn: 1, endColumn: 8 }),
    };
    const hover = hoverProvider.provideHover(mockModel, { lineNumber: 1, column: 3 });
    expect(hover).toBeTruthy();
    expect(hover.contents[0].value).toContain('`PROGRAM`');
    expect(hover.contents[1].value).toContain('IEC 61131-3 Program block');

    // Hover for unknown word
    mockModel.getWordAtPosition.mockReturnValue({ word: 'MY_VAR_1', startColumn: 1, endColumn: 9 });
    expect(hoverProvider.provideHover(mockModel, { lineNumber: 1, column: 3 })).toBeNull();
  });
});
