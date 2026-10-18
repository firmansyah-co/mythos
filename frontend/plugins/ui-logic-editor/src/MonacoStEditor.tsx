import React, { useState, useEffect } from 'react';
import Editor, { Monaco } from '@monaco-editor/react';
import { registerStructuredTextLanguage, ST_LANGUAGE_ID } from './stLanguage';

export interface MonacoStEditorProps {
  value?: string;
  defaultValue?: string;
  onChange?: (val: string | undefined) => void;
  readOnly?: boolean;
  className?: string;
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
}) => {
  const [internalVal, setInternalVal] = useState<string>(value ?? defaultValue);
  const [isClient, setIsClient] = useState(false);

  useEffect(() => {
    setIsClient(true);
  }, []);

  useEffect(() => {
    if (value !== undefined) {
      setInternalVal(value);
    }
  }, [value]);

  const handleBeforeMount = (monaco: Monaco) => {
    registerStructuredTextLanguage(monaco);
  };

  const handleChange = (newVal: string | undefined) => {
    setInternalVal(newVal ?? '');
    onChange?.(newVal);
  };

  // Safe fallback for tests / headless environments where Monaco fails or is mocked
  const isHeadless = typeof window === 'undefined' || !window.document || !window.navigator;

  if (isHeadless || !isClient) {
    return (
      <div className={`w-full h-full bg-[#0F141C] p-2 ${className}`}>
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
    <div className={`w-full h-full relative overflow-hidden bg-[#0A0D12] ${className}`} data-testid="monaco-st-editor-container">
      <Editor
        height="100%"
        width="100%"
        defaultLanguage={ST_LANGUAGE_ID}
        language={ST_LANGUAGE_ID}
        value={internalVal}
        defaultValue={defaultValue}
        beforeMount={handleBeforeMount}
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
