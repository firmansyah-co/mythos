import type * as Monaco from 'monaco-editor';

/**
 * IEC 61131-3 Structured Text Language Definition for Monaco Editor.
 */
export const ST_LANGUAGE_ID = 'structured-text';

export const stLanguageDefinition: Monaco.languages.IMonarchLanguage = {
  defaultToken: '',
  tokenPostfix: '.st',
  ignoreCase: true,

  keywords: [
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
  ],

  typeKeywords: [
    'BOOL', 'SINT', 'INT', 'DINT', 'LINT',
    'USINT', 'UINT', 'UDINT', 'ULINT',
    'REAL', 'LREAL',
    'TIME', 'TOD', 'TIME_OF_DAY', 'DATE', 'DT', 'DATE_AND_TIME',
    'STRING', 'WSTRING',
    'BYTE', 'WORD', 'DWORD', 'LWORD',
  ],

  operators: [
    ':=', '=', '<>', '<', '<=', '>', '>=',
    '+', '-', '*', '/', '**',
    '&',
  ],

  symbols: /[=><!~?:&|+\-*\/\^%]+/,

  escapes: /\\(?:[abfnrtv\\\"']|x[0-9A-Fa-f]{1,4}|u[0-9A-Fa-f]{4}|U[0-9A-Fa-f]{8})/,

  tokenizer: {
    root: [
      // Direct variable addresses: %IX0.0, %QX0.1, %IW10, %MW100, etc.
      [/%[IQM][XBWDL][0-9]+(\.[0-9]+)?/i, 'variable.predefined'],
      [/%[IQM][0-9]+(\.[0-9]+)?/i, 'variable.predefined'],

      // Identifiers and keywords
      [/[a-zA-Z_][a-zA-Z0-9_]*/, {
        cases: {
          '@keywords': 'keyword',
          '@typeKeywords': 'type',
          '@default': 'identifier',
        },
      }],

      // Whitespace
      { include: '@whitespace' },

      // Delimiters and operators
      [/[{}()\[\]]/, '@brackets'],
      [/[;,.]/, 'delimiter'],
      [/@symbols/, {
        cases: {
          '@operators': 'operator',
          '@default': '',
        },
      }],

      // Numbers
      [/16#[0-9a-fA-F_]+/, 'number.hex'],
      [/2#[0-1_]+/, 'number.binary'],
      [/8#[0-7_]+/, 'number.octal'],
      [/T#[0-9a-zA-Z_]+/i, 'number'], // Time literal T#10s, T#500ms
      [/TIME#[0-9a-zA-Z_]+/i, 'number'],
      [/\d*\.\d+([eE][\-+]?\d+)?/, 'number.float'],
      [/\d+/, 'number'],

      // Strings
      [/'([^'\\]|\\.)*'/, 'string'],
      [/"([^"\\]|\\.)*"/, 'string'],
    ],

    whitespace: [
      [/[ \t\r\n]+/, 'white'],
      [/\(\*/, 'comment', '@comment'],
      [/\/\/.*$/, 'comment'],
    ],

    comment: [
      [/[^\*]+/, 'comment'],
      [/\*\)/, 'comment', '@pop'],
      [/\*/, 'comment'],
    ],
  },
};

export const stLanguageConfiguration: Monaco.languages.LanguageConfiguration = {
  comments: {
    lineComment: '//',
    blockComment: ['(*', '*)'],
  },
  brackets: [
    ['(', ')'],
    ['[', ']'],
  ],
  autoClosingPairs: [
    { open: '(', close: ')' },
    { open: '[', close: ']' },
    { open: "'", close: "'", notIn: ['string', 'comment'] },
    { open: '"', close: '"', notIn: ['string', 'comment'] },
    { open: '(*', close: '*)', notIn: ['string'] },
  ],
  surroundingPairs: [
    { open: '(', close: ')' },
    { open: '[', close: ']' },
    { open: "'", close: "'" },
    { open: '"', close: '"' },
  ],
};

/**
 * Register IEC 61131-3 Structured Text in Monaco Editor.
 */
export function registerStructuredTextLanguage(monaco: typeof Monaco) {
  const languages = monaco.languages.getLanguages();
  if (!languages.some((l) => l.id === ST_LANGUAGE_ID)) {
    monaco.languages.register({ id: ST_LANGUAGE_ID });
    monaco.languages.setMonarchTokensProvider(ST_LANGUAGE_ID, stLanguageDefinition);
    monaco.languages.setLanguageConfiguration(ST_LANGUAGE_ID, stLanguageConfiguration);
  }
}
