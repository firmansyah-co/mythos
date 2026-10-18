import React, { useState, useEffect, useRef } from 'react';
import Editor, { Monaco } from '@monaco-editor/react';
import * as Y from 'yjs';
import { MonacoBinding } from 'y-monaco';
import type { Awareness } from 'y-protocols/awareness';
import { registerStructuredTextLanguage, ST_LANGUAGE_ID } from './stLanguage';

export interface MonacoStEditorProps {
  value?: string;
  defaultValue?: string;
  onChange?: (val: string | undefined) => void;
  readOnly?: boolean;
  className?: string;

  // Collaborative editing options (TSK-311)
  yText?: Y.Text;
  awareness?: Awareness;
  roomId?: string;
}

const DEFAULT_ST_SAMPLE = `PROGRAM MainBottlingControl
  VAR
    Conveyor_Motor AT %QX0.0 : BOOL;
    Bottle_Detect_PE AT %IX0.0 : BOOL;
    Capping_Cylinder AT %QX0.1 : BOOL;
    Bottle_Count : DINT := 0;
  END_VAR

  // Cyclic Execution: Detect bottle and actuate pneumatic capping cylinder
  IF Bottle_Detect_PE THEN
    Capping_Cylinder := TRUE;
    Bottle_Count := Bottle_Count + 1;
  ELSE
    Capping_Cylinder := FALSE;
  END_IF;

  // Run conveyor if not in emergency stop
  Conveyor_Motor := TRUE;
END_PROGRAM
`;

export const MonacoStEditor: React.FC<MonacoStEditorProps> = ({
  value,
  defaultValue = DEFAULT_ST_SAMPLE,
  onChange,
  readOnly = false,
  className = '',
  yText,
  awareness,
  roomId,
}) => {
  const [internalVal, setInternalVal] = useState<string>(() => {
    if (yText && yText.toString().length > 0) {
      return yText.toString();
    }
    return value ?? defaultValue;
  });
  const [isClient, setIsClient] = useState(false);

  const editorRef = useRef<any>(null);
  const bindingRef = useRef<MonacoBinding | null>(null);

  useEffect(() => {
    setIsClient(true);
  }, []);

  // Sync external value when not bound to Y.Text
  useEffect(() => {
    if (!yText && value !== undefined) {
      setInternalVal(value);
    }
  }, [value, yText]);

  // Sync Y.Text changes to internalVal for fallback / external visibility
  useEffect(() => {
    if (!yText) return;

    const observer = () => {
      const current = yText.toString();
      setInternalVal(current);
      onChange?.(current);
    };

    yText.observe(observer);
    if (yText.toString().length > 0) {
      setInternalVal(yText.toString());
    } else if (defaultValue) {
      // Seed initial value into yText if empty
      yText.insert(0, defaultValue);
      setInternalVal(defaultValue);
    }

    return () => {
      yText.unobserve(observer);
    };
  }, [yText]);

  // Manage MonacoBinding lifecycle when editor instance and yText are ready
  useEffect(() => {
    const editor = editorRef.current;
    if (!editor || !yText) {
      if (bindingRef.current) {
        bindingRef.current.destroy();
        bindingRef.current = null;
      }
      return;
    }

    const model = editor.getModel();
    if (!model) return;

    try {
      const binding = new MonacoBinding(
        yText,
        model,
        new Set([editor]),
        awareness ?? null
      );
      bindingRef.current = binding;

      // Inject custom styling for remote cursor tags if awareness is present
      if (awareness && typeof document !== 'undefined') {
        injectRemoteCursorStyles();
      }

      return () => {
        binding.destroy();
        bindingRef.current = null;
      };
    } catch (err) {
      console.warn('[MonacoStEditor] Failed to initialize MonacoBinding:', err);
    }
  }, [yText, awareness]);

  const handleBeforeMount = (monaco: Monaco) => {
    registerStructuredTextLanguage(monaco);
  };

  const handleOnMount = (editor: any, _monaco: Monaco) => {
    editorRef.current = editor;

    if (yText) {
      const model = editor.getModel();
      if (model) {
        try {
          const binding = new MonacoBinding(
            yText,
            model,
            new Set([editor]),
            awareness ?? null
          );
          bindingRef.current = binding;
          injectRemoteCursorStyles();
        } catch (err) {
          console.warn('[MonacoStEditor] Failed to initialize MonacoBinding on mount:', err);
        }
      }
    }
  };

  const handleChange = (newVal: string | undefined) => {
    // If not bound to Yjs, track internal value manually
    if (!yText) {
      setInternalVal(newVal ?? '');
      onChange?.(newVal);
    } else {
      // For textarea fallback in headless mode: update yText manually
      if (newVal !== undefined && newVal !== yText.toString()) {
        yText.doc?.transact(() => {
          yText.delete(0, yText.length);
          yText.insert(0, newVal);
        });
      }
    }
  };

  // Safe fallback for tests / headless environments where Monaco fails or is mocked
  const isHeadless = typeof window === 'undefined' || !window.document || !window.navigator || Boolean((window as any).happyDOM) || Boolean((import.meta as any).env?.TEST);

  if (isHeadless || !isClient) {
    return (
      <div className={`w-full h-full bg-[#0F141C] p-2 ${className}`} data-testid="monaco-st-editor-wrapper">
        {roomId && (
          <div className="text-[10px] text-cyan-400 font-mono mb-1 flex items-center justify-between">
            <span>● Room: {roomId}</span>
            <span>{yText ? 'Yjs CRDT Active' : 'Standalone'}</span>
          </div>
        )}
        <textarea
          data-testid="monaco-headless-fallback"
          className="w-full h-full bg-neutral-900 text-neutral-200 font-mono text-xs p-3 border border-neutral-800 rounded resize-none focus:outline-none"
          value={internalVal}
          readOnly={readOnly}
          onChange={(e) => handleChange(e.target.value)}
        />
      </div>
    );
  }

  return (
    <div
      className={`w-full h-full relative overflow-hidden bg-[#0A0D12] ${className}`}
      data-testid="monaco-st-editor-container"
    >
      <Editor
        height="100%"
        width="100%"
        defaultLanguage={ST_LANGUAGE_ID}
        language={ST_LANGUAGE_ID}
        value={yText ? undefined : internalVal}
        defaultValue={defaultValue}
        beforeMount={handleBeforeMount}
        onMount={handleOnMount}
        onChange={handleChange}
        theme="vs-dark"
        options={{
          readOnly,
          minimap: { enabled: true, maxColumn: 80 },
          fontSize: 13,
          lineNumbers: 'on',
          scrollBeyondLastLine: false,
          automaticLayout: true,
          fontFamily: "'JetBrains Mono', 'Fira Code', Menlo, Monaco, 'Courier New', monospace",
          fontLigatures: true,
          tabSize: 2,
          renderWhitespace: 'selection',
          bracketPairColorization: { enabled: true },
        }}
        loading={
          <div className="flex items-center justify-center h-full text-xs font-mono text-neutral-400 bg-[#0A0D12]">
            Loading IEC 61131-3 Monaco Editor...
          </div>
        }
      />
    </div>
  );
};

/**
 * Injects dynamic CSS rules for y-monaco remote cursor and selection rendering.
 */
function injectRemoteCursorStyles(): void {
  const STYLE_ID = 'y-monaco-cursor-styles';
  if (typeof document === 'undefined' || document.getElementById(STYLE_ID)) {
    return;
  }

  const styleEl = document.createElement('style');
  styleEl.id = STYLE_ID;
  styleEl.textContent = `
    .yRemoteSelection {
      background-color: rgba(59, 130, 246, 0.25);
    }
    .yRemoteSelectionHead {
      position: absolute;
      border-left: 2px solid #3B82F6;
      box-sizing: border-box;
      height: 100%;
    }
    .yRemoteSelectionHead::after {
      content: ' ';
      border: 3px solid #3B82F6;
      border-radius: 4px;
      left: -4px;
      top: -5px;
      position: absolute;
    }
  `;
  document.head.appendChild(styleEl);
}
