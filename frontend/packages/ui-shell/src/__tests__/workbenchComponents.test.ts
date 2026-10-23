import { describe, it, expect } from 'vitest';
import { NonActuatingPreviewBanner } from '../components/banner/NonActuatingPreviewBanner';
import { ExitKioskDialog } from '../components/kiosk/ExitKioskDialog';
import { Zone1TopNav } from '../components/workbench/Zone1TopNav';
import { Zone2ProjectTree } from '../components/workbench/Zone2ProjectTree';
import { Zone4TaskCards } from '../components/workbench/Zone4TaskCards';
import { Zone5BottomConsole } from '../components/workbench/Zone5BottomConsole';
import { DockviewWorkspace } from '../components/workbench/DockviewWorkspace';
import { App } from '../App';

import { PortalView } from '../components/portal/PortalView';
import { CreateBlockModal } from '../components/modals/CreateBlockModal';
import { CreateScreenModal } from '../components/modals/CreateScreenModal';

describe('Workbench Shell 5-Zone Components (SYS-REQ-097 & SYS-REQ-098 Parity)', () => {
  it('exports all 5 zones, modals, portal view, and dialog primitives as valid React components', () => {
    const isValidComponent = (comp: any) => typeof comp === 'function' || (typeof comp === 'object' && comp !== null);

    expect(isValidComponent(NonActuatingPreviewBanner)).toBe(true);
    expect(isValidComponent(ExitKioskDialog)).toBe(true);
    expect(isValidComponent(Zone1TopNav)).toBe(true);
    expect(isValidComponent(Zone2ProjectTree)).toBe(true);
    expect(isValidComponent(DockviewWorkspace)).toBe(true);
    expect(isValidComponent(Zone4TaskCards)).toBe(true);
    expect(isValidComponent(Zone5BottomConsole)).toBe(true);
    expect(isValidComponent(PortalView)).toBe(true);
    expect(isValidComponent(CreateBlockModal)).toBe(true);
    expect(isValidComponent(CreateScreenModal)).toBe(true);
    expect(isValidComponent(App)).toBe(true);
  });
});
