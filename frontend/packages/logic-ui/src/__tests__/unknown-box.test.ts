// A box naming a function or function block the LD or FBD editor does not know (D4c stage 2,
// ADR-014): every program is compiled with the project's library, which the editors' catalogues
// do not hold yet, so the editor says what it does not know, never that the program cannot call
// it. Only the words changed: the box is still marked, as before.

import { describe, expect, it } from 'vitest';
import { check as checkFbd } from '../fbdedit';
import { open as openFbd } from '../fbdmodel';
import { check as checkLd } from '../ldedit';
import { open as openLd } from '../ldmodel';
import { fbdOf, ladderOf } from './harness';

const SAID = (name: string) =>
  `${name} is not a function or function block this editor knows: it knows this program's own and the standard ones. If the project's library declares it, Check compiles the program with it.`;

const INTERFACE = '<interface><localVars><variable name="m"><type><derived name="Motor"/></type></variable><variable name="run"><type><BOOL/></type></variable></localVars></interface>';

describe('a box the editor does not know', () => {
  it('in a Ladder Diagram is said to be unknown to the editor, not uncallable', () => {
    const text = ladderOf(
      'Main',
      `<block localId="3" typeName="Motor" instanceName="m"><position x="100" y="0"/>
         <inputVariables><variable formalParameter="run"><connectionPointIn><connection refLocalId="1"/></connectionPointIn></variable></inputVariables>
         <inOutVariables/><outputVariables/></block>
       <coil localId="4"><position x="300" y="0"/><connectionPointIn><connection refLocalId="1"/></connectionPointIn><variable>run</variable></coil>`,
      INTERFACE
    );
    const opened = openLd(text);
    if (!opened.ok) throw new Error(opened.error);
    const pou = opened.model.pous[0];
    if (!pou) throw new Error('no POU');
    const said = [...checkLd(pou, opened.source.pous[0]?.vars ?? [], []).elements.values()].flat();
    expect(said).toContain(SAID('Motor'));
    expect(said.some((s) => s.includes('this program can call'))).toBe(false);
  });

  it('in a Function Block Diagram is said to be unknown to the editor, not uncallable', () => {
    const text = fbdOf(
      'Main',
      `<inVariable localId="1" width="40" height="20"><position x="0" y="0"/><connectionPointOut/><expression>run</expression></inVariable>
       <block localId="2" typeName="Motor" instanceName="m" width="80" height="60"><position x="100" y="0"/>
         <inputVariables><variable formalParameter="run"><connectionPointIn><connection refLocalId="1"/></connectionPointIn></variable></inputVariables>
         <inOutVariables/><outputVariables><variable formalParameter="Q"><connectionPointOut/></variable></outputVariables></block>
       <outVariable localId="3" width="40" height="20"><position x="300" y="0"/><connectionPointIn><connection refLocalId="2" formalParameter="Q"/></connectionPointIn><expression>run</expression></outVariable>`,
      INTERFACE
    );
    const opened = openFbd(text);
    if (!opened.ok) throw new Error(opened.error);
    const pou = opened.model.pous[0];
    if (!pou) throw new Error('no POU');
    const said = [...checkFbd(pou, opened.source.pous[0]?.vars ?? [], []).elements.values()].flat();
    expect(said).toContain(SAID('Motor'));
    expect(said.some((s) => s.includes('this program can call'))).toBe(false);
  });
});
