import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import React from 'react';
import { createRoot, Root } from 'react-dom/client';
import { flushSync } from 'react-dom';
import { VariableForcingDeck } from '../debug/VariableForcingDeck';
import { ForcedVariableEntry, ForcingAuditLog } from '../debug/types';

let container: HTMLDivElement;
let root: Root;

function renderClient(element: React.ReactElement): string {
  flushSync(() => {
    root.render(element);
  });
  return container.innerHTML;
}

describe('TSK-705: VariableForcingDeck Modal Suite (SYS-REQ-096 Safety Invariant)', () => {
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

  it('renders nothing when isOpen is false', () => {
    const html = renderClient(
      <VariableForcingDeck
        isOpen={false}
        onClose={() => {}}
        forcedVariables={{}}
        auditLogs={[]}
        onForceVariable={() => {}}
        onUnforceVariable={() => {}}
        onReleaseAll={() => {}}
      />
    );
    expect(html).toBe('');
  });

  it('renders Variable Force Deck modal with SYS-REQ-096 safety banner when isOpen is true', () => {
    const html = renderClient(
      <VariableForcingDeck
        isOpen={true}
        onClose={() => {}}
        forcedVariables={{}}
        auditLogs={[]}
        onForceVariable={() => {}}
        onUnforceVariable={() => {}}
        onReleaseAll={() => {}}
      />
    );
    expect(html).toContain('PLC Variable Force Deck');
    expect(html).toContain('SYS-REQ-096 SAFETY LOCK');
    expect(html).toContain('CAUTION: Forcing overrides physical sensor inputs');
    expect(html).toContain('No active forced variables');
  });

  it('renders active forced variables and allows unforcing an item', () => {
    const onUnforce = vi.fn();
    const mockForced: Record<string, ForcedVariableEntry> = {
      DI_StartPB: {
        variableName: 'DI_StartPB',
        dataType: 'BOOL',
        value: true,
        forcedAt: '2026-10-19T18:00:00Z',
        operator: 'QA_Test_Operator',
        active: true,
      },
    };

    renderClient(
      <VariableForcingDeck
        isOpen={true}
        onClose={() => {}}
        forcedVariables={mockForced}
        auditLogs={[]}
        onForceVariable={() => {}}
        onUnforceVariable={onUnforce}
        onReleaseAll={() => {}}
      />
    );

    expect(container.innerHTML).toContain('DI_StartPB');
    expect(container.innerHTML).toContain('true');

    const buttons = Array.from(container.querySelectorAll('button'));
    const unforceBtn = buttons.find((b) => b.textContent?.trim() === 'Unforce');
    expect(unforceBtn).toBeTruthy();

    flushSync(() => {
      unforceBtn?.click();
    });

    expect(onUnforce).toHaveBeenCalledWith('DI_StartPB');
  });

  it('renders immutable audit log trail in the second tab', () => {
    const mockLogs: ForcingAuditLog[] = [
      {
        id: 'log-1',
        timestamp: '2026-10-19T18:00:00Z',
        variableName: 'DI_EStop',
        action: 'FORCE',
        value: false,
        operator: 'System Administrator',
        comment: 'Emergency Stop override for maintenance test',
      },
    ];

    renderClient(
      <VariableForcingDeck
        isOpen={true}
        onClose={() => {}}
        forcedVariables={{}}
        auditLogs={mockLogs}
        onForceVariable={() => {}}
        onUnforceVariable={() => {}}
        onReleaseAll={() => {}}
      />
    );

    // Switch to audit tab
    const buttons = Array.from(container.querySelectorAll('button'));
    const auditTabBtn = buttons.find((b) => b.textContent?.includes('Forcing Audit Trail'));
    expect(auditTabBtn).toBeTruthy();

    flushSync(() => {
      auditTabBtn?.click();
    });

    expect(container.innerHTML).toContain('Emergency Stop override');
    expect(container.innerHTML).toContain('FORCE');
    expect(container.innerHTML).toContain('System Administrator');
  });
});
