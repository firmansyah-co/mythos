/**
 * Draft-vs-committed reconciliation for operator-writable setpoints.
 *
 * Implements `docs/04-hmi-and-ui/operator-setpoint-reconcile-rule.md` (DECIDED 2026-11-11).
 *
 * ## The defect this replaces
 *
 * Every operator-writable field used to be seeded once at mount and never reconciled:
 *
 *     const [inputSpeed, setInputSpeed] = useState<number>(speedSetpoint ?? 0);
 *
 * That is wrong in two directions at once:
 *
 *  1. **Stale-as-current.** Telemetry arriving after mount never reached the field, so it kept
 *     showing the mount-time value as though it were the device's. Before S-004 this was latent
 *     because `speedSetpoint` was never live; S-004 made it live, so it is a live defect.
 *  2. **Fabrication.** `?? 0` invents a setpoint when the device reports none. An operator cannot
 *     tell an invented 0 from a measured 0.
 *
 * The fix for (1) — read the prop every render — is exactly wrong for the second defect the rule
 * names: it would overwrite an edit the operator is in the middle of making. So the two values are
 * kept apart rather than merged.
 *
 * ## The rule, in one paragraph
 *
 * `committed` is read from props on every render and is never held in state. A `draft` exists only
 * inside an episode, and an episode opens on the operator's first input (R3), never on focus. A
 * frame never overwrites an open draft (R5), and an open draft is always shown as uncommitted
 * beside the committed value so the operator can see the device move underneath the edit (R6).
 * Leaving GOOD quality ends every episode and discards the draft with a visible reason (R7).
 * A component re-key discards it with a visible notice (O6). There is no echo path (O2).
 *
 * ## What this hook deliberately does not do
 *
 * It does not send commands, does not know about any transport, and does not decide whether a field
 * is writable. It owns only the two-provenance bookkeeping.
 */

import { useCallback, useEffect, useState } from 'react';

/** Why an episode closed without being committed. Shown to the operator; never silent. */
export type DraftDiscardReason = 'QUALITY_LOST' | 'IDENTITY_CHANGED' | 'CANCELLED';

/** The operator-facing wording for each discard reason. */
export const DRAFT_DISCARD_NOTICE: Record<DraftDiscardReason, string> = {
  // R7: the control is disabled when the device is not reporting, so a retained draft would be
  // hidden state behind a disabled control.
  QUALITY_LOST: 'Draft discarded: the device stopped reporting',
  // O6: a re-key changes which device the surface shows. Carrying the draft would apply it to a
  // different device than the one it was typed against.
  IDENTITY_CHANGED: 'Draft discarded when the faceplate changed device',
  // R4: an explicit abandon the operator performed.
  CANCELLED: 'Draft discarded',
};

export interface UseOperatorDraftOptions {
  /** The committed value, read from props every render. `null` means the device reports none. */
  committed: number | null;
  /** True only when the quality is GOOD. A non-live device cannot be edited. */
  isLive: boolean;
  /**
   * Anything that identifies WHICH device this surface is showing — typically the component id.
   * A change discards an open draft with a visible notice (O6). Must be stable across re-renders
   * of the same device.
   */
  identity: string;
  /**
   * How far the committed value must move from the episode's basis before the divergence is
   * announced. REQUIRED configuration with NO DEFAULT (O4): a plausible-looking bound here would
   * be a fabricated number, and an operator cannot tell a chosen bound from a measured one.
   * `null` means unconfigured — drift detection is inactive and the surface says so.
   */
  driftThreshold: number | null;
}

export interface OperatorDraft {
  /** The committed value from the device. `null` = not reported. Never seeded, never invented. */
  committed: number | null;
  /** The open draft. `null` when no episode is open, and also a legitimate cleared field. */
  draft: number | null;
  /** True while an episode is open. Tracked explicitly: a `null` draft is a valid operator input. */
  isDraftOpen: boolean;
  /**
   * What the control displays: the draft if one is open, otherwise the committed value.
   * `null` when neither exists — the caller renders its own no-data/label treatment.
   */
  value: number | null;
  /**
   * True when the committed value has moved from the episode's basis by more than the configured
   * threshold. Always `false` when no threshold is configured.
   */
  diverged: boolean;
  /** The visible reason the last episode closed without committing, or `null`. */
  notice: string | null;
  /** R3: opens an episode on the operator's first input. No-op while one is already open. */
  begin: (initial: number | null) => void;
  /** Records the operator's current input. Opens an episode if none is open (R3). */
  update: (next: number | null) => void;
  /** R4: the operator committed — close the draft. */
  commit: () => void;
  /** R4: explicit abandon (Escape, cancel) — discard with a visible reason. */
  cancel: () => void;
  /** Clears the notice once the operator has seen it. */
  dismissNotice: () => void;
}

interface Episode {
  /** The committed value when the episode opened, used only to describe drift. */
  basis: number | null;
  /** The device identity the draft was typed against. */
  identity: string;
}

/**
 * The threshold is unconfigured when it is absent, null, or not a usable number. Drift detection is
 * inactive in that case rather than falling back to a default (O4).
 */
export function isDriftConfigured(threshold: number | null | undefined): threshold is number {
  return typeof threshold === 'number' && Number.isFinite(threshold) && threshold >= 0;
}

/** True when the committed value has moved from the basis by more than the threshold. */
export function hasDrifted(
  basis: number | null,
  committed: number | null,
  threshold: number | null | undefined
): boolean {
  if (!isDriftConfigured(threshold)) return false; // unconfigured: inactive, never a guess
  if (basis === null || committed === null) return false; // nothing comparable to drift against
  return Math.abs(committed - basis) > threshold;
}

export function useOperatorDraft({
  committed,
  isLive,
  identity,
  driftThreshold,
}: UseOperatorDraftOptions): OperatorDraft {
  // R1/R2: the draft is state; the committed value is not. They are never merged into one piece of
  // state, because merging is how a draft becomes indistinguishable from a measurement.
  const [draft, setDraft] = useState<number | null>(null);
  const [episode, setEpisode] = useState<Episode | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const closeWithNotice = useCallback((reason: DraftDiscardReason) => {
    setEpisode(null);
    setDraft(null);
    setNotice(DRAFT_DISCARD_NOTICE[reason]);
  }, []);

  // R7: leaving GOOD ends every episode and discards the draft, with the reason shown. The control
  // is disabled in that state, so a retained draft would be hidden state behind a disabled control.
  useEffect(() => {
    if (!isLive && episode !== null) closeWithNotice('QUALITY_LOST');
  }, [isLive, episode, closeWithNotice]);

  // O6: a re-key changes which device this surface shows. Discard, and say so — a silent discard is
  // the exact failure the rule exists to prevent.
  useEffect(() => {
    if (episode !== null && episode.identity !== identity) closeWithNotice('IDENTITY_CHANGED');
  }, [identity, episode, closeWithNotice]);

  const begin = useCallback(
    (initial: number | null) => {
      if (episode !== null) return; // an episode is already open
      setNotice(null);
      setEpisode({ basis: committed, identity });
      setDraft(initial);
    },
    [episode, committed, identity]
  );

  const update = useCallback(
    (next: number | null) => {
      // R3: the first input opens the episode, not focus.
      if (episode === null) {
        setNotice(null);
        setEpisode({ basis: committed, identity });
      }
      setDraft(next);
    },
    [episode, committed, identity]
  );

  // R4: an explicit boundary. Committing closes the episode; the value the caller sends is the
  // operator's, and the field returns to reporting telemetry immediately.
  const commit = useCallback(() => {
    setEpisode(null);
    setDraft(null);
  }, []);

  const cancel = useCallback(() => {
    if (episode === null) return;
    closeWithNotice('CANCELLED');
  }, [episode, closeWithNotice]);

  const dismissNotice = useCallback(() => setNotice(null), []);

  const isDraftOpen = episode !== null;

  return {
    committed,
    draft,
    isDraftOpen,
    // R2: with no episode open the control shows the committed value, or nothing — never a copy of
    // a value taken at some earlier moment.
    value: isDraftOpen ? draft : committed,
    diverged: isDraftOpen && hasDrifted(episode?.basis ?? null, committed, driftThreshold),
    notice,
    begin,
    update,
    commit,
    cancel,
    dismissNotice,
  };
}

/**
 * The label for a committed value the device is not reporting.
 *
 * O5: a tuning parameter the device may never report is shown as `UNKNOWN`, never seeded at mount
 * and never presented as though the device had confirmed it.
 */
export const COMMITTED_UNKNOWN_TEXT = 'UNKNOWN';

/** Shown when no drift threshold is configured (O4). */
export const DRIFT_NOT_CONFIGURED_TEXT = 'NOT CONFIGURED';

/**
 * Format the committed value shown beside an open draft.
 *
 * Distinguishes "the device reports a value" from "the device does not report this field", which
 * O5 requires and which a plain `?? 0` erases.
 */
export function formatCommitted(
  committed: number | null,
  format: (value: number) => string
): string {
  return committed === null ? COMMITTED_UNKNOWN_TEXT : format(committed);
}

/**
 * The tuning trio as ONE episode (R8): three fields, one Apply, one `onTuneUpdate`.
 *
 * R8 is explicit that `tuneKp`/`tuneTn`/`tuneTv` are a single episode — drafts open and close
 * together — while `targetSp`, `targetCv` and `inputSpeed` are each their own. Using three
 * single-value hooks for the trio would silently split it into three episodes and let one field be
 * closed and sent while its siblings stayed open, which is not the transaction the operator
 * performed.
 *
 * Same rules apply: no mount-time seed, a frame never overwrites an open draft, leaving GOOD or a
 * change of device discards with a visible reason, and there is no echo path.
 */
export type TuningValues = {
  kp: number | null;
  tn: number | null;
  tv: number | null;
};

export interface UseTuningDraftOptions {
  committed: TuningValues;
  isLive: boolean;
  identity: string;
  driftThreshold: number | null;
}

export interface TuningDraft {
  committed: TuningValues;
  draft: TuningValues | null;
  isDraftOpen: boolean;
  /** What the controls display: the draft while open, otherwise the committed values. */
  value: TuningValues;
  diverged: boolean;
  notice: string | null;
  begin: (initial: TuningValues) => void;
  update: (next: TuningValues) => void;
  commit: () => void;
  cancel: () => void;
  dismissNotice: () => void;
}

/** True when ANY of the three has moved from its basis by more than the threshold. */
function tuningHasDrifted(
  basis: TuningValues | null,
  committed: TuningValues,
  threshold: number | null | undefined
): boolean {
  if (!basis || !isDriftConfigured(threshold)) return false;
  return (['kp', 'tn', 'tv'] as const).some((key) => hasDrifted(basis[key], committed[key], threshold));
}

export function useTuningDraft({
  committed,
  isLive,
  identity,
  driftThreshold,
}: UseTuningDraftOptions): TuningDraft {
  const [draft, setDraft] = useState<TuningValues | null>(null);
  const [episode, setEpisode] = useState<{ basis: TuningValues; identity: string } | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const closeWithNotice = useCallback((reason: DraftDiscardReason) => {
    setEpisode(null);
    setDraft(null);
    setNotice(DRAFT_DISCARD_NOTICE[reason]);
  }, []);

  // R7
  useEffect(() => {
    if (!isLive && episode !== null) closeWithNotice('QUALITY_LOST');
  }, [isLive, episode, closeWithNotice]);

  // O6
  useEffect(() => {
    if (episode !== null && episode.identity !== identity) closeWithNotice('IDENTITY_CHANGED');
  }, [identity, episode, closeWithNotice]);

  const begin = useCallback(
    (initial: TuningValues) => {
      if (episode !== null) return;
      setNotice(null);
      setEpisode({ basis: committed, identity });
      setDraft(initial);
    },
    [episode, committed, identity]
  );

  const update = useCallback(
    (next: TuningValues) => {
      if (episode === null) {
        setNotice(null);
        setEpisode({ basis: committed, identity });
      }
      setDraft(next);
    },
    [episode, committed, identity]
  );

  const commit = useCallback(() => {
    setEpisode(null);
    setDraft(null);
  }, []);

  const cancel = useCallback(() => {
    if (episode === null) return;
    closeWithNotice('CANCELLED');
  }, [episode, closeWithNotice]);

  const dismissNotice = useCallback(() => setNotice(null), []);
  const isDraftOpen = episode !== null;

  return {
    committed,
    draft,
    isDraftOpen,
    value: isDraftOpen && draft ? draft : committed,
    diverged: isDraftOpen && tuningHasDrifted(episode?.basis ?? null, committed, driftThreshold),
    notice,
    begin,
    update,
    commit,
    cancel,
    dismissNotice,
  };
}
