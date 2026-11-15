// The I/O field: one tag's value on a screen, in the editor and the operator runtime alike.

import { viewTag, type QualityTone } from '../data/quality';
import { useProjectIo } from '../data/scope';
import type { IoFieldElement } from '../data/screenModel';
import { useTelemetry } from '../data/telemetry';
import { TagValue } from '../shell/QualityBadge';

/**
 * An I/O field as it is drawn. The value obeys the same rule as everywhere else: shown as
 * current only when GOOD and online. What the field is bound to is always legible, so a field
 * with no value still says which tag it is waiting for.
 */
export function IoField({ element }: { element: IoFieldElement }) {
  const io = useProjectIo();
  const sample = useTelemetry((state) => (element.tag ? state.tags.get(element.tag) : undefined));
  const stream = useTelemetry((state) => state.stream);

  if (element.tag === '') {
    return (
      <div className="iofield iofield--unbound" data-testid="io-field" data-state="not-bound">
        {element.label && <span className="iofield__label">{element.label}</span>}
        <span className="iofield__state">NOT BOUND</span>
      </div>
    );
  }

  // Values exist only for the project the controller runs. For any other project the field is
  // offline whatever the stream holds, because the stream is another project's.
  const view = viewTag(sample, io ? stream : 'IDLE');
  const tone: QualityTone | 'offline' =
    view.kind === 'live'
      ? 'good'
      : view.kind === 'stale' || view.kind === 'never-read'
        ? view.tone
        : 'offline';
  const word =
    view.kind === 'stale' || view.kind === 'never-read'
      ? view.qualityLabel
      : view.kind === 'no-data'
        ? 'NO DATA'
        : view.kind === 'offline'
          ? 'OFFLINE'
          : null;

  return (
    <div className={`iofield iofield--${tone}`} data-testid="io-field" data-state={view.kind}>
      <span className="iofield__label" title={element.tag}>
        {element.label || element.tag}
      </span>
      <span className="iofield__value">
        <TagValue sample={sample} stream={io ? stream : 'IDLE'} />
        {element.unit && view.kind === 'live' && <span className="iofield__unit">{element.unit}</span>}
      </span>
      {word && <span className="iofield__state">{word}</span>}
    </div>
  );
}
