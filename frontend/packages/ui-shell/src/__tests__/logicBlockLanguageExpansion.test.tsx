import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import React from 'react';
import { createRoot, Root } from 'react-dom/client';
import { flushSync } from 'react-dom';
import { CreateBlockModal } from '../components/modals/CreateBlockModal';
import { LogicBlockLanguage } from '../types/hmi';

let container: HTMLDivElement;
let root: Root;

function renderClient(element: React.ReactElement): string {
  flushSync(() => {
    root.render(element);
  });
  return container.innerHTML;
}

describe('IEC 61131-3 Multi-Language Logic Block Support in UI Shell', () => {
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

  it('provides all 5 IEC 61131-3 languages in CreateBlockModal (ST, IL, LD, FBD, SFC)', () => {
    let createdBlock: any = null;
    const html = renderClient(
      <CreateBlockModal
        isOpen={true}
        onClose={() => {}}
        onCreate={(block) => {
          createdBlock = block;
        }}
      />
    );

    expect(html).toContain('Structured Text (ST)');
    expect(html).toContain('Instruction List (IL)');
    expect(html).toContain('Ladder Diagram (LD)');
    expect(html).toContain('Function Block Diagram (FBD)');
    expect(html).toContain('Sequential Function Chart (SFC)');

    const select = container.querySelector('[data-testid="modal-block-language"]') as HTMLSelectElement;
    expect(select).toBeTruthy();

    // Select IL
    flushSync(() => {
      select.value = 'IL';
      select.dispatchEvent(new Event('change', { bubbles: true }));
    });

    const submitBtn = container.querySelector('[data-testid="modal-block-submit"]') as HTMLButtonElement;
    flushSync(() => {
      submitBtn.click();
    });

    expect(createdBlock).toBeTruthy();
    expect(createdBlock.language).toBe('IL');
  });
});
