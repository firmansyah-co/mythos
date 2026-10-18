/**
 * MythOS HMI Types & Descriptors conforming to Siemens TIA Portal & ISA-101.
 */

export type HmiLifecycleMode = 'EDIT' | 'PREVIEW' | 'DEPLOY' | 'RUNTIME';

export interface SubScreenDescriptor {
  id: string;
  name: string;
  type: 'POPUP' | 'SLIDE_OVER' | 'EMBEDDED';
  width?: number;
  height?: number;
  canvasData?: Record<string, any>;
}

export interface ScreenDescriptor {
  id: string;
  name: string;
  is_root: boolean;
  version?: string;
  subscreens: SubScreenDescriptor[];
  canvasData?: Record<string, any>;
}

export interface ScreenTab {
  tabId: string;
  screenId: string;
  subscreenId?: string;
  title: string;
  isSubScreen: boolean;
  isDirty: boolean;
}

export interface AuthCredentials {
  role: 'Engineer' | 'Admin' | 'Operator';
  token?: string;
  password?: string;
}
