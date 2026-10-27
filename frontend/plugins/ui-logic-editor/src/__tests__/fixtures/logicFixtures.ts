/**
 * Fixtures for the logic-editor tests.
 *
 * These were `DEFAULT_*` constants living beside the components, wired in as parameter defaults.
 * That is how the editors came to render an invented PLC program, an invented variable table and
 * invented data types for any caller that passed no data: `LogicEditorPlugin` passes none.
 *
 * They live here so no component can import them. They describe sample content for tests only.
 * Production code must render an empty surface when it has no author data, and must never fall
 * back to a sample.
 */

import type { LadderRung } from '../../ladder/types';
import type { FbdBlock, FbdWire } from '../../fbd/types';
import type { SfcStep, SfcTransition } from '../../sfc/types';
import type { VariableRow, DataTypeDefinition } from '../../variables/types';

export const ST_FIXTURE_PROGRAM = `PROGRAM MainBottlingControl
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

export const IL_FIXTURE_PROGRAM = `(* Instruction List IEC 61131-3 Routine *)
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

export const LADDER_FIXTURE_RUNGS: LadderRung[] = [
  {
    id: 'rung-1',
    comment: 'Network 1: Feed Conveyor Run permissive interlock',
    isEnergized: false,
    elements: [
      { id: 'e1', type: 'contact_no', tag: 'external.DI_StartPB', isEnergized: false },
      { id: 'e2', type: 'contact_nc', tag: 'external.DI_EStop_PB', isEnergized: true },
      { id: 'e3', type: 'coil', tag: 'external.DO_ConveyorRun', isEnergized: false },
    ],
  },
  {
    id: 'rung-2',
    comment: 'Network 2: Bottle Detect & Indexing Gate with TON Timer',
    isEnergized: false,
    elements: [
      { id: 'e4', type: 'contact_no', tag: 'external.DI_BottlePE', isEnergized: false },
      {
        id: 'e-block-1',
        type: 'function_block',
        blockType: 'TON',
        instanceName: 'TON_BottleDwell',
        inputs: [
          { name: 'IN', type: 'BOOL', binding: 'DI_BottlePE' },
          { name: 'PT', type: 'TIME', binding: 'T#500MS' },
        ],
        outputs: [
          { name: 'Q', type: 'BOOL', binding: 'Gate_Open' },
          { name: 'ET', type: 'TIME', binding: 'Dwell_Elapsed' },
        ],
      },
      { id: 'e5', type: 'coil_set', tag: 'internal.B_IndexActive', isEnergized: false },
    ],
  },
  {
    id: 'rung-3',
    comment: 'Network 3: Dual Emergency Stop Bypass (Parallel OR Branch)',
    isEnergized: false,
    elements: [
      {
        id: 'branch-1',
        type: 'branch',
        isEnergized: false,
        branches: [
          [
            { id: 'b1-e1', type: 'contact_no', tag: 'DI_ManualPB', isEnergized: false },
          ],
          [
            { id: 'b2-e1', type: 'contact_no', tag: 'DI_AutoPermissive', isEnergized: false },
            { id: 'b2-e2', type: 'contact_nc', tag: 'DI_FaultAlarm', isEnergized: true },
          ],
        ],
      },
      { id: 'e6', type: 'coil', tag: 'DO_MainPowerContactor', isEnergized: false },
    ],
  },
];

export const FBD_FIXTURE_BLOCKS: FbdBlock[] = [
  {
    id: 'blk-and-1',
    name: 'AND_1',
    blockType: 'AND',
    category: 'LOGIC',
    x: 60,
    y: 60,
    inputs: [
      { id: 'pin-in-1', name: 'IN1', dataType: 'BOOL', isNegated: false },
      { id: 'pin-in-2', name: 'IN2', dataType: 'BOOL', isNegated: false },
    ],
    outputs: [{ id: 'pin-out-1', name: 'OUT', dataType: 'BOOL' }],
  },
  {
    id: 'blk-timer-1',
    name: 'TON_1',
    blockType: 'TON',
    category: 'TIMER',
    x: 320,
    y: 60,
    inputs: [
      { id: 'pin-ton-in', name: 'IN', dataType: 'BOOL', isNegated: false },
      { id: 'pin-ton-pt', name: 'PT', dataType: 'TIME', value: 'T#2S' },
    ],
    outputs: [
      { id: 'pin-ton-q', name: 'Q', dataType: 'BOOL' },
      { id: 'pin-ton-et', name: 'ET', dataType: 'TIME' },
    ],
  },
  {
    id: 'blk-pid-1',
    name: 'PID_Pressure',
    blockType: 'PID',
    category: 'CONTROL',
    x: 600,
    y: 60,
    inputs: [
      { id: 'pin-pid-sp', name: 'SP', dataType: 'REAL', value: '50.0' },
      { id: 'pin-pid-pv', name: 'PV', dataType: 'REAL', value: '48.2' },
      { id: 'pin-pid-kp', name: 'KP', dataType: 'REAL', value: '2.5' },
    ],
    outputs: [
      { id: 'pin-pid-mv', name: 'MV', dataType: 'REAL' },
      { id: 'pin-pid-err', name: 'ERROR', dataType: 'REAL' },
    ],
  },
];

export const FBD_FIXTURE_WIRES: FbdWire[] = [
  {
    id: 'wire-1',
    fromBlockId: 'blk-and-1',
    fromPinId: 'pin-out-1',
    toBlockId: 'blk-timer-1',
    toPinId: 'pin-ton-in',
    isEnergized: true,
  },
];

export const SFC_FIXTURE_STEPS: SfcStep[] = [
  {
    id: 'step-0',
    name: 'Init_Step',
    isInitial: true,
    isActive: true,
    x: 180,
    y: 40,
    actions: [
      { id: 'act-1', qualifier: 'N', actionName: 'DO_IdleIndicator := TRUE;' },
    ],
  },
  {
    id: 'step-1',
    name: 'Conveyor_Run',
    isInitial: false,
    isActive: false,
    x: 180,
    y: 200,
    actions: [
      { id: 'act-2', qualifier: 'S', actionName: 'DO_ConveyorMotor' },
      { id: 'act-3', qualifier: 'L', duration: 'T#5S', actionName: 'FeedWarningLight' },
    ],
  },
  {
    id: 'step-2',
    name: 'Cap_Cylinder_Dwell',
    isInitial: false,
    isActive: false,
    x: 180,
    y: 360,
    actions: [
      { id: 'act-4', qualifier: 'P', actionName: 'TriggerBottleCountInc' },
      { id: 'act-5', qualifier: 'D', duration: 'T#500MS', actionName: 'ExtendCappingArm' },
    ],
  },
];

export const SFC_FIXTURE_TRANSITIONS: SfcTransition[] = [
  {
    id: 'trans-0-1',
    name: 'T1',
    condition: 'DI_StartPB = TRUE',
    fromStepId: 'step-0',
    toStepId: 'step-1',
    isSatisfied: true,
  },
  {
    id: 'trans-1-2',
    name: 'T2',
    condition: 'DI_BottleInPosition = TRUE',
    fromStepId: 'step-1',
    toStepId: 'step-2',
    isSatisfied: false,
  },
];

export const VARIABLE_FIXTURE_ROWS: VariableRow[] = [
  {
    id: 'v-1',
    name: 'DI_StartPB',
    scope: 'VAR_INPUT',
    dataType: 'BOOL',
    address: '%IX0.0',
    initialValue: 'FALSE',
    retain: false,
    constant: false,
    comment: 'Conveyor Start Pushbutton (N.O.)',
  },
  {
    id: 'v-2',
    name: 'DI_EStop_PB',
    scope: 'VAR_INPUT',
    dataType: 'BOOL',
    address: '%IX0.1',
    initialValue: 'TRUE',
    retain: false,
    constant: false,
    comment: 'Emergency Stop Mushroom Button (N.C.)',
  },
  {
    id: 'v-3',
    name: 'DO_ConveyorRun',
    scope: 'VAR_OUTPUT',
    dataType: 'BOOL',
    address: '%QX0.0',
    initialValue: 'FALSE',
    retain: false,
    constant: false,
    comment: 'Conveyor Motor Contactor Actuation',
  },
  {
    id: 'v-4',
    name: 'Bottle_Count',
    scope: 'VAR',
    dataType: 'DINT',
    address: '',
    initialValue: '0',
    retain: true,
    constant: false,
    comment: 'Accumulated bottle count in shift',
  },
  {
    id: 'v-5',
    name: 'Conveyor_Velocity_Setpoint',
    scope: 'VAR',
    dataType: 'REAL',
    address: '%MW100',
    initialValue: '24.5',
    retain: false,
    constant: false,
    comment: 'VFD Speed Reference in m/min',
  },
];

export const DATA_TYPE_FIXTURE: DataTypeDefinition[] = [
  {
    id: 'dt-bottle-spec',
    name: 'DUT_BottleSpec',
    kind: 'STRUCT',
    comment: 'Bottling line container dimensional profile',
    members: [
      { id: 'm-1', name: 'Height_mm', dataType: 'REAL', initialValue: '250.0', comment: 'Bottle height' },
      { id: 'm-2', name: 'Diameter_mm', dataType: 'REAL', initialValue: '75.0', comment: 'Outer diameter' },
      { id: 'm-3', name: 'Volume_ml', dataType: 'DINT', initialValue: '500', comment: 'Nominal volume in ml' },
      { id: 'm-4', name: 'IsGlass', dataType: 'BOOL', initialValue: 'TRUE', comment: 'Glass vs PET flag' },
    ],
  },
  {
    id: 'dt-machine-state',
    name: 'DUT_MachineState',
    kind: 'ENUM',
    comment: 'PackML ISA-TR88 Operational Machine States',
    enumConstants: ['STOPPED', 'STARTING', 'IDLE', 'RUNNING', 'HOLDING', 'HELD', 'ABORTED'],
  },
];
