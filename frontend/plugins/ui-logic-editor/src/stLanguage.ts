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
 * Standard IEC 61131-3 Keywords & Standard Functions Documentation
 */
export const ST_KEYWORD_DOCS: Record<string, string> = {
  PROGRAM: 'Defines an IEC 61131-3 Program block (entry point for task execution).',
  FUNCTION_BLOCK: 'Defines an IEC 61131-3 Function Block with stateful instance memory.',
  FUNCTION: 'Defines an IEC 61131-3 Function without internal state.',
  VAR: 'Declaration section for local variables.',
  VAR_INPUT: 'Declaration section for incoming parameters.',
  VAR_OUTPUT: 'Declaration section for outgoing parameters.',
  VAR_IN_OUT: 'Declaration section for bidirectional reference parameters.',
  VAR_GLOBAL: 'Declaration section for globally scoped variables.',
  IF: 'Conditional branch: executes statements when condition evaluates to TRUE.',
  CASE: 'Multi-way selection based on an integer or enumerated expression.',
  FOR: 'Deterministic counted loop with index variable, TO, and optional BY.',
  WHILE: 'Condition-controlled loop executing while condition remains TRUE.',
  REPEAT: 'Condition-controlled loop evaluating termination condition at the end (UNTIL).',
  TON: 'Timer On-Delay: Q activates after PT time has elapsed while IN is TRUE.',
  TOF: 'Timer Off-Delay: Q remains TRUE for PT time after IN transitions to FALSE.',
  TP: 'Timer Pulse: Q pulses TRUE for fixed duration PT upon rising edge of IN.',
  CTU: 'Count Up: CV increments on rising edge of CU up to limit PV.',
  CTD: 'Count Down: CV decrements on rising edge of CD down to 0.',
};

/**
 * Register IEC 61131-3 Structured Text in Monaco Editor with full autocomplete, hover, and snippet support.
 */
export function registerStructuredTextLanguage(monaco: typeof Monaco) {
  const languages = monaco.languages.getLanguages();
  if (!languages.some((l) => l.id === ST_LANGUAGE_ID)) {
    monaco.languages.register({ id: ST_LANGUAGE_ID });
    monaco.languages.setMonarchTokensProvider(ST_LANGUAGE_ID, stLanguageDefinition);
    monaco.languages.setLanguageConfiguration(ST_LANGUAGE_ID, stLanguageConfiguration);

    // Register Autocomplete Provider
    if (monaco.languages.registerCompletionItemProvider) monaco.languages.registerCompletionItemProvider(ST_LANGUAGE_ID, {
      provideCompletionItems: (_model, _position) => {
        const keywords = (stLanguageDefinition.keywords as string[]).map((kw) => ({
          label: kw,
          kind: monaco.languages.CompletionItemKind.Keyword,
          insertText: kw,
          detail: `IEC 61131-3 Keyword (${kw})`,
          documentation: ST_KEYWORD_DOCS[kw] || `Structured Text Keyword: ${kw}`,
        }));

        const typeKeywords = (stLanguageDefinition.typeKeywords as string[]).map((tk) => ({
          label: tk,
          kind: monaco.languages.CompletionItemKind.TypeParameter,
          insertText: tk,
          detail: `IEC Data Type (${tk})`,
        }));

        const snippets = [
          {
            label: 'IF-THEN',
            kind: monaco.languages.CompletionItemKind.Snippet,
            insertText: ['IF ${1:condition} THEN', '	${2:// statements}', 'END_IF;'].join('\n'),
            insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
            detail: 'IF-THEN statement',
          },
          {
            label: 'IF-ELSE',
            kind: monaco.languages.CompletionItemKind.Snippet,
            insertText: ['IF ${1:condition} THEN', '	${2:// statements}', 'ELSE', '	${3:// else}', 'END_IF;'].join('\n'),
            insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
            detail: 'IF-ELSE statement',
          },
          {
            label: 'FOR-LOOP',
            kind: monaco.languages.CompletionItemKind.Snippet,
            insertText: ['FOR ${1:i} := ${2:0} TO ${3:10} DO', '	${4:// statements}', 'END_FOR;'].join('\n'),
            insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
            detail: 'FOR loop',
          },
          {
            label: 'VAR-BLOCK',
            kind: monaco.languages.CompletionItemKind.Snippet,
            insertText: ['VAR', '	${1:var_name} : ${2:BOOL} := ${3:FALSE};', 'END_VAR'].join('\n'),
            insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
            detail: 'Variable declaration block',
          },
        ];

        return { suggestions: [...keywords, ...typeKeywords, ...snippets] } as any;
      },
    });

    // Register Hover Provider
    if (monaco.languages.registerHoverProvider) monaco.languages.registerHoverProvider(ST_LANGUAGE_ID, {
      provideHover: (model, position) => {
        const word = model.getWordAtPosition(position);
        if (!word) return null;
        const upper = word.word.toUpperCase();
        const doc = ST_KEYWORD_DOCS[upper];
        if (doc) {
          return {
            range: monaco.Range ? new monaco.Range(
              position.lineNumber,
              word.startColumn,
              position.lineNumber,
              word.endColumn) : undefined as any,
            contents: [
              { value: `**IEC 61131-3 Keyword / POU**: \`${upper}\`` },
              { value: doc },
            ],
          };
        }
        return null;
      },
    });
  }
}
