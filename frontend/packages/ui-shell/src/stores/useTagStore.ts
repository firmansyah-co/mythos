/**
 * Live TagBus cache for the HMI canvas.
 *
 * SYS-REQ-096 (Zero Silent Fallback): a faceplate must never render a stale value as though it
 * were live, and must never render a fabricated one. This store is the only place the canvas
 * reads process values from, and it holds exactly what the most recent telemetry batch said.
 *
 * ## Why the snapshot is replaced, not merged
 *
 * The server enumerates the whole tag catalog on every 100ms tick (`build_telemetry_event`), and
 * emits an update only for tags whose slot it could read. A tag that is missing from a batch is
 * therefore a tag the server could not read — not a tag whose value is unchanged. Merging batches
 * would keep the last good value alive across that gap and present it as current, which is the
 * exact defect this store exists to prevent. So each batch *replaces* the cache.
 *
 * ## Why quality is mapped from `quality_name` / `quality_code`, never from `quality`
 *
 * The wire carries three quality fields:
 *
 *   quality       "GOOD" | "BAD" | "UNCERTAIN"   -- the class, lossy by design
 *   quality_code  u16                            -- the raw OPC word, authoritative
 *   quality_name  "GOOD" | "BAD_COMM_FAILURE" | "UNCERTAIN_SUBSTITUTE" | "UNRECOGNIZED" | ...
 *
 * The faceplate enum is `'GOOD' | 'BAD_QUALITY' | 'NOT_BOUND'`. Passing the class string through
 * as a quality declaration does not work and fails OPEN: `asQuality()` in `componentData.ts`
 * rejects "BAD" and "UNCERTAIN" (they are not in the enum), and `resolveComponentQuality` then
 * falls through to its final branch, which returns GOOD whenever the payload carries any value.
 * Measured against the resolver extracted verbatim from origin/dev:
 *
 *   quality: "BAD"               + value  ->  GOOD  {"actualSpeed": 1450}
 *   quality: "UNCERTAIN"         + value  ->  GOOD  {"actualSpeed": 1450}
 *   quality: "BAD_COMM_FAILURE"  + value  ->  GOOD  {"actualSpeed": 1450}
 *
 * The third line is why this module maps the *name* rather than special-casing the two class
 * strings: the correct, specific name is not in the enum either. A comm failure with a stale
 * value in the slot therefore renders as a live reading unless the mapping happens here.
 */

import { create } from 'zustand';
import type { ProcessDataQuality } from '@mythos/ui-faceplate-lib';
import type { CanvasComponentLike } from '@mythos/ui-faceplate-lib';

/** One tag's value as reported by the most recent telemetry batch. */
export interface LiveTagSample {
  /** TagBus URI, as it appears on the wire and in a component binding. */
  uri: string;
  /** The tag's true JSON value. `null` when the writer declared no usable type. */
  value: unknown;
  /** Declared type name from the wire: "BOOL", "REAL", …, or "UNSPECIFIED". */
  type: string;
  /** Raw OPC quality word — the authoritative quality source. */
  qualityCode: number;
  /** Specific quality state from the wire, e.g. "GOOD", "BAD_COMM_FAILURE". */
  qualityName: string;
  /** Faceplate quality this sample resolves to. Never inferred from the value's presence. */
  quality: ProcessDataQuality;
}

/** One `TagValueUpdate` as it appears on the `tag_update` wire event. */
export interface WireTagValueUpdate {
  uri: string;
  type?: string;
  value?: unknown;
  quality?: string;
  quality_code?: number;
  quality_name?: string;
  timestamp_ns?: number;
}

/** One `TelemetryBatch` as it appears on the `tag_update` wire event. */
export interface WireTelemetryBatch {
  timestamp_ms?: number;
  batch_window_ms?: number;
  updates?: WireTagValueUpdate[];
}

/** The stream's own connection state, as the canvas must report it. */
export type TelemetryConnectionState = 'IDLE' | 'CONNECTING' | 'OPEN' | 'CLOSED';

/** The named GOOD code (`QualityCode::GOOD` in `mythos-shm/src/slot.rs`). */
const QUALITY_CODE_GOOD = 0x00c0;

/** The wire reports this type when the slot carried no usable type code. */
const UNSPECIFIED_TYPE = 'UNSPECIFIED';

/**
 * Map a wire quality to the faceplate quality enum.
 *
 * Only `GOOD` is live. Every other state — a bad word, an uncertain word, a word this build does
 * not name, and an absent quality — resolves to `BAD_QUALITY`, which suppresses the process value
 * and announces the abnormal state. A value that cannot be proven good is not shown.
 *
 * `UNCERTAIN_*` deliberately maps to `BAD_QUALITY` rather than `GOOD`: an uncertain value is not
 * a measurement an operator may act on, and the faceplate enum has no uncertain member. The
 * consequence is that the badge reads `BAD COMM FAILURE` for an uncertain word, which is a
 * pre-existing limit of the three-value enum and is recorded rather than papered over here.
 */
export function mapWireQualityToFaceplateQuality(
  qualityName: string | undefined,
  qualityCode: number | undefined,
  typeName?: string
): ProcessDataQuality {
  // A slot whose writer declared no type carries no readable measurement, whatever its quality
  // word says. The wire documents this as "the writer did not say what this is".
  if (typeName === UNSPECIFIED_TYPE) return 'BAD_QUALITY';

  if (typeof qualityName === 'string' && qualityName.length > 0) {
    if (qualityName === 'GOOD') return 'GOOD';
    if (qualityName === 'UNRECOGNIZED') return 'BAD_QUALITY';
    // Named bad and uncertain words both suppress the value.
    if (qualityName.startsWith('BAD_') || qualityName.startsWith('UNCERTAIN_')) return 'BAD_QUALITY';
    // A name this build does not know is never promoted to GOOD.
    return 'BAD_QUALITY';
  }

  // No name on the wire: the frame is malformed (the server always serialises `quality_name`).
  // Fall back to the raw word, but accept ONLY the one named GOOD code. Testing the class bits
  // instead would be a fail-open of its own: `0x7fff & 0x00c0 === 0x00c0`, so a word whose class
  // bits happen to read good would pass even though this build does not name it — and the
  // repository's own rule is that an unnamed word is "UNRECOGNIZED", never a good value.
  if (typeof qualityCode === 'number') {
    return qualityCode === QUALITY_CODE_GOOD ? 'GOOD' : 'BAD_QUALITY';
  }

  return 'BAD_QUALITY';
}

/** Turn one wire update into a cache sample, or `null` when it names no tag. */
function toSample(update: WireTagValueUpdate): LiveTagSample | null {
  if (!update || typeof update.uri !== 'string' || update.uri.length === 0) return null;
  const type = typeof update.type === 'string' ? update.type : UNSPECIFIED_TYPE;
  return {
    uri: update.uri,
    value: update.value ?? null,
    type,
    qualityCode: typeof update.quality_code === 'number' ? update.quality_code : -1,
    qualityName: typeof update.quality_name === 'string' ? update.quality_name : '',
    quality: mapWireQualityToFaceplateQuality(update.quality_name, update.quality_code, type),
  };
}

export interface TagStoreState {
  /** Tag samples from the most recent batch, keyed by TagBus URI. */
  tags: Map<string, LiveTagSample>;
  /** Connection state reported by the SSE consumer. */
  connection: TelemetryConnectionState;
  /** `timestamp_ms` of the most recent batch, or `null` before the first one. */
  lastBatchAtMs: number | null;

  /**
   * Replace the cache with one batch. Absent tags are dropped, not retained: see the module note
   * on why the snapshot is replaced rather than merged.
   */
  applyBatch: (batch: WireTelemetryBatch) => void;
  setConnection: (connection: TelemetryConnectionState) => void;
  /**
   * Drop every sample. Called when the stream closes, so a bound faceplate degrades to
   * `BAD_QUALITY` instead of continuing to render the last frame as though it were current.
   */
  clear: () => void;
}

export const useTagStore = create<TagStoreState>((set) => ({
  tags: new Map<string, LiveTagSample>(),
  connection: 'IDLE',
  lastBatchAtMs: null,

  applyBatch: (batch) => {
    const updates = Array.isArray(batch?.updates) ? batch.updates : [];
    const next = new Map<string, LiveTagSample>();
    for (const update of updates) {
      const sample = toSample(update);
      if (sample) next.set(sample.uri, sample);
    }
    set({
      tags: next,
      lastBatchAtMs: typeof batch?.timestamp_ms === 'number' ? batch.timestamp_ms : null,
    });
  },

  setConnection: (connection) => set({ connection }),

  clear: () => set({ tags: new Map<string, LiveTagSample>(), lastBatchAtMs: null }),
}));

/**
 * The tags a canvas component is bound to, as `[faceplateKey, tagUri]` pairs.
 *
 * A binding is `{ property, tag }`: `property` is the faceplate key the value feeds, `tag` is the
 * TagBus URI. Anything malformed is ignored rather than guessed at — a binding this module cannot
 * read is not a binding, and the component then resolves as unbound or bad-quality on its own.
 */
export function readComponentBindings(component: CanvasComponentLike | null | undefined): Array<[string, string]> {
  const raw =
    (component?.bindings as unknown) ??
    (component?.binding as unknown) ??
    ((component?.props as Record<string, unknown> | undefined)?.bindings as unknown) ??
    ((component?.props as Record<string, unknown> | undefined)?.binding as unknown);

  const list = Array.isArray(raw) ? raw : raw ? [raw] : [];
  const pairs: Array<[string, string]> = [];
  for (const entry of list) {
    if (!entry || typeof entry !== 'object') continue;
    const property = (entry as { property?: unknown }).property;
    const tag = (entry as { tag?: unknown }).tag;
    if (typeof property === 'string' && property.length > 0 && typeof tag === 'string' && tag.length > 0) {
      pairs.push([property, tag]);
    }
  }
  return pairs;
}

/** What a component's bindings resolve to against the current cache. */
export interface ComponentLiveResolution {
  /**
   * The declared quality for this component. Always present when the component is bound, so the
   * resolver never has to infer quality from whether a value happens to be present.
   */
  quality: ProcessDataQuality;
  /** Live process values, keyed by faceplate property. Empty unless every binding is GOOD. */
  live: Record<string, unknown>;
}

/**
 * Resolve a canvas component's bindings against the cache.
 *
 * Returns `null` when this store knows nothing about the component — no readable binding, or no
 * sample for any of its tags. `null` means "leave the component exactly as authored", which keeps
 * `resolveComponentData`'s existing contract intact: a component that supplies its own live
 * payload still renders it, and an unbound component still resolves to `NOT_BOUND`.
 *
 * When the store does hold samples, it is the live source and its quality is authoritative. The
 * component is `GOOD` only if **every** binding resolves to a GOOD sample. One bad or missing
 * binding is enough to suppress the whole faceplate: a faceplate showing a live speed beside a
 * dead current is more misleading than one showing neither, because nothing on screen
 * distinguishes the two.
 *
 * The asymmetry is deliberate. Partial knowledge of a component is a reason to say less, not to
 * fall back to whatever else is lying around.
 */
export function resolveComponentLive(
  component: CanvasComponentLike | null | undefined,
  tags: Map<string, LiveTagSample>
): ComponentLiveResolution | null {
  const bindings = readComponentBindings(component);
  if (bindings.length === 0) return null;

  // No binding of this component has ever been reported: this store has nothing to say about it.
  if (!bindings.some(([, uri]) => tags.has(uri))) return null;

  const live: Record<string, unknown> = {};
  let allGood = true;

  for (const [property, uri] of bindings) {
    const sample = tags.get(uri);
    if (!sample || sample.quality !== 'GOOD') {
      allGood = false;
      continue;
    }
    live[property] = sample.value;
  }

  return { quality: allGood ? 'GOOD' : 'BAD_QUALITY', live: allGood ? live : {} };
}
