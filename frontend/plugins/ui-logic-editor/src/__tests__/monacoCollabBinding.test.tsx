import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import React from 'react';
import { createRoot, Root } from 'react-dom/client';
import { flushSync } from 'react-dom';
import * as Y from 'yjs';
import { MonacoBinding } from 'y-monaco';
import { MonacoStEditor } from '../MonacoStEditor';
import { LogicEditorPlugin } from '../LogicEditorPlugin';

let container: HTMLDivElement;
let root: Root;

function renderClient(element: React.ReactElement): string {
  flushSync(() => {
    root.render(element);
  });
  return container.innerHTML;
}

describe('Monaco Collaborative CRDT Binding (TSK-311)', () => {
  beforeEach(() => {
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

  it('binds to Y.Text and initializes with shared document content', () => {
    const doc = new Y.Doc();
    const yText = doc.getText('monaco');
    yText.insert(0, 'PROGRAM BottlingMachine\nVAR count : INT; END_VAR');

    const html = renderClient(
      <MonacoStEditor
        yText={yText}
        roomId="room-bottling-line"
      />
    );

    expect(html).toContain('room-bottling-line');
    expect(html).toContain('Yjs CRDT Active');

    const textarea = container.querySelector('textarea') as HTMLTextAreaElement;
    expect(textarea).toBeTruthy();
    expect(textarea.value).toBe('PROGRAM BottlingMachine\nVAR count : INT; END_VAR');
  });

  it('synchronizes edits across two clients via Yjs CRDT doc', () => {
    const doc1 = new Y.Doc();
    const doc2 = new Y.Doc();

    const yText1 = doc1.getText('logic');
    const yText2 = doc2.getText('logic');

    // Simulate network sync between doc1 and doc2
    doc1.on('update', (update: Uint8Array) => {
      Y.applyUpdate(doc2, update);
    });
    doc2.on('update', (update: Uint8Array) => {
      Y.applyUpdate(doc1, update);
    });

    // Client 1 types initial program
    yText1.insert(0, 'PROGRAM Main;\n');

    // Render Client 2's editor
    renderClient(
      <MonacoStEditor
        yText={yText2}
        roomId="shared-line-1"
      />
    );

    let textarea = container.querySelector('textarea') as HTMLTextAreaElement;
    expect(textarea.value).toBe('PROGRAM Main;\n');


    // Client 2 types in textarea fallback
    flushSync(() => {
      const nativeSetter = Object.getOwnPropertyDescriptor(
        window.HTMLTextAreaElement.prototype,
        'value'
      )!.set!;
      nativeSetter.call(
        textarea,
        'PROGRAM Main;\n  Conveyor := TRUE;\nEND_PROGRAM;'
      );
      textarea.dispatchEvent(new Event('change', { bubbles: true }));
    });


    // Verify both CRDT docs converged deterministically
    expect(yText2.toString()).toBe('PROGRAM Main;\n  Conveyor := TRUE;\nEND_PROGRAM;');
    expect(yText1.toString()).toBe('PROGRAM Main;\n  Conveyor := TRUE;\nEND_PROGRAM;');
  });

  it('passes collaborative props down through LogicEditorPlugin', () => {
    const doc = new Y.Doc();
    const yText = doc.getText('editor-test');
    yText.insert(0, 'PROGRAM PluginTest; END_PROGRAM');

    const html = renderClient(
      <LogicEditorPlugin
        yText={yText}
        roomId="room-integration-99"
      />
    );

    expect(html).toContain('Room: room-integration-99');
    expect(html).toContain('Structured Text (ST)');
    expect(html).toContain('PROGRAM PluginTest; END_PROGRAM');
  });

  it('verifies MonacoBinding instance creation with mock monaco model', () => {
    const doc = new Y.Doc();
    const yText = doc.getText('binding-test');
    yText.insert(0, 'VAR_GLOBAL count : DINT; END_VAR');

    // Mock minimal Monaco ITextModel
    let modelValue = yText.toString();
    const listeners: Array<(e: any) => void> = [];
    const mockModel: any = {
      getValue: () => modelValue,
      setValue: (val: string) => {
        modelValue = val;
      },
      onDidChangeContent: (fn: any) => {
        listeners.push(fn);
        return { dispose: () => {} };
      },
      onWillDispose: () => ({ dispose: () => {} }),
      getPositionAt: (offset: number) => ({ lineNumber: 1, column: offset + 1 }),
      getOffsetAt: (pos: any) => pos.column - 1,
      applyEdits: (edits: any[]) => {
        edits.forEach((edit) => {
          modelValue = edit.text;
        });
      },
    };

    const binding = new MonacoBinding(yText, mockModel);
    expect(binding).toBeDefined();
    expect(mockModel.getValue()).toBe('VAR_GLOBAL count : DINT; END_VAR');

    // Clean up
    binding.destroy();
  });
});
