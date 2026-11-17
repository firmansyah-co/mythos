// A watched variable's value as text, in its declared type. What does not have the shape its
// type declares gives `null`, so the caller shows no value instead of a guess.

const INTEGERS = new Set(['SINT', 'INT', 'DINT', 'LINT']);

/** The types whose variables have one value to show. */
export function hasOneValue(type: string): boolean {
  return type === 'BOOL' || type === 'REAL' || type === 'LREAL' || type === 'TIME' || INTEGERS.has(type);
}

export function formatVariable(type: string, value: unknown): string | null {
  if (type === 'BOOL') return typeof value === 'boolean' ? (value ? 'TRUE' : 'FALSE') : null;
  if (typeof value !== 'number' || !Number.isFinite(value)) return null;
  if (INTEGERS.has(type)) return Number.isInteger(value) ? String(value) : null;
  // A REAL is a 32-bit float carried as a double: more digits than it has would be noise.
  if (type === 'REAL') return String(Number(value.toPrecision(7)));
  if (type === 'LREAL') return String(Number(value.toPrecision(15)));
  // A TIME is its nanoseconds; shown in milliseconds, to the microsecond.
  if (type === 'TIME') return Number.isInteger(value) ? `${Number((value / 1e6).toFixed(3))} ms` : null;
  return null;
}
