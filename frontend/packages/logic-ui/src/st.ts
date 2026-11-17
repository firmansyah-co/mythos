// Structured Text as the editor paints it: comments, strings, numbers, keywords and type names.
// This is colouring only; what a program means is the compiler's to say (the logic module's
// `check`), never this file's.

export type TokenKind = 'keyword' | 'type' | 'comment' | 'string' | 'number' | 'text';

export interface Token {
  kind: TokenKind;
  text: string;
  /** Where it starts in the source. */
  offset: number;
}

const KEYWORDS = new Set(
  `PROGRAM END_PROGRAM FUNCTION END_FUNCTION FUNCTION_BLOCK END_FUNCTION_BLOCK TYPE END_TYPE
   STRUCT END_STRUCT VAR VAR_INPUT VAR_OUTPUT VAR_IN_OUT VAR_TEMP VAR_GLOBAL VAR_EXTERNAL END_VAR
   CONSTANT RETAIN NON_RETAIN AT ARRAY OF IF THEN ELSIF ELSE END_IF CASE END_CASE FOR TO BY DO
   END_FOR WHILE END_WHILE REPEAT UNTIL END_REPEAT EXIT RETURN CONTINUE AND OR XOR NOT MOD TRUE
   FALSE`.split(/\s+/)
);

const TYPES = new Set(
  `BOOL SINT INT DINT LINT USINT UINT UDINT ULINT REAL LREAL TIME DATE TIME_OF_DAY TOD
   DATE_AND_TIME DT STRING WSTRING BYTE WORD DWORD LWORD TON TOF TP CTU CTD CTUD R_TRIG F_TRIG
   SR RS`.split(/\s+/)
);

const IDENT_START = /[A-Za-z_]/;
const IDENT = /[A-Za-z0-9_]/;
const DIGIT = /[0-9]/;

/**
 * Split `source` into tokens. Every character of the source is in exactly one token, in order,
 * so the painted text is the source: a comment or a string that is never closed runs to the end.
 */
export function tokenize(source: string): Token[] {
  const tokens: Token[] = [];
  let at = 0;
  let plain = 0;
  const flush = (end: number) => {
    if (end > plain) tokens.push({ kind: 'text', text: source.slice(plain, end), offset: plain });
  };
  const push = (kind: TokenKind, end: number) => {
    flush(at);
    tokens.push({ kind, text: source.slice(at, end), offset: at });
    at = end;
    plain = end;
  };
  const closing = (from: number, close: string) => {
    const found = source.indexOf(close, from);
    return found < 0 ? source.length : found + close.length;
  };

  while (at < source.length) {
    const c = source[at] ?? '';
    const two = source.slice(at, at + 2);
    if (two === '(*') push('comment', closing(at + 2, '*)'));
    else if (two === '/*') push('comment', closing(at + 2, '*/'));
    else if (two === '//') {
      const end = source.indexOf('\n', at);
      push('comment', end < 0 ? source.length : end);
    } else if (c === "'" || c === '"') {
      // `$` escapes the next character in a string literal, the quote included.
      let end = at + 1;
      while (end < source.length && source[end] !== c && source[end] !== '\n') {
        end += source[end] === '$' ? 2 : 1;
      }
      push('string', Math.min(end + 1, source.length));
    } else if (DIGIT.test(c)) {
      // 12, 1.5, 1.0E-3, 16#FF, 2#1010_0001.
      let end = at + 1;
      while (end < source.length && /[0-9A-Za-z_#]/.test(source[end] ?? '')) end += 1;
      if (source[end] === '.' && DIGIT.test(source[end + 1] ?? '')) {
        end += 1;
        while (end < source.length && /[0-9_]/.test(source[end] ?? '')) end += 1;
        if (/[eE]/.test(source[end] ?? '')) {
          const sign = /[+-]/.test(source[end + 1] ?? '') ? 2 : 1;
          if (DIGIT.test(source[end + sign] ?? '')) {
            end += sign;
            while (end < source.length && DIGIT.test(source[end] ?? '')) end += 1;
          }
        }
      }
      push('number', end);
    } else if (IDENT_START.test(c)) {
      let end = at + 1;
      while (end < source.length && IDENT.test(source[end] ?? '')) end += 1;
      const word = source.slice(at, end).toUpperCase();
      // T#5s, TIME#1m30s: a duration literal, painted as a number.
      if ((word === 'T' || word === 'TIME') && source[end] === '#') {
        end += 1;
        while (end < source.length && /[0-9A-Za-z_.]/.test(source[end] ?? '')) end += 1;
        push('number', end);
      } else if (KEYWORDS.has(word)) push('keyword', end);
      else if (TYPES.has(word)) push('type', end);
      else at = end;
    } else {
      at += 1;
    }
  }
  flush(source.length);
  return tokens;
}

/** The offset of a 1-based line and column, or `null` when the source has no such place. */
export function offsetOf(source: string, line: number, col: number): number | null {
  if (line < 1 || col < 1) return null;
  let start = 0;
  for (let current = 1; current < line; current += 1) {
    const next = source.indexOf('\n', start);
    if (next < 0) return null;
    start = next + 1;
  }
  const end = source.indexOf('\n', start);
  const length = (end < 0 ? source.length : end) - start;
  return col - 1 <= length ? start + col - 1 : null;
}
