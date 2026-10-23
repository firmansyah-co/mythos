/**
 * IEC 61131-3 Sequential Function Chart (SFC) Types & Specifications.
 */

export type SfcActionQualifier =
  | 'N' // Non-stored (active as long as step is active)
  | 'S' // Set (Stored)
  | 'R' // Reset
  | 'P' // Pulse (executed once on entry)
  | 'L' // time Limited
  | 'D'; // time Delayed

export interface SfcAction {
  id: string;
  qualifier: SfcActionQualifier;
  actionName: string;
  duration?: string; // e.g. T#5s for L / D
  indicator?: string;
}

export interface SfcStep {
  id: string;
  name: string; // e.g. "Step_0", "Step_1"
  isInitial?: boolean; // Double border
  isActive?: boolean;  // Active token
  x: number;
  y: number;
  actions: SfcAction[];
}

export interface SfcTransition {
  id: string;
  name: string;
  condition: string; // Boolean ST expression e.g. "DI_StartPB AND NOT DI_EStop"
  fromStepId: string;
  toStepId: string;
  isSatisfied?: boolean;
}

export interface SfcCanvasProps {
  steps?: SfcStep[];
  transitions?: SfcTransition[];
  onAddStep?: (step: SfcStep) => void;
  onUpdateStep?: (stepId: string, updated: SfcStep) => void;
  onDeleteStep?: (stepId: string) => void;
  onAddTransition?: (trans: SfcTransition) => void;
  readOnly?: boolean;
  className?: string;

  // Live online debugging telemetry
  debugActive?: boolean;
  liveTelemetry?: Record<string, boolean | number | string>;
}
