// The module's reasons why the project's stored types and data blocks do not check, as
// `GET /blocks` answered them: shown where a layout is said to be NOT KNOWN, so the reason is the
// module's own and not only "does not check".

import { whereOf } from './model';
import { useData } from './store';

export function StateReasons() {
  const blocksOk = useData((state) => state.blocksOk);
  const diagnostics = useData((state) => state.blocksDiagnostics);
  if (blocksOk !== false) return null;
  if (diagnostics.length === 0) {
    return (
      <p className="muted editor__footnote" data-testid="state-reasons-none">
        The data module gave no reason with its answer.
      </p>
    );
  }
  return (
    <ul className="data-diagnostics" aria-label="Why the project's types and data blocks do not check">
      {diagnostics.map((d, index) => (
        <li key={index} data-testid="state-reason">
          <span className="mono">{whereOf(d)}</span> {d.message}
        </li>
      ))}
    </ul>
  );
}
