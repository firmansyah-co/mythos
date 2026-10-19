import { describe, it, expect, vi } from 'vitest';
import {
  IL_LANGUAGE_ID,
  ilLanguageDefinition,
  ilLanguageConfiguration,
  IL_OPERATOR_DOCS,
  registerInstructionListLanguage,
} from '../ilLanguage';

describe('TSK-701: IEC 61131-3 Instruction List (IL) Language Suite', () => {
  it('defines valid language ID as instruction-list', () => {
    expect(IL_LANGUAGE_ID).toBe('instruction-list');
  });

  it('contains all standard IEC 61131-3 IL operator keywords', () => {
    const keywords = ilLanguageDefinition.keywords as string[];
    const expectedOperators = [
      'LD', 'LDN', 'ST', 'STN', 'S', 'R',
      'AND', 'ANDN', '&', '&N',
      'OR', 'ORN',
      'XOR', 'XORN',
      'NOT',
      'ADD', 'SUB', 'MUL', 'DIV', 'MOD',
      'GT', 'GE', 'EQ', 'NE', 'LE', 'LT',
      'JMP', 'JMPC', 'JMPCN', 'JMPN',
      'CAL', 'CALC', 'CALCN', 'CALN',
      'RET', 'RETC', 'RETCN', 'RETN',
    ];
    for (const op of expectedOperators) {
      expect(keywords).toContain(op);
    }
  });

  it('contains documentation entries for all primary IL operators', () => {
    expect(IL_OPERATOR_DOCS['LD']).toContain('Load current result');
    expect(IL_OPERATOR_DOCS['LDN']).toContain('Load current result with negated operand');
    expect(IL_OPERATOR_DOCS['ST']).toContain('Store current result');
    expect(IL_OPERATOR_DOCS['STN']).toContain('Store negated current result');
    expect(IL_OPERATOR_DOCS['S']).toContain('Set boolean operand');
    expect(IL_OPERATOR_DOCS['R']).toContain('Reset boolean operand');
    expect(IL_OPERATOR_DOCS['AND']).toContain('Boolean AND');
    expect(IL_OPERATOR_DOCS['OR']).toContain('Boolean OR');
    expect(IL_OPERATOR_DOCS['JMP']).toContain('jump');
    expect(IL_OPERATOR_DOCS['CAL']).toContain('Function Block');
    expect(IL_OPERATOR_DOCS['RET']).toContain('Return');
  });

  it('configures proper comment characters and auto-closing pairs', () => {
    expect(ilLanguageConfiguration.comments?.lineComment).toBe('//');
    expect(ilLanguageConfiguration.comments?.blockComment).toEqual(['(*', '*)']);
    expect(ilLanguageConfiguration.brackets).toEqual([['(', ')']]);
  });

  it('registers IL language, monarch tokens provider, completion and hover providers in Monaco', () => {
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
        CompletionItemKind: { Keyword: 14 },
        Range: class {
          constructor(public a: number, public b: number, public c: number, public d: number) {}
        },
      },
    };

    registerInstructionListLanguage(mockMonaco as any);

    expect(mockMonaco.languages.register).toHaveBeenCalledWith({ id: IL_LANGUAGE_ID });
    expect(mockMonaco.languages.setMonarchTokensProvider).toHaveBeenCalledWith(
      IL_LANGUAGE_ID,
      ilLanguageDefinition
    );
    expect(mockMonaco.languages.setLanguageConfiguration).toHaveBeenCalledWith(
      IL_LANGUAGE_ID,
      ilLanguageConfiguration
    );
    expect(mockMonaco.languages.registerCompletionItemProvider).toHaveBeenCalled();
    expect(mockMonaco.languages.registerHoverProvider).toHaveBeenCalled();

    // Verify Autocomplete Suggestions
    expect(completionProvider).toBeTruthy();
    const completions = completionProvider.provideCompletionItems({}, {});
    expect(completions.suggestions.length).toBeGreaterThan(20);
    const ldSuggest = completions.suggestions.find((s: any) => s.label === 'LD');
    expect(ldSuggest).toBeTruthy();
    expect(ldSuggest.insertText).toBe('LD ');

    // Verify Hover Provider
    expect(hoverProvider).toBeTruthy();
    const mockModel = {
      getWordAtPosition: vi.fn().mockReturnValue({ word: 'LD', startColumn: 1, endColumn: 3 }),
    };
    const hover = hoverProvider.provideHover(mockModel, { lineNumber: 1, column: 2 });
    expect(hover).toBeTruthy();
    expect(hover.contents[0].value).toContain('`LD`');
    expect(hover.contents[1].value).toContain('Load current result');

    // Hover for unknown word returns null
    mockModel.getWordAtPosition.mockReturnValue({ word: 'UNKNOWN_VAR', startColumn: 1, endColumn: 11 });
    expect(hoverProvider.provideHover(mockModel, { lineNumber: 1, column: 2 })).toBeNull();
  });
});
