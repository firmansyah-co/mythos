/**
 * IEC 61131-3 Function Block Diagram (FBD) Types & Specifications.
 */

export type FbdDataType = 'BOOL' | 'INT' | 'DINT' | 'REAL' | 'TIME' | 'STRING' | 'ANY';

export interface FbdPin {
  id: string;
  name: string;
  dataType: FbdDataType;
  isNegated?: boolean; // Inversion bubble 'o'
  value?: string | number | boolean;
  connectedWireId?: string;
}

export type FbdBlockCategory = 'LOGIC' | 'MATH' | 'TIMER' | 'BISTABLE' | 'SELECTION' | 'CONTROL';

export interface FbdBlock {
  id: string;
  name: string; // e.g. "AND_1", "PID_Flow"
  blockType: 'AND' | 'OR' | 'XOR' | 'NOT' | 'ADD' | 'SUB' | 'MUL' | 'DIV' | 'SEL' | 'MUX' | 'PID' | 'TON' | 'TOF' | 'SR' | 'RS' | string;
  category: FbdBlockCategory;
  x: number;
  y: number;
  inputs: FbdPin[];
  outputs: FbdPin[];
  isEnergized?: boolean;
}

export interface FbdWire {
  id: string;
  fromBlockId: string;
  fromPinId: string;
  toBlockId: string;
  toPinId: string;
  isEnergized?: boolean;
}

export interface FbdCanvasProps {
  blocks?: FbdBlock[];
  wires?: FbdWire[];
  onAddBlock?: (block: FbdBlock) => void;
  onUpdateBlock?: (blockId: string, updated: FbdBlock) => void;
  onDeleteBlock?: (blockId: string) => void;
  onConnectWire?: (fromBlockId: string, fromPinId: string, toBlockId: string, toPinId: string) => void;
  onDeleteWire?: (wireId: string) => void;
  readOnly?: boolean;
  className?: string;

  // Live online debugging telemetry
  debugActive?: boolean;
  liveTelemetry?: Record<string, boolean | number | string>;
}
