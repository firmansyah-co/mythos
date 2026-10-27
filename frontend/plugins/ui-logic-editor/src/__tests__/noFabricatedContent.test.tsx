/**
 * SYS-REQ-096: no logic-editor surface may invent content when it has none.
 *
 * Every editor here used to default to a sample: a conveyor interlock program in ladder, an
 * AND/TON/PID network in FBD, a bottling sequence in SFC, a sample declaration table, DUT_ types,
 * and a bottled-plant ST/IL program in the textual editors. `LogicEditorPlugin` passes none of
 * that data, so an engineer opening any POU read a program that was not theirs.
 *
 * These assertions are deliberately broad: any fabricated identifier trips them, not just the
 * literals that used to be present.
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import React from 'react';
import { createRoot, Root } from 'react-dom/client';
import { flushSync } from 'react-dom';
import { LadderCanvas } from '../LadderCanvas';
import { FbdCanvas } from '../fbd/FbdCanvas';
import { SfcCanvas } from '../sfc/SfcCanvas';
import { VariableTableGrid } from '../variables/VariableTableGrid';
import { DataTypeEditor } from '../variables/DataTypeEditor';
import { MonacoStEditor } from '../MonacoStEditor';
import { LogicEditorPlugin } from '../LogicEditorPlugin';

let container: HTMLDivElement;
let root: Root;

function renderClient(element: React.ReactElement): string {
  flushSync(() => {
    root.render(element);
  });
  return container.innerHTML;
}

/** Identifiers from every fabricated surface this change removed. */
const FABRICATED_IDENTIFIERS = [
  'external.DI_StartPB',
  'external.DI_EStop_PB',
  'external.DO_ConveyorRun',
  'TON_BottleDwell',
  'DO_MainPowerContactor',
  'AND_1',
  'PID_Pressure',
  'Init_Step',
  'DI_StartPB',
  'DI_EStop_PB',
  'DUT_BottleSpec',
  'DUT_MachineState',
  'Conveyor_Motor',
  'Capping_Cylinder',
  'MainBottlingControl',
];

function expectNoFabricatedIdentifiers(html: string, surface: string) {
  const found = FABRICATED_IDENTIFIERS.filter((id) => html.includes(id));
  expect(found, `${surface} rendered fabricated content: ${found.join(', ')}`).toEqual([]);
}

describe('SYS-REQ-096: logic editor surfaces render no invented program', () => {
  beforeEach(() => {
    container = document.createElement('div');
    document.body.appendChild(container);
    root = createRoot(container);
  });

  afterEach(() => {
    flushSync(() => {
      root.unmount();
    });
    container.remove();
  });

  it('ladder: shows no logic in this POU instead of sample rungs', () => {
    const html = renderClient(<LadderCanvas />);
    expect(html).toContain('No logic in this POU');
    expect(html).not.toContain('RUNG 1');
    expectNoFabricatedIdentifiers(html, 'LadderCanvas');
  });

  it('fbd: shows no logic in this POU instead of a sample block network', () => {
    const html = renderClient(<FbdCanvas />);
    expect(html).toContain('No logic in this POU');
    expectNoFabricatedIdentifiers(html, 'FbdCanvas');
  });

  it('sfc: shows no logic in this POU instead of a sample sequence', () => {
    const html = renderClient(<SfcCanvas />);
    expect(html).toContain('No logic in this POU');
    expectNoFabricatedIdentifiers(html, 'SfcCanvas');
  });

  it('variable table: shows no variables declared instead of sample declarations', () => {
    const html = renderClient(<VariableTableGrid />);
    expect(html).toContain('No variables declared');
    expect(html).not.toContain('%IX0.0');
    expectNoFabricatedIdentifiers(html, 'VariableTableGrid');
  });

  it('data types: shows no data types declared instead of DUT_ samples', () => {
    const html = renderClient(<DataTypeEditor />);
    expect(html).toContain('No data types declared');
    expectNoFabricatedIdentifiers(html, 'DataTypeEditor');
  });

  it('st/il: shows no program instead of a bottled-plant routine', () => {
    const html = renderClient(<MonacoStEditor />);
    expectNoFabricatedIdentifiers(html, 'MonacoStEditor');
    expect(html).not.toContain('END_PROGRAM');
  });

  it('the plugin invents nothing on any tab it can reach', () => {
    // The plugin hands these surfaces no data, so every tab must state its emptiness. This walks
    // every reachable tab rather than only the default one, because the default (ST) passes an
    // empty string as `value` and would hide a sample behind the textual editors.
    renderClient(<LogicEditorPlugin targetBlock="MainTask" />);

    const tabs = ['st', 'il', 'ladder', 'fbd', 'sfc', 'variables', 'datatypes'];
    for (const tab of tabs) {
      const button = container.querySelector(`[data-testid="tab-${tab}"]`) as HTMLButtonElement;
      expect(button, `tab ${tab} is missing`).toBeTruthy();
      flushSync(() => {
        button.click();
      });
      expectNoFabricatedIdentifiers(container.innerHTML, `LogicEditorPlugin / ${tab}`);
    }
  });
});
