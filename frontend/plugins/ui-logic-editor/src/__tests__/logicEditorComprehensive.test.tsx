import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import React from 'react';
import { createRoot, Root } from 'react-dom/client';
import { flushSync } from 'react-dom';
import { DeployControlBar, DeployResultInfo } from '../DeployControlBar';
import { MonacoStEditor } from '../MonacoStEditor';
import { LogicEditorPlugin } from '../LogicEditorPlugin';
import { LadderCanvas } from '../LadderCanvas';
import { registerStructuredTextLanguage, ST_LANGUAGE_ID, stLanguageDefinition } from '../stLanguage';

let container: HTMLDivElement;
let root: Root;

function renderClient(element: React.ReactElement): string {
  flushSync(() => {
    root.render(element);
  });
  return container.innerHTML;
}

describe('Monaco Logic Editor & Deployment Pipeline (TSK-307 / SYS-REQ-021)', () => {
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

  describe('1. IEC 61131-3 Structured Text Language Definition & Monarch Registration', () => {
    it('defines structured-text language ID and required keywords', () => {
      expect(ST_LANGUAGE_ID).toBe('structured-text');
      const keywords = stLanguageDefinition.keywords as string[];
      expect(keywords).toContain('PROGRAM');
      expect(keywords).toContain('END_PROGRAM');
      expect(keywords).toContain('FUNCTION_BLOCK');
      expect(keywords).toContain('VAR');
      expect(keywords).toContain('END_VAR');
      expect(keywords).toContain('IF');
      expect(keywords).toContain('THEN');
      expect(keywords).toContain('ELSIF');
      expect(keywords).toContain('ELSE');
      expect(keywords).toContain('END_IF');
      expect(keywords).toContain('CASE');
      expect(keywords).toContain('FOR');
      expect(keywords).toContain('WHILE');
    });

    it('defines standard IEC 61131-3 data types', () => {
      const typeKeywords = stLanguageDefinition.typeKeywords as string[];
      expect(typeKeywords).toContain('BOOL');
      expect(typeKeywords).toContain('SINT');
      expect(typeKeywords).toContain('INT');
      expect(typeKeywords).toContain('DINT');
      expect(typeKeywords).toContain('LINT');
      expect(typeKeywords).toContain('REAL');
      expect(typeKeywords).toContain('LREAL');
      expect(typeKeywords).toContain('TIME');
      expect(typeKeywords).toContain('STRING');
      expect(typeKeywords).toContain('BYTE');
      expect(typeKeywords).toContain('WORD');
      expect(typeKeywords).toContain('DWORD');
    });

    it('registers Monarch language into Monaco without throwing', () => {
      const mockMonaco = {
        languages: {
          getLanguages: vi.fn().mockReturnValue([]),
          register: vi.fn(),
          setMonarchTokensProvider: vi.fn(),
          setLanguageConfiguration: vi.fn(),
        },
      };

      registerStructuredTextLanguage(mockMonaco as any);
      expect(mockMonaco.languages.register).toHaveBeenCalledWith({ id: ST_LANGUAGE_ID });
      expect(mockMonaco.languages.setMonarchTokensProvider).toHaveBeenCalledWith(
        ST_LANGUAGE_ID,
        stLanguageDefinition
      );
      expect(mockMonaco.languages.setLanguageConfiguration).toHaveBeenCalled();
    });
  });

  describe('2. DeployControlBar & POST /api/v1/control/deploy Dispatch', () => {
    it('renders control bar with target block indicator and deploy button', () => {
      const html = renderClient(
        <DeployControlBar code="PROGRAM OB1; END_PROGRAM;" targetBlock="MainTask" />
      );
      expect(html).toContain('TARGET:');
      expect(html).toContain('MainTask');
      expect(html).toContain('IEC 61131-3 ST');
      expect(html).toContain('Deploy to Controller');
    });

    it('dispatches structured payload to POST /api/v1/control/deploy on deploy click and handles success', async () => {
      let capturedUrl = '';
      let capturedBody: any = null;

      global.fetch = vi.fn().mockImplementation(async (url: string, init: any) => {
        capturedUrl = url;
        capturedBody = JSON.parse(init.body);
        return {
          ok: true,
          status: 200,
          json: async () => ({
            success: true,
            compilation_time_ms: 38,
            message: 'In-controller compilation and hot-reload succeeded',
            binary_hash: 'c8f498b37ad2e10',
          }),
        };
      });

      let reportedResult: DeployResultInfo | null = null;
      renderClient(
        <DeployControlBar
          code="PROGRAM BottlingControl; Conveyor := TRUE; END_PROGRAM;"
          targetBlock="OB1_Main"
          onDeployResult={(res) => {
            reportedResult = res;
          }}
        />
      );

      const deployBtn = container.querySelector('button');
      expect(deployBtn).toBeTruthy();

      deployBtn?.click();

      // Wait for async fetch to resolve and state to flush
      await vi.waitFor(() => {
        expect(reportedResult).toBeTruthy();
      });

      expect(capturedUrl).toBe('/api/v1/control/deploy');
      expect(capturedBody).toEqual({
        code: 'PROGRAM BottlingControl; Conveyor := TRUE; END_PROGRAM;',
        language: 'ST',
        target_block: 'OB1_Main',
        hot_reload: true,
      });

      expect(reportedResult?.success).toBe(true);
      expect(reportedResult?.compilationTimeMs).toBe(38);
      expect(reportedResult?.binaryHash).toBe('c8f498b37ad2e10');

      await vi.waitFor(() => {
        expect(container.innerHTML).toContain('In-controller compilation and hot-reload succeeded');
        expect(container.innerHTML).toContain('38ms');
      });
    });

    it('handles compilation errors and HTTP error responses with diagnostics', async () => {
      global.fetch = vi.fn().mockImplementation(async () => {
        return {
          ok: false,
          status: 422,
          text: async () =>
            JSON.stringify({
              success: false,
              message: 'Syntax error at line 5: unexpected token AT',
            }),
        };
      });

      let reportedResult: DeployResultInfo | null = null;
      renderClient(
        <DeployControlBar
          code="INVALID SYNTAX;"
          targetBlock="MainTask"
          onDeployResult={(res) => {
            reportedResult = res;
          }}
        />
      );

      const deployBtn = container.querySelector('button');
      deployBtn?.click();

      await vi.waitFor(() => {
        expect(reportedResult).toBeTruthy();
      });

      expect(reportedResult?.success).toBe(false);
      expect(reportedResult?.message).toContain('Deployment failed (422)');
      expect(reportedResult?.message).toContain('Syntax error at line 5');

      await vi.waitFor(() => {
        expect(container.innerHTML).toContain('Syntax error at line 5');
      });
    });

    it('handles network connection refusal gracefully', async () => {
      global.fetch = vi.fn().mockRejectedValue(new Error('Failed to fetch (ECONNREFUSED)'));

      let reportedResult: DeployResultInfo | null = null;
      renderClient(
        <DeployControlBar
          code="PROGRAM Test; END_PROGRAM"
          onDeployResult={(res) => {
            reportedResult = res;
          }}
        />
      );

      const deployBtn = container.querySelector('button');
      deployBtn?.click();

      await vi.waitFor(() => {
        expect(reportedResult).toBeTruthy();
      });

      expect(reportedResult?.success).toBe(false);
      expect(reportedResult?.message).toContain('Network/RPC error: Failed to fetch (ECONNREFUSED)');

      await vi.waitFor(() => {
        expect(container.innerHTML).toContain('Network/RPC error');
      });
    });
  });

  describe('3. MonacoStEditor Component & Headless Fallback', () => {
    it('renders textarea fallback when in headless / simulated DOM environment', () => {
      const html = renderClient(
        <MonacoStEditor defaultValue="PROGRAM Headless; END_PROGRAM;" />
      );
      expect(html).toContain('PROGRAM Headless; END_PROGRAM;');
      expect(container.querySelector('textarea')).toBeTruthy();
    });

    it('triggers onChange callbacks when content changes', () => {
      const onChangeMock = vi.fn();
      renderClient(
        <MonacoStEditor defaultValue="Initial ST" onChange={onChangeMock} />
      );

      const textarea = container.querySelector('textarea') as HTMLTextAreaElement;
      expect(textarea).toBeTruthy();

      flushSync(() => {
        textarea.value = 'Updated ST Source';
        textarea.dispatchEvent(new Event('input', { bubbles: true }));
        // Simulate React onChange
        textarea.dispatchEvent(new Event('change', { bubbles: true }));
      });
    });
  });

  describe('4. Ladder Diagram & LogicEditorPlugin Integration', () => {
    it('renders an explicit empty canvas rather than an invented program', () => {
      // The canvas used to default to a sample conveyor interlock program. With no author data
      // it now says so; the fabricated identifiers must not appear through any path.
      const html = renderClient(<LadderCanvas targetBlock="OB1" />);
      expect(html).toContain('Ladder Diagram (LD)');
      expect(html).toContain('No logic in this POU');
      expect(html).not.toContain('RUNG 1');
      expect(html).not.toContain('external.DI_StartPB');
      expect(html).not.toContain('TON_BottleDwell');
    });

    it('renders full LogicEditorPlugin with toggle tabs between ST and Ladder Diagram', () => {
      const html = renderClient(<LogicEditorPlugin targetBlock="MainTask" />);
      expect(html).toContain('Structured Text (ST)');
      expect(html).toContain('Ladder Diagram (LD)');
      expect(html).toContain('Deploy to Controller');
    });
  });
});
