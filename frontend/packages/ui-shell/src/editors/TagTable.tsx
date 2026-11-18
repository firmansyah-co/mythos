import { useMemo, useState } from 'react';
import { Search } from 'lucide-react';
import type { IoStatus } from '../api/client';
import { useController } from '../data/controller';
import { allTagUris, connectionOfTag } from '../data/derive';
import { formatTimestampNs, viewTag } from '../data/quality';
import { useTelemetry } from '../data/telemetry';
import { useWorkbench } from '../data/workbench';
import { ForcedMark, QualityBadge, TagValue } from '../shell/QualityBadge';
import { RuntimeGate } from '../shell/RuntimeGate';

/**
 * The tags a connection (or every connection) provides, with their live values when online.
 *
 * The rows come from the controller's connection status, not from the telemetry stream: a tag
 * the stream stops reporting must stay on screen and say NO DATA, not disappear.
 */
export function TagTable({ connectionId }: { connectionId?: string }) {
  return <RuntimeGate>{(io) => <Rows io={io} connectionId={connectionId} />}</RuntimeGate>;
}

function Rows({ io, connectionId }: { io: IoStatus; connectionId?: string }) {
  const tagMeta = useController((state) => state.tagMeta);
  const forcedTags = useController((state) => state.forcedTags);
  const tags = useTelemetry((state) => state.tags);
  const stream = useTelemetry((state) => state.stream);
  const selection = useWorkbench((state) => state.selection);
  const select = useWorkbench((state) => state.select);
  const [filter, setFilter] = useState('');

  const uris = useMemo(() => allTagUris(io, connectionId), [io, connectionId]);
  const needle = filter.trim().toLowerCase();
  const shown = needle ? uris.filter((uri) => uri.toLowerCase().includes(needle)) : uris;

  return (
    <div className="editor__column">
      <div className="editor__tools">
        <label className="search">
          <Search size={13} aria-hidden />
          <input
            type="search"
            placeholder="Filter tags"
            aria-label="Filter tags"
            value={filter}
            onChange={(event) => setFilter(event.target.value)}
          />
        </label>
        <span className="muted">
          {shown.length} of {uris.length} tags
        </span>
        {stream !== 'OPEN' && (
          <span className="badge badge--neutral" data-testid="offline-hint">
            OFFLINE — values are not being read
          </span>
        )}
      </div>

      {uris.length === 0 ? (
        <div className="notice">
          <div>
            <h3>No tags</h3>
            <p>
              {connectionId
                ? 'This connection provides no tags.'
                : (io.note ?? 'No running connection provides a tag.')}
            </p>
          </div>
        </div>
      ) : (
        <div className="editor__scroll">
          <table className="grid" aria-label="Tags">
            <thead>
              <tr>
                <th>Tag</th>
                <th className="num">Value</th>
                <th>Data type</th>
                <th>Quality</th>
                <th>Last read</th>
                {!connectionId && <th>Connection</th>}
                <th>Source</th>
              </tr>
            </thead>
            <tbody>
              {shown.map((uri) => {
                const sample = tags.get(uri);
                const view = viewTag(sample, stream);
                const hasType =
                  stream === 'OPEN' && sample !== undefined && view.kind !== 'never-read';
                const lastRead =
                  stream === 'OPEN' && sample ? formatTimestampNs(sample.timestampNs) : null;
                return (
                  <tr
                    key={uri}
                    data-testid="tag-row"
                    data-uri={uri}
                    aria-selected={selection?.kind === 'tag' && selection.uri === uri}
                    onClick={() => select({ kind: 'tag', uri })}
                  >
                    <td className="mono" title={uri}>
                      {uri}
                    </td>
                    <td className="num">
                      <TagValue sample={sample} stream={stream} />
                      <ForcedMark uri={uri} forcedTags={forcedTags} />
                    </td>
                    <td className="mono">{hasType ? sample.type : '—'}</td>
                    <td>
                      <QualityBadge view={view} />
                    </td>
                    <td className="mono">{lastRead ?? '—'}</td>
                    {!connectionId && <td>{connectionOfTag(io, uri)?.id ?? '—'}</td>}
                    <td className="grow" title={tagMeta.get(uri)?.description ?? ''}>
                      {tagMeta.get(uri)?.description || '—'}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
