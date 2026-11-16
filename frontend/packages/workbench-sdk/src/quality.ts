// What a tag is allowed to say on screen.
//
// The wire carries, for every tag the controller could read from the tag bus: a declared type,
// a value, and a quality. A device value that can no longer be read keeps its last reading and
// carries a bad quality; a tag that was never read has type `UNSPECIFIED` and a null value.
//
// One rule governs everything here: **a value is presented as current only when its quality is
// exactly GOOD and the stream that delivered it is open.** Everything else is shown as what it
// is — stale, never read, or unknown — and never as a number that looks like a measurement.

export type StreamState = 'IDLE' | 'CONNECTING' | 'OPEN' | 'CLOSED';

/** One tag as the most recent telemetry batch reported it. */
export interface TagSample {
  uri: string;
  /** Declared type name: "BOOL", "REAL", ..., or "UNSPECIFIED". */
  type: string;
  /** The value in the tag's own JSON type. `null` when the tag has none. */
  value: unknown;
  /** Specific quality state, e.g. "GOOD", "BAD_COMM_FAILURE". Empty when the wire had none. */
  qualityName: string;
  /** Raw quality word. `-1` when the wire had none. */
  qualityCode: number;
  /** Unix time in nanoseconds of the last successful read. `0` when there has been none. */
  timestampNs: number;
}

export const UNSPECIFIED_TYPE = 'UNSPECIFIED';

/** `QualityCode::GOOD` in `mythos-shm`. */
const QUALITY_CODE_GOOD = 0x00c0;

/**
 * Whether a sample is a current, trustworthy measurement.
 *
 * Only the one named GOOD state qualifies. An uncertain value, a name this build does not know,
 * and a missing name all fail: none of them is a measurement an operator may act on. When the
 * name is absent the raw word is accepted only if it is exactly the GOOD code; testing the class
 * bits instead would pass words this build cannot name.
 */
export function isGood(sample: TagSample): boolean {
  if (sample.type === UNSPECIFIED_TYPE || sample.value === null || sample.value === undefined) {
    return false;
  }
  if (sample.qualityName.length > 0) return sample.qualityName === 'GOOD';
  return sample.qualityCode === QUALITY_CODE_GOOD;
}

export type QualityTone = 'good' | 'bad' | 'warn' | 'neutral';

/** How a tag is to be presented. Each variant is a distinct thing to tell the reader. */
export type TagView =
  /** The workbench is not reading the controller. Nothing is known about any value. */
  | { kind: 'offline' }
  /** The stream is open and the controller reported nothing for this tag. */
  | { kind: 'no-data' }
  /** The controller knows the tag and has never read a value for it. */
  | { kind: 'never-read'; qualityLabel: string; tone: QualityTone }
  /** A current measurement. */
  | { kind: 'live'; text: string }
  /** The last value that was read, which is no longer current. */
  | { kind: 'stale'; text: string; qualityLabel: string; tone: QualityTone };

/** "BAD_COMM_FAILURE" -> "BAD COMM FAILURE". An absent name is reported as such. */
export function qualityLabel(sample: TagSample): string {
  if (sample.qualityName.length > 0) return sample.qualityName.replace(/_/g, ' ');
  return sample.qualityCode >= 0 ? `QUALITY 0x${sample.qualityCode.toString(16)}` : 'NO QUALITY';
}

function toneFor(sample: TagSample): QualityTone {
  const name = sample.qualityName;
  if (name === 'GOOD') return 'good';
  if (name === 'BAD_CONFIG_ERROR' || name.startsWith('UNCERTAIN_')) return 'warn';
  if (name === 'BAD_NOT_CONNECTED') return 'neutral';
  return 'bad';
}

/**
 * Render a value in its declared type.
 *
 * Floats are shown to the precision their type actually has: a REAL is a 32-bit float carried
 * through JSON as a double, and printing all seventeen digits would display noise as data.
 * Returns `null` when the value does not have the shape its type declares, so the caller shows
 * "no value" instead of a guess.
 */
export function formatValue(type: string, value: unknown): string | null {
  switch (type) {
    case 'BOOL':
      return typeof value === 'boolean' ? (value ? 'TRUE' : 'FALSE') : null;
    case 'REAL':
      return typeof value === 'number' && Number.isFinite(value)
        ? String(Number(value.toPrecision(7)))
        : null;
    case 'LREAL':
      return typeof value === 'number' && Number.isFinite(value)
        ? String(Number(value.toPrecision(15)))
        : null;
    case 'BYTE':
    case 'INT':
    case 'DINT':
    case 'LINT':
    case 'UINT':
    case 'UDINT':
    case 'ULINT':
      return typeof value === 'number' && Number.isInteger(value) ? String(value) : null;
    default:
      return null;
  }
}

export function viewTag(sample: TagSample | undefined, stream: StreamState): TagView {
  if (stream !== 'OPEN') return { kind: 'offline' };
  if (!sample) return { kind: 'no-data' };

  const text = formatValue(sample.type, sample.value);
  if (text === null) {
    // No value in the declared type. A GOOD quality with nothing readable behind it is not a
    // measurement either, so it is reported in the neutral tone rather than as healthy.
    const tone = isGood(sample) || sample.qualityName === 'GOOD' ? 'neutral' : toneFor(sample);
    return { kind: 'never-read', qualityLabel: qualityLabel(sample), tone };
  }

  if (isGood(sample)) return { kind: 'live', text };
  return { kind: 'stale', text, qualityLabel: qualityLabel(sample), tone: toneFor(sample) };
}

/** Local wall-clock time of a nanosecond timestamp, or `null` when there is none. */
export function formatTimestampNs(timestampNs: number): string | null {
  if (!Number.isFinite(timestampNs) || timestampNs <= 0) return null;
  return formatClock(timestampNs / 1_000_000);
}

export function formatClock(unixMs: number): string {
  const d = new Date(unixMs);
  const two = (n: number) => String(n).padStart(2, '0');
  return `${two(d.getHours())}:${two(d.getMinutes())}:${two(d.getSeconds())}.${String(
    d.getMilliseconds()
  ).padStart(3, '0')}`;
}
