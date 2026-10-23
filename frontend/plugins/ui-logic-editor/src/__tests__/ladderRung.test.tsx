import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import React from 'react';
import { createRoot, Root } from 'react-dom/client';
import { flushSync } from 'react-dom';
import { RungRenderer } from '../ladder/RungRenderer';
import {
  ContactElementView,
  CoilElementView,
  FunctionBlockElementView,
} from '../ladder/LadderElements';
import { DEFAULT_LD_RUNGS } from '../LadderCanvas';
import { LadderRung } from '../ladder/types';

let container: HTMLDivElement;
let root: Root;

function renderClient(element: React.ReactElement): string {
  flushSync(() => {
    root.render(element);
  });
  return container.innerHTML;
}

describe('TSK-702: Ladder Rung & Elements Unit Suite', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    container = document.createElement('div');
    document.body.appendChild(container);
    root = createRoot(container);
  });

  afterEach(() => {
    flushSync(() => {
      root.unmount();
    });
    container.remove();
  });

  describe('1. Contact Elements (NO, NC, P, N)', () => {
    it('renders Normally Open (NO: ──┤ ├───) contact', () => {
      const html = renderClient(
        <ContactElementView
          element={{ id: 'c1', type: 'contact_no', tag: 'DI_StartPB', isEnergized: false }}
          readOnly
        />
      );
      expect(html).toContain('──┤ ├───');
      expect(html).toContain('DI_StartPB');
      expect(html).toContain('N.O. Contact');
    });

    it('renders Normally Closed (NC: ──┤/├───) contact', () => {
      const html = renderClient(
        <ContactElementView
          element={{ id: 'c2', type: 'contact_nc', tag: 'DI_EStop', isEnergized: true }}
          readOnly
        />
      );
      expect(html).toContain('──┤/├───');
      expect(html).toContain('DI_EStop');
      expect(html).toContain('N.C. Contact');
    });

    it('renders Positive Transition Sensing (P: ──┤P├───) contact', () => {
      const html = renderClient(
        <ContactElementView
          element={{ id: 'c3', type: 'contact_p', tag: 'DI_EdgeSensor', isEnergized: false }}
          readOnly
        />
      );
      expect(html).toContain('──┤P├───');
      expect(html).toContain('Rising Edge (P)');
    });

    it('renders Negative Transition Sensing (N: ──┤N├───) contact', () => {
      const html = renderClient(
        <ContactElementView
          element={{ id: 'c4', type: 'contact_n', tag: 'DI_FallingEdge', isEnergized: false }}
          readOnly
        />
      );
      expect(html).toContain('──┤N├───');
      expect(html).toContain('Falling Edge (N)');
    });
  });

  describe('2. Coil Elements (Normal, Negated, Set, Reset)', () => {
    it('renders Normal Output Coil ( )', () => {
      const html = renderClient(
        <CoilElementView
          element={{ id: 'q1', type: 'coil', tag: 'DO_MotorRun', isEnergized: false }}
          readOnly
        />
      );
      expect(html).toContain('( )');
      expect(html).toContain('Normal Coil');
      expect(html).toContain('DO_MotorRun');
    });

    it('renders Set Coil (S) latch', () => {
      const html = renderClient(
        <CoilElementView
          element={{ id: 'q2', type: 'coil_set', tag: 'B_MachineActive', isEnergized: true }}
          readOnly
        />
      );
      expect(html).toContain('(S)');
      expect(html).toContain('Set Coil (Latch)');
    });

    it('renders Reset Coil (R) unlatch', () => {
      const html = renderClient(
        <CoilElementView
          element={{ id: 'q3', type: 'coil_reset', tag: 'B_MachineActive', isEnergized: false }}
          readOnly
        />
      );
      expect(html).toContain('(R)');
      expect(html).toContain('Reset Coil (Unlatch)');
    });

    it('renders Negated Coil (/)', () => {
      const html = renderClient(
        <CoilElementView
          element={{ id: 'q4', type: 'coil_negated', tag: 'DO_InvertedOutput', isEnergized: false }}
          readOnly
        />
      );
      expect(html).toContain('(/)');
      expect(html).toContain('Negated Coil');
    });
  });

  describe('3. Function Blocks and Branches in RungRenderer', () => {
    it('renders TON function block with inputs and outputs', () => {
      const html = renderClient(
        <FunctionBlockElementView
          element={{
            id: 'fb1',
            type: 'function_block',
            blockType: 'TON',
            instanceName: 'TON_BottleDwell',
            inputs: [
              { name: 'IN', type: 'BOOL', binding: 'DI_BottlePE' },
              { name: 'PT', type: 'TIME', binding: 'T#500MS' },
            ],
            outputs: [
              { name: 'Q', type: 'BOOL', binding: 'Gate_Open' },
              { name: 'ET', type: 'TIME', binding: 'Elapsed' },
            ],
          }}
          readOnly
        />
      );
      expect(html).toContain('TON');
      expect(html).toContain('TON_BottleDwell');
      expect(html).toContain('IN');
      expect(html).toContain('PT');
      expect(html).toContain('Q');
      expect(html).toContain('ET');
    });

    it('renders RungRenderer with quick insertion toolbar', () => {
      renderClient(
        <RungRenderer
          rung={DEFAULT_LD_RUNGS[0]}
          index={0}
          onUpdateRung={() => {}}
          onDeleteRung={() => {}}
          onAddContact={() => {}}
          onAddCoil={() => {}}
          onAddBranch={() => {}}
          onAddBlock={() => {}}
        />
      );
      expect(container.innerHTML).toContain('+ ┤ ├ (NO)');
      expect(container.innerHTML).toContain('+ ┤/├ (NC)');
      expect(container.innerHTML).toContain('+ ┤P├ (Edge)');
      expect(container.innerHTML).toContain('OR Branch');
      expect(container.innerHTML).toContain('+ TON');
      expect(container.innerHTML).toContain('+ ( )');
      expect(container.innerHTML).toContain('+ (S)');
      expect(container.innerHTML).toContain('+ (R)');
    });
  });
});
