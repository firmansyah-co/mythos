/**
 * MythOS HMI Types & Descriptors conforming to Siemens TIA Portal & ISA-101.
 */

export type HmiLifecycleMode = 'EDIT' | 'PREVIEW' | 'DEPLOY' | 'RUNTIME';

export type WorkspaceMode = 'PORTAL' | 'PROJECT';

export type LogicBlockType = 'OB' | 'FB' | 'FC' | 'DB';

export type LogicBlockLanguage = 'ST' | 'IL' | 'LD' | 'FBD' | 'SFC';

export interface LogicBlockDescriptor {
  id: string;
  name: string;
  blockType: LogicBlockType;
  language: LogicBlockLanguage;
  number?: number;
  code?: string;
  comment?: string;
}

export interface ProjectMetadata {
  id: string;
  name: string;
  version: string;
  description?: string;
  author?: string;
  path?: string;
  created_at_utc?: string;
  updated_at_utc?: string;
}

export type SelectedItemType = 'PROJECT' | 'DEVICE' | 'LOGIC_BLOCK' | 'HMI_SCREEN' | 'TAG' | 'FACEPLATE';

export interface SelectedContextItem {
  id: string;
  name: string;
  type: SelectedItemType;
  subType?: string;
  hardwareAddress?: string;
  udtArchetype?: string;
  metadata?: Record<string, any>;
}

export type WorkbenchDocType = 'logic' | 'screen' | 'topology' | 'device' | 'faceplate';

export interface WorkbenchTabItem {
  id: string;
  docType: WorkbenchDocType;
  title: string;
  params?: Record<string, any>;
}

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
