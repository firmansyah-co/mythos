import type { StreamState, TagSample, TagView } from '../data/quality';
import { viewTag } from '../data/quality';

/** The words and tone for a tag's state. Exported so every surface says the same thing. */
export function badgeFor(view: TagView): { label: string; tone: string } {
  switch (view.kind) {
    case 'offline':
      return { label: 'OFFLINE', tone: 'neutral' };
    case 'no-data':
      return { label: 'NO DATA', tone: 'neutral' };
    case 'live':
      return { label: 'GOOD', tone: 'good' };
    case 'never-read':
    case 'stale':
      return { label: view.qualityLabel, tone: view.tone };
  }
}

export function QualityBadge({ view }: { view: TagView }) {
  const { label, tone } = badgeFor(view);
  return (
    <span className={`badge badge--${tone}`} data-testid="quality-badge">
      {label}
    </span>
  );
}

/**
 * A tag's value cell.
 *
 * Only a live value is rendered as a plain number. A stale one is visibly marked and says so
 * in words; everything else shows a dash, so an empty cell is never mistaken for a zero.
 */
export function TagValue({ sample, stream }: { sample: TagSample | undefined; stream: StreamState }) {
  const view = viewTag(sample, stream);
  if (view.kind === 'live') {
    return (
      <span className="tagvalue tagvalue--live mono" data-testid="tag-value" data-state="live">
        {view.text}
      </span>
    );
  }
  if (view.kind === 'stale') {
    return (
      <span
        className="tagvalue tagvalue--stale mono"
        data-testid="tag-value"
        data-state="stale"
        title="Last value read. It is no longer current."
      >
        {view.text}
        <span className="tagvalue__flag">stale</span>
      </span>
    );
  }
  return (
    <span
      className="tagvalue tagvalue--none mono"
      data-testid="tag-value"
      data-state={view.kind}
      aria-label="no value"
    >
      —
    </span>
  );
}
