import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import React from 'react';
import { createRoot, Root } from 'react-dom/client';
import { flushSync } from 'react-dom';
import * as Y from 'yjs';
import { LogicEditorPlugin } from '../LogicEditorPlugin';
import { sharedDebugEngine } from '../debug/OnlineDebugEngine';

let container: HTMLDivElement;
let root: Root;

function renderClient(element: React.ReactElement): string {
  flushSync(() => {
    root.render(element);
  });
  return container.innerHTML;
}

describe('TSK-701..705: LogicEditorPlugin Comprehensive Integration Suite', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    sharedDebugEngine.disconnect();
    container = document.createElement('div');
    document.body.appendChild(container);
    root = createRoot(container);
  });

  afterEach(() => {
    sharedDebugEngine.disconnect();
    flushSync(() => {
      root.unmount();
    });
    container.remove();
  });

  describe('1. Universal Tab Navigation across all IEC 61131-3 Editors', () => {
    it('renders all language tabs and initial ST editor', () => {
      const html = renderClient(<LogicEditorPlugin targetBlock="MainTask" />);
      expect(html).toContain('Structured Text (ST)');
      expect(html).toContain('IL Editor');
      expect(html).toContain('Ladder Diagram (LD)');
      expect(html).toContain('FBD Canvas');
      expect(html).toContain('SFC Canvas');
      expect(html).toContain('Variables');
      expect(html).toContain('Data Types');
      expect(html).toContain('Deploy to Controller');
    });

    it('switches between ST, IL, LD, FBD, SFC, Variables, and Data Types views smoothly', () => {
      renderClient(<LogicEditorPlugin targetBlock="MainTask" />);

      // Switch to IL
      const ilTab = container.querySelector('[data-testid="tab-il"]') as HTMLButtonElement;
      expect(ilTab).toBeTruthy();
      flushSync(() => {
        ilTab.click();
      });
      expect(container.innerHTML).toContain('IL');

      // Switch to Ladder Diagram
      const ldTab = container.querySelector('[data-testid="tab-ladder"]') as HTMLButtonElement;
      expect(ldTab).toBeTruthy();
      flushSync(() => {
        ldTab.click();
      });
      expect(container.innerHTML).toContain('Ladder Diagram (LD) Canvas');
      expect(container.innerHTML).toContain('RUNG 1');

      // Switch to FBD
      const fbdTab = container.querySelector('[data-testid="tab-fbd"]') as HTMLButtonElement;
      expect(fbdTab).toBeTruthy();
      flushSync(() => {
        fbdTab.click();
      });
      expect(container.innerHTML).toContain('Function Block Diagram (FBD)');
      expect(container.innerHTML).toContain('AND_1');

      // Switch to SFC
      const sfcTab = container.querySelector('[data-testid="tab-sfc"]') as HTMLButtonElement;
      expect(sfcTab).toBeTruthy();
      flushSync(() => {
        sfcTab.click();
      });
      expect(container.innerHTML).toContain('Sequential Function Chart (SFC)');
      expect(container.innerHTML).toContain('Init_Step');

      // Switch to Variables
      const varsTab = container.querySelector('[data-testid="tab-variables"]') as HTMLButtonElement;
      expect(varsTab).toBeTruthy();
      flushSync(() => {
        varsTab.click();
      });
      expect(container.innerHTML).toContain('DI_StartPB');

      // Switch to Data Types
      const dtTab = container.querySelector('[data-testid="tab-datatypes"]') as HTMLButtonElement;
      expect(dtTab).toBeTruthy();
      flushSync(() => {
        dtTab.click();
      });
      expect(container.innerHTML).toContain('DUT_BottleSpec');
    });
  });

  describe('2. Online Debug Mode & Power-Flow Toggle', () => {
    it('toggles online debug mode with start and stop controls', () => {
      renderClient(<LogicEditorPlugin targetBlock="MainTask" />);

      const debugBtn = container.querySelector('[data-testid="debug-toggle-btn"]') as HTMLButtonElement;
      expect(debugBtn).toBeTruthy();
      expect(debugBtn.textContent).toContain('Online Debug');

      // Toggle Debug On
      flushSync(() => {
        debugBtn.click();
      });

      expect(container.innerHTML).toContain('Stop Debug');
      expect(container.innerHTML).toContain('ONLINE POWER-FLOW');

      // Toggle Debug Off
      flushSync(() => {
        debugBtn.click();
      });

      expect(container.innerHTML).toContain('Online Debug');
    });

    it('opens Variable Forcing Deck modal when forced variable trigger is clicked in online debug mode', () => {
      renderClient(<LogicEditorPlugin targetBlock="MainTask" />);

      // Enable online debug first
      const debugBtn = container.querySelector('[data-testid="debug-toggle-btn"]') as HTMLButtonElement;
      flushSync(() => {
        debugBtn.click();
      });

      // Now PowerFlowOverlay is rendered with the force deck button
      const overlayBtn = container.querySelector('[data-testid="power-flow-overlay"] button') as HTMLButtonElement;
      expect(overlayBtn).toBeTruthy();
      expect(overlayBtn.textContent).toContain('Forced:');

      // Click to open modal
      flushSync(() => {
        overlayBtn.click();
      });

      expect(container.innerHTML).toContain('PLC Variable Force Deck');
      expect(container.innerHTML).toContain('SYS-REQ-096 SAFETY LOCK');
    });
  });

  describe('3. Collaboration & Multi-User Awareness Binding', () => {
    it('accepts Yjs yText and awareness document bindings without throwing', () => {
      const ydoc = new Y.Doc();
      const ytext = ydoc.getText('logic');
      ytext.insert(0, 'PROGRAM CollabTest; Conveyor := TRUE; END_PROGRAM;');

      expect(() => {
        renderClient(
          <LogicEditorPlugin
            targetBlock="CollabTest"
            yText={ytext}
            roomId="mythos-collab-room-1"
          />
        );
      }).not.toThrow();

      expect(container.innerHTML).toContain('CollabTest');
      ydoc.destroy();
    });
  });
});
