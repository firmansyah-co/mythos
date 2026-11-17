// The HMI UI under test runs against the workbench's real host (`createHost`), so what is
// proven here is the module as the workbench activates it, not against a stand-in. The
// workbench's own stores are set the way the workbench sets them; the module sees them only
// through the host.

import { activate, MODULE_ID } from '../entry';
import { createHost } from '../../../ui-shell/src/modules/host';
import { resetStores as resetShell } from '../../../ui-shell/src/__tests__/harness';

export {
  connection,
  ioStatus,
  loadedIoConfig,
  modbusConnection,
  project,
  render,
  sample,
  typeInto,
  unmountAll,
} from '../../../ui-shell/src/__tests__/harness';
export { useController } from '../../../ui-shell/src/data/controller';
export { useProgramTags } from '../../../ui-shell/src/data/programTags';
export { goOffline, useTelemetry } from '../../../ui-shell/src/data/telemetry';
export { useWorkbench as useShell } from '../../../ui-shell/src/data/workbench';
export { Inspector } from '../../../ui-shell/src/shell/Inspector';

let deactivate: (() => void) | null = null;

/** Every store as the workbench starts, and the HMI UI activated against it afresh. */
export function resetStores(): void {
  deactivate?.();
  resetShell();
  deactivate = activate(createHost(MODULE_ID));
}
