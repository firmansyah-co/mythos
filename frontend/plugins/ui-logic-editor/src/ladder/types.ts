/**
 * IEC 61131-3 Ladder Diagram (LD) Types and Definitions.
 * Conforms to OpenPLC Editor v4 graphical LD conventions.
 */

export type LadderContactType =
  | 'contact_no'    // Normally Open: ──┤ ├───
  | 'contact_nc'    // Normally Closed: ──┤/├───
  | 'contact_p'     // Positive Transition Sensing (Rising Edge): ──┤P├───
  | 'contact_n';    // Negative Transition Sensing (Falling Edge): ──┤N├───

export type LadderCoilType =
  | 'coil'          // Normal Output Coil: ───( )───
  | 'coil_normal'   // Normal Output Coil: ───( )───
  | 'coil_negated'  // Negated Coil: ───(/)───
  | 'coil_set'      // Set (Latch) Coil: ───(S)───
  | 'coil_reset';   // Reset (Unlatch) Coil: ───(R)───

export interface FunctionBlockPort {
  name: string;
  type: string;
  binding?: string;
  value?: string | number | boolean;
}

export interface LadderBlockElement {
  id: string;
  type: 'function_block';
  blockType: 'TON' | 'TOF' | 'TP' | 'CTU' | 'CTD' | 'SR' | 'RS' | string;
  instanceName: string;
  inputs: FunctionBlockPort[];
  outputs: FunctionBlockPort[];
  isEnergized?: boolean;
}

export interface LadderParallelBranch {
  id: string;
  type: 'branch';
  branches: LadderElement[][];
  isEnergized?: boolean;
}

export interface LadderContactElement {
  id: string;
  type: LadderContactType;
  tag: string;
  isEnergized?: boolean;
}

export interface LadderCoilElement {
  id: string;
  type: LadderCoilType;
  tag: string;
  isEnergized?: boolean;
}

export type LadderElement =
  | LadderContactElement
  | LadderCoilElement
  | LadderBlockElement
  | LadderParallelBranch;

export interface LadderRung {
  id: string;
  comment: string;
  elements: LadderElement[];
  isEnergized?: boolean;
}

export interface LadderCanvasProps {
  rungs?: LadderRung[];
  onAddRung?: () => void;
  onUpdateRung?: (rungId: string, updated: LadderRung) => void;
  onDeleteRung?: (rungId: string) => void;
  targetBlock?: string;
  readOnly?: boolean;
  className?: string;

  // Live online debugging & power-flow telemetry (SYS-REQ-096)
  debugActive?: boolean;
  liveTelemetry?: Record<string, boolean | number | string>;
  onForceVariable?: (variableName: string, value: boolean | number | string) => void;
}
