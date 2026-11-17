// A watched variable's value as text, in its declared type. What does not have the shape its
// type declares gives `null`, so the caller shows no value instead of a guess.

const INTEGERS = new Set(['SINT', 'INT', 'DINT', 'LINT']);

/** The length of a `STRING[n]` (the name the compiler gives the type), or null. */
function stringLength(type: string): number | null {
  const found = /^STRING\[(\d+)\]$/.exec(type);
  return found ? Number(found[1]) : null;
}

/** The types whose variables have one value to show. */
export function hasOneValue(type: string): boolean {
  return (
    type === 'BOOL' ||
    type === 'REAL' ||
    type === 'LREAL' ||
    type === 'TIME' ||
    INTEGERS.has(type) ||
    stringLength(type) !== null
  );
}

/**
 * A STRING as Structured Text writes it: between quotes, `$` and `'` as `$$` and `$'`, and a
 * character that is not printable as `$` and its two hexadecimal digits. Each character of
 * the value is one byte of the string; a code past 255 is not one, and gives null.
 */
function stringLiteral(value: string, length: number): string | null {
  const chars = Array.from(value);
  if (chars.length > length) return null;
  let text = "'";
  for (const ch of chars) {
    const code = ch.codePointAt(0) ?? 0;
    if (code > 0xff) return null;
    if (ch === '$') text += '$$';
    else if (ch === "'") text += "$'";
    else if (code >= 0x20 && code <= 0x7e) text += ch;
    else text += `$${code.toString(16).toUpperCase().padStart(2, '0')}`;
  }
  return `${text}'`;
}

export function formatVariable(type: string, value: unknown): string | null {
  if (type === 'BOOL') return typeof value === 'boolean' ? (value ? 'TRUE' : 'FALSE') : null;
  const length = stringLength(type);
  if (length !== null) return typeof value === 'string' ? stringLiteral(value, length) : null;
  if (typeof value !== 'number' || !Number.isFinite(value)) return null;
  if (INTEGERS.has(type)) return Number.isInteger(value) ? String(value) : null;
  // A REAL is a 32-bit float carried as a double: more digits than it has would be noise.
  if (type === 'REAL') return String(Number(value.toPrecision(7)));
  if (type === 'LREAL') return String(Number(value.toPrecision(15)));
  // A TIME is its nanoseconds; shown in milliseconds, to the microsecond.
  if (type === 'TIME') return Number.isInteger(value) ? `${Number((value / 1e6).toFixed(3))} ms` : null;
  return null;
}
