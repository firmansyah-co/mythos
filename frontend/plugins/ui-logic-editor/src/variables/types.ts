/**
 * IEC 61131-3 Variable Scopes, Data Types, and Structure Definitions.
 */

export type VariableScope =
  | 'VAR'
  | 'VAR_INPUT'
  | 'VAR_OUTPUT'
  | 'VAR_IN_OUT'
  | 'VAR_TEMP'
  | 'VAR_GLOBAL'
  | 'VAR_EXTERNAL';

export const ELEMENTARY_DATA_TYPES = [
  'BOOL',
  'BYTE',
  'WORD',
  'DWORD',
  'LWORD',
  'SINT',
  'INT',
  'DINT',
  'LINT',
  'USINT',
  'UINT',
  'UDINT',
  'ULINT',
  'REAL',
  'LREAL',
  'TIME',
  'DATE',
  'TIME_OF_DAY',
  'DATE_AND_TIME',
  'STRING',
  'WSTRING',
] as const;

export type ElementaryDataType = typeof ELEMENTARY_DATA_TYPES[number];

export interface VariableRow {
  id: string;
  name: string;
  scope: VariableScope;
  dataType: string;
  address?: string; // %IX0.0, %QX0.0, %MW10
  initialValue?: string;
  retain?: boolean;
  constant?: boolean;
  comment?: string;
  liveValue?: string | number | boolean;
  isForced?: boolean;
}

export interface StructMember {
  id: string;
  name: string;
  dataType: string;
  initialValue?: string;
  comment?: string;
}

export interface DataTypeDefinition {
  id: string;
  name: string;
  kind: 'STRUCT' | 'ENUM' | 'ALIAS';
  // If STRUCT:
  members?: StructMember[];
  // If ENUM:
  enumConstants?: string[];
  // If ALIAS:
  baseType?: string;
  comment?: string;
}

export interface VariableTableGridProps {
  variables?: VariableRow[];
  availableDataTypes?: string[];
  onChange?: (variables: VariableRow[]) => void;
  readOnly?: boolean;
  className?: string;

  // Live telemetry & forcing
  debugActive?: boolean;
  onForceVariable?: (name: string, value: any) => void;
  onUnforceVariable?: (name: string) => void;
}

export interface DataTypeEditorProps {
  dataTypes?: DataTypeDefinition[];
  onChange?: (dataTypes: DataTypeDefinition[]) => void;
  readOnly?: boolean;
  className?: string;
}
