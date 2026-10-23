import React, { useState, useEffect, useRef } from 'react';
import Editor, { Monaco } from '@monaco-editor/react';
import * as Y from 'yjs';
import { MonacoBinding } from 'y-monaco';
import type { Awareness } from 'y-protocols/awareness';
import { registerStructuredTextLanguage, ST_LANGUAGE_ID } from './stLanguage';
import { registerInstructionListLanguage, IL_LANGUAGE_ID } from './ilLanguage';

export type TextualLogicLanguage = 'ST' | 'IL';

export interface LogicDiagnostic {
  line: number;
  column?: number;
  message: string;
  severity: 'error' | 'warning' | 'info';
}

export interface MonacoStEditorProps {
  value?: string;
  defaultValue?: string;
  onChange?: (val: string | undefined) => void;
  readOnly?: boolean;
  className?: string;

  // Language selection (ST vs IL)
  language?: TextualLogicLanguage;
  onLanguageChange?: (lang: TextualLogicLanguage) => void;

  // Editor Theme
  theme?: 'vs-dark' | 'vs-light' | 'hc-black';

  // Inline Lint Diagnostics
  diagnostics?: LogicDiagnostic[];

  // Live evaluated values map for debug inline view (SYS-REQ-096)
  liveValues?: Record<string, string | number | boolean>;

  // Collaborative editing options (TSK-311)
  yText?: Y.Text;
  awareness?: Awareness;
  roomId?: string;
}

export const DEFAULT_ST_SAMPLE = `PROGRAM MainBottlingControl
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

export const DEFAULT_IL_SAMPLE = `(* Instruction List IEC 61131-3 Routine *)
LD    %IX0.0          (* Load bottle presence sensor *)
ANDN  %IX0.1          (* And Not Emergency Stop *)
ST    %QX0.0          (* Energize conveyor run coil *)

LD    %IX0.2          (* Bottle in position *)
JMPC  ACTUATE_CYLINDER

LD    FALSE
ST    %QX0.1          (* Reset cylinder *)
RET

ACTUATE_CYLINDER:
LD    TRUE
ST    %QX0.1          (* Extend pneumatic cylinder *)
CAL   TON_1(IN:=TRUE, PT:=T#500MS)
RET
`;

export const MonacoStEditor: React.FC<MonacoStEditorProps> = ({
  value,
  defaultValue,
  onChange,
  readOnly = false,
  className = '',
  language = 'ST',
  onLanguageChange,
  theme = 'vs-dark',
  diagnostics = [],
  liveValues,
  yText,
  awareness,
  roomId,
}) => {
  const [currentLang, setCurrentLang] = useState<TextualLogicLanguage>(language);
  const defaultCode = defaultValue ?? (currentLang === 'IL' ? DEFAULT_IL_SAMPLE : DEFAULT_ST_SAMPLE);

  const [internalVal, setInternalVal] = useState<string>(() => {
    if (yText && yText.toString().length > 0) {
      return yText.toString();
    }
    return value ?? defaultCode;
  });
  const [isClient, setIsClient] = useState(false);

  const editorRef = useRef<any>(null);
  const monacoRef = useRef<Monaco | null>(null);
  const bindingRef = useRef<MonacoBinding | null>(null);

  useEffect(() => {
    setIsClient(true);
  }, []);

  useEffect(() => {
    if (language !== currentLang) {
      setCurrentLang(language);
    }
  }, [language]);

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
    } else if (defaultCode) {
      yText.insert(0, defaultCode);
      setInternalVal(defaultCode);
    }

    return () => {
      yText.unobserve(observer);
    };
  }, [yText, defaultCode]);

  // Manage MonacoBinding lifecycle
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

  // Update diagnostics markers when changed
  useEffect(() => {
    if (!monacoRef.current || !editorRef.current) return;
    const model = editorRef.current.getModel();
    if (!model) return;

    const markers = diagnostics.map((d) => {
      const monacoSeverity =
        d.severity === 'error'
          ? monacoRef.current!.MarkerSeverity.Error
          : d.severity === 'warning'
          ? monacoRef.current!.MarkerSeverity.Warning
          : monacoRef.current!.MarkerSeverity.Info;

      return {
        severity: monacoSeverity,
        message: d.message,
        startLineNumber: d.line,
        startColumn: d.column || 1,
        endLineNumber: d.line,
        endColumn: 1000,
      };
    });

    monacoRef.current.editor.setModelMarkers(model, 'logic-linter', markers);
  }, [diagnostics]);

  const handleBeforeMount = (monaco: Monaco) => {
    monacoRef.current = monaco;
    registerStructuredTextLanguage(monaco);
    registerInstructionListLanguage(monaco);
  };

  const handleOnMount = (editor: any, monaco: Monaco) => {
    editorRef.current = editor;
    monacoRef.current = monaco;

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
    if (!yText) {
      setInternalVal(newVal ?? '');
      onChange?.(newVal);
    } else {
      if (newVal !== undefined && newVal !== yText.toString()) {
        yText.doc?.transact(() => {
          yText.delete(0, yText.length);
          yText.insert(0, newVal);
        });
      }
    }
  };

  const activeLanguageId = currentLang === 'IL' ? IL_LANGUAGE_ID : ST_LANGUAGE_ID;

  // Safe fallback for tests / headless environments
  const isHeadless =
    typeof window === 'undefined' ||
    !window.document ||
    !window.navigator ||
    Boolean((window as any).happyDOM) ||
    Boolean((import.meta as any).env?.TEST);

  if (isHeadless || !isClient) {
    return (
      <div className={`w-full h-full bg-[var(--bg-surface)] p-2 flex flex-col ${className}`} data-testid="monaco-st-editor-wrapper">
        <div className="flex items-center justify-between pb-1.5 mb-1 border-b border-neutral-800 text-[11px] font-mono">
          <div className="flex items-center gap-2">
            <span className="text-neutral-400">Language:</span>
            <button
              onClick={() => {
                const next = currentLang === 'ST' ? 'IL' : 'ST';
                setCurrentLang(next);
                onLanguageChange?.(next);
              }}
              data-testid="lang-toggle-btn"
              className="px-2 py-0.5 rounded bg-neutral-800 text-cyan-400 hover:bg-neutral-700"
            >
              {currentLang}
            </button>
            {roomId && <span className="text-cyan-400 ml-2">● Room: {roomId}</span>}
          </div>
          <span className="text-neutral-500">{yText ? 'Yjs CRDT Active' : 'Standalone'}</span>
        </div>

        {diagnostics.length > 0 && (
          <div className="mb-2 p-1.5 bg-rose-950/40 border border-rose-800/60 rounded text-[11px] text-rose-300 font-mono">
            {diagnostics.map((d, i) => (
              <div key={i}>Line {d.line}: {d.message}</div>
            ))}
          </div>
        )}

        {liveValues && Object.keys(liveValues).length > 0 && (
          <div className="mb-2 flex flex-wrap gap-2 text-[10px] font-mono bg-neutral-900/80 p-1.5 rounded border border-neutral-800">
            <span className="text-neutral-500">Live Telemetry:</span>
            {Object.entries(liveValues).map(([k, v]) => (
              <span key={k} className="px-1.5 py-0.5 rounded bg-neutral-800 border border-neutral-700 text-emerald-400">
                {k}: {String(v)}
              </span>
            ))}
          </div>
        )}

        <textarea
          data-testid="monaco-headless-fallback"
          className="flex-1 w-full bg-neutral-900 text-neutral-200 font-mono text-xs p-3 border border-neutral-800 rounded resize-none focus:outline-none"
          value={internalVal}
          readOnly={readOnly}
          onChange={(e) => handleChange(e.target.value)}
        />
      </div>
    );
  }

  return (
    <div
      className={`w-full h-full relative overflow-hidden bg-[var(--bg-base)] flex flex-col ${className}`}
      data-testid="monaco-st-editor-container"
    >
      {/* Mini top toolbar for sub-language toggle & live badge summary */}
      <div className="flex items-center justify-between px-3 py-1 bg-[var(--bg-surface)] border-b border-neutral-800 text-xs">
        <div className="flex items-center gap-2">
          <div className="flex items-center bg-neutral-900 rounded p-0.5 border border-neutral-800">
            <button
              onClick={() => {
                setCurrentLang('ST');
                onLanguageChange?.('ST');
              }}
              className={`px-2 py-0.5 rounded text-[10px] font-mono font-medium transition-colors ${
                currentLang === 'ST'
                  ? 'bg-blue-600 text-white'
                  : 'text-neutral-400 hover:text-neutral-200'
              }`}
            >
              Structured Text (ST)
            </button>
            <button
              onClick={() => {
                setCurrentLang('IL');
                onLanguageChange?.('IL');
              }}
              className={`px-2 py-0.5 rounded text-[10px] font-mono font-medium transition-colors ${
                currentLang === 'IL'
                  ? 'bg-blue-600 text-white'
                  : 'text-neutral-400 hover:text-neutral-200'
              }`}
            >
              Instruction List (IL)
            </button>
          </div>
        </div>

        {liveValues && Object.keys(liveValues).length > 0 && (
          <div className="flex items-center gap-2 overflow-x-auto text-[10px] font-mono">
            <span className="text-neutral-500">Live Badges:</span>
            {Object.entries(liveValues).slice(0, 4).map(([k, v]) => (
              <span
                key={k}
                className="px-1.5 py-0.2 rounded bg-neutral-800 border border-neutral-700 text-emerald-400"
              >
                {k}={String(v)}
              </span>
            ))}
          </div>
        )}
      </div>

      <div className="flex-1 w-full h-full relative">
        <Editor
          height="100%"
          width="100%"
          defaultLanguage={activeLanguageId}
          language={activeLanguageId}
          value={yText ? undefined : internalVal}
          defaultValue={defaultCode}
          beforeMount={handleBeforeMount}
          onMount={handleOnMount}
          onChange={handleChange}
          theme={theme}
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
            <div className="flex items-center justify-center h-full text-xs font-mono text-neutral-400 bg-[var(--bg-base)]">
              Loading IEC 61131-3 Monaco Editor ({currentLang})...
            </div>
          }
        />
      </div>
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
      border-left: 2px solid var(--collab-blue);
      box-sizing: border-box;
      height: 100%;
    }
    .yRemoteSelectionHead::after {
      content: ' ';
      border: 3px solid var(--collab-blue);
      border-radius: 4px;
      left: -4px;
      top: -5px;
      position: absolute;
    }
  `;
  document.head.appendChild(styleEl);
}
