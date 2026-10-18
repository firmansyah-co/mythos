import { describe, it, expect } from 'vitest';
import { NonActuatingPreviewBanner } from '../components/banner/NonActuatingPreviewBanner';
import { ExitKioskDialog } from '../components/kiosk/ExitKioskDialog';
import { Zone1TopNav } from '../components/workbench/Zone1TopNav';
import { Zone2ProjectTree } from '../components/workbench/Zone2ProjectTree';
import { Zone4TaskCards } from '../components/workbench/Zone4TaskCards';
import { Zone5BottomConsole } from '../components/workbench/Zone5BottomConsole';
import { DockviewWorkspace } from '../components/workbench/DockviewWorkspace';
import { App } from '../App';

describe('Workbench Shell 5-Zone Components (SYS-REQ-097 Parity)', () => {
  it('exports all 5 zones and dialog primitives as valid React components', () => {
    expect(typeof NonActuatingPreviewBanner).toBe('function');
    expect(typeof ExitKioskDialog).toBe('function');
    expect(typeof Zone1TopNav).toBe('function');
    expect(typeof Zone2ProjectTree).toBe('function');
    expect(typeof DockviewWorkspace).toBe('function');
    expect(typeof Zone4TaskCards).toBe('function');
    expect(typeof Zone5BottomConsole).toBe('function');
    expect(typeof App).toBe('function');
  });
});
