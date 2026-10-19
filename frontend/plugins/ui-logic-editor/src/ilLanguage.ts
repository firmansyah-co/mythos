import type * as Monaco from 'monaco-editor';

/**
 * IEC 61131-3 Instruction List (IL) Language Definition for Monaco Editor.
 */
export const IL_LANGUAGE_ID = 'instruction-list';

export const ilLanguageDefinition: Monaco.languages.IMonarchLanguage = {
  defaultToken: '',
  tokenPostfix: '.il',
  ignoreCase: true,

  keywords: [
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
  ],

  operators: [
    '(', ')', '?', 'N',
  ],

  tokenizer: {
    root: [
      // Instruction List Jump / Branch Labels (e.g. "LABEL1:", "Error_Exit:")
      [/^[ 	]*[a-zA-Z_]\w*:/, 'type.identifier'],
      [/[a-zA-Z_]\w*:/, 'type.identifier'],

      // Direct IEC 61131-3 memory address tokens (%IX0.0, %QX0.1, %MW10, etc.)
      [/%[IQM][XBWDL][0-9]+(\.[0-9]+)?/i, 'variable.predefined'],
      [/%[IQM][0-9]+(\.[0-9]+)?/i, 'variable.predefined'],

      // Standard IL operators & modifiers
      [/&N/i, 'keyword'],
      [/&/, 'keyword'],

      // Keywords & Identifiers
      [/[a-zA-Z_]\w*/, {
        cases: {
          '@keywords': 'keyword',
          '@default': 'identifier',
        },
      }],

      // Whitespace
      { include: '@whitespace' },

      // Numbers: Hex, Binary, Octal, Time, Floating point, Decimal
      [/16#[0-9a-fA-F_]+/, 'number.hex'],
      [/2#[0-1_]+/, 'number.binary'],
      [/8#[0-7_]+/, 'number.octal'],
      [/T#[0-9a-zA-Z_]+/i, 'number'],
      [/TIME#[0-9a-zA-Z_]+/i, 'number'],
      [/\d*\.\d+([eE][\-+]?\d+)?/, 'number.float'],
      [/\d+/, 'number'],

      // Comments: (* ... *) or //
      [/\(\*[\s\S]*?\*\)/, 'comment'],
      [/\/\/.*$/, 'comment'],

      // Strings
      [/'([^'\\]|\\.)*'/, 'string'],
      [/"([^"\\]|\\.)*"/, 'string'],
    ],

    whitespace: [
      [/[ \t\r\n]+/, 'white'],
    ],
  },
};

export const ilLanguageConfiguration: Monaco.languages.LanguageConfiguration = {
  comments: {
    lineComment: '//',
    blockComment: ['(*', '*)'],
  },
  autoClosingPairs: [
    { open: '(*', close: '*)' },
    { open: '(', close: ')' },
    { open: "'", close: "'" },
    { open: '"', close: '"' },
  ],
  brackets: [
    ['(', ')'],
  ],
};

/**
 * Standard IEC 61131-3 IL Documentation for Hover tooltips and autocompletion descriptions.
 */
export const IL_OPERATOR_DOCS: Record<string, string> = {
  LD: 'Load current result with operand value: CR := operand',
  LDN: 'Load current result with negated operand value: CR := NOT operand',
  ST: 'Store current result into operand: operand := CR',
  STN: 'Store negated current result into operand: operand := NOT CR',
  S: 'Set boolean operand to TRUE if CR is TRUE (Latch)',
  R: 'Reset boolean operand to FALSE if CR is TRUE (Unlatch)',
  AND: 'Boolean AND operation: CR := CR AND operand',
  ANDN: 'Boolean AND NOT operation: CR := CR AND (NOT operand)',
  OR: 'Boolean OR operation: CR := CR OR operand',
  ORN: 'Boolean OR NOT operation: CR := CR OR (NOT operand)',
  XOR: 'Boolean XOR operation: CR := CR XOR operand',
  XORN: 'Boolean XOR NOT operation: CR := CR XOR (NOT operand)',
  NOT: 'Invert current result accumulator: CR := NOT CR',
  ADD: 'Arithmetic Addition: CR := CR + operand',
  SUB: 'Arithmetic Subtraction: CR := CR - operand',
  MUL: 'Arithmetic Multiplication: CR := CR * operand',
  DIV: 'Arithmetic Division: CR := CR / operand',
  MOD: 'Modulo division remainder: CR := CR MOD operand',
  GT: 'Greater Than comparison: CR := (CR > operand)',
  GE: 'Greater Than or Equal comparison: CR := (CR >= operand)',
  EQ: 'Equality comparison: CR := (CR = operand)',
  NE: 'Non-equality comparison: CR := (CR <> operand)',
  LE: 'Less Than or Equal comparison: CR := (CR <= operand)',
  LT: 'Less Than comparison: CR := (CR < operand)',
  JMP: 'Unconditional jump to label',
  JMPC: 'Jump to label if current result (CR) is TRUE',
  JMPCN: 'Jump to label if current result (CR) is FALSE',
  CAL: 'Unconditional call to Function Block or Subroutine',
  CALC: 'Conditional call if CR is TRUE',
  CALCN: 'Conditional call if CR is FALSE',
  RET: 'Return from POU unconditionally',
  RETC: 'Return from POU if CR is TRUE',
  RETCN: 'Return from POU if CR is FALSE',
};

/**
 * Register IEC 61131-3 Instruction List language in Monaco.
 */
export function registerInstructionListLanguage(monaco: typeof Monaco) {
  const languages = monaco.languages.getLanguages();
  if (!languages.some((l) => l.id === IL_LANGUAGE_ID)) {
    monaco.languages.register({ id: IL_LANGUAGE_ID });
    monaco.languages.setMonarchTokensProvider(IL_LANGUAGE_ID, ilLanguageDefinition);
    monaco.languages.setLanguageConfiguration(IL_LANGUAGE_ID, ilLanguageConfiguration);

    // Register Autocomplete Provider
    if (monaco.languages.registerCompletionItemProvider) monaco.languages.registerCompletionItemProvider(IL_LANGUAGE_ID, {
      provideCompletionItems: (_model, _position) => {
        const suggestions = (ilLanguageDefinition.keywords as string[]).map((kw) => ({
          label: kw,
          kind: monaco.languages.CompletionItemKind.Keyword,
          insertText: kw + ' ',
          detail: `IEC 61131-3 IL Operator (${kw})`,
          documentation: IL_OPERATOR_DOCS[kw] || `Instruction List Operator: ${kw}`,
        }));
        return { suggestions } as any;
      },
    });

    // Register Hover Provider
    if (monaco.languages.registerHoverProvider) monaco.languages.registerHoverProvider(IL_LANGUAGE_ID, {
      provideHover: (model, position) => {
        const word = model.getWordAtPosition(position);
        if (!word) return null;
        const upper = word.word.toUpperCase();
        const doc = IL_OPERATOR_DOCS[upper];
        if (doc) {
          return {
            range: monaco.Range ? new monaco.Range(
              position.lineNumber,
              word.startColumn,
              position.lineNumber,
              word.endColumn) : undefined as any,
            contents: [
              { value: `**IEC 61131-3 IL Operator**: \`${upper}\`` },
              { value: doc },
            ],
          };
        }
        return null;
      },
    });
  }
}
