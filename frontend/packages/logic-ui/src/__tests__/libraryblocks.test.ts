// The project library's blocks as the LD and FBD editors take them (libraryblocks.ts): each
// answer of the module's check of the stored library, mapped to boxes or to the one line that
// says why there are none; an entry of another shape; what a box whose block changed is told;
// and a library box placed as a document POU of the same declaration is placed.

/// <reference types="node" />
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { check as checkFbd, insert as insertFbd, newBox, orAt, setSinkText } from '../fbdedit';
import { FBD_KIT } from '../FbdInstructions';
import { findBox, type BoxDef } from '../ldcatalog';
import { check as checkLd, newElement, setPin } from '../ldedit';
import { callable, LD_KIT } from '../LdInstructions';
import { open as openLd, ownBoxes } from '../ldmodel';
import { open as openFbd } from '../fbdmodel';
import {
  BEING_CHECKED,
  editorDoesNotKnow,
  libraryBoxes,
  NAMES_UNREAD,
  NOT_ANSWERED,
  OTHER_SHAPE,
  pinProblems,
  unknownBoxWords,
  type LibraryBoxes,
} from '../libraryblocks';
import type { LibraryBlock, LibraryCheck } from '../library';
import { checksWith, MOTOR, motorLibrary, SCALE } from './libraryanswer';

const answered = (answer: LibraryCheck) => libraryBoxes({ answer, error: null });
const DIAGNOSTIC = { line: 1, col: 1, end_line: 1, end_col: 2, message: 'expected END_FUNCTION' };

describe('the library check answered', () => {
  it('maps a function block and a function to boxes with the answered pins, in order, names upper-cased', () => {
    expect(answered(checksWith(MOTOR, SCALE))).toEqual({
      why: null,
      boxes: [
        {
          name: 'Motor',
          kind: 'functionBlock',
          inputs: [
            { name: 'RUN', type: 'BOOL' },
            { name: 'SPEED', type: 'INT' },
          ],
          outputs: [
            { name: 'RUNNING', type: 'BOOL' },
            { name: 'RPM', type: 'INT' },
          ],
          group: 'Library',
        },
        {
          name: 'Scale',
          kind: 'function',
          inputs: [
            { name: 'RAW', type: 'INT' },
            { name: 'FACTOR', type: 'INT' },
          ],
          outputs: [{ name: 'OUT', type: 'INT' }],
          group: 'Library',
        },
      ],
    });
  });

  it('keeps a type as the module wrote it, a string length and a structure among them', () => {
    const known = answered(
      checksWith({ name: 'Label', kind: 'function', inputs: [{ name: 'at', type: 'Point' }], outputs: [], result: 'STRING[12]' })
    );
    expect(known.boxes?.[0]?.inputs).toEqual([{ name: 'AT', type: 'Point' }]);
    expect(known.boxes?.[0]?.outputs).toEqual([{ name: 'OUT', type: 'STRING[12]' }]);
  });

  it('an empty list is a library that declares no block, not one that is not known', () => {
    expect(answered(checksWith())).toEqual({ boxes: [], why: null });
  });
});

describe('a list that is not known is said, never shown as one', () => {
  it('while no check has answered', () => {
    expect(libraryBoxes(null)).toEqual({ boxes: null, why: BEING_CHECKED });
  });

  it('when the list of the library could not be read, no check is said to be under way', () => {
    // The store asks no check of the library when its list could not be read.
    expect(libraryBoxes(null, 'logic/library: Permission denied (os error 13)')).toEqual({
      boxes: null,
      why: 'The library could not be read: logic/library: Permission denied (os error 13); its blocks are not offered.',
    });
    // A check that answered is what is said, whatever the list's read gave.
    expect(libraryBoxes({ answer: checksWith(MOTOR), error: null }, 'refused').boxes?.map((d) => d.name)).toEqual(['Motor']);
  });

  it('when the check could not be made, in the words given', () => {
    expect(libraryBoxes({ answer: null, error: 'the controller did not answer' })).toEqual({
      boxes: null,
      why: 'The library could not be checked: the controller did not answer; its blocks are not offered.',
    });
  });

  it('when the library does not check, with the count of diagnostics, even if blocks came with it', () => {
    const fails: LibraryCheck = { ok: false, diagnostics: [DIAGNOSTIC, DIAGNOSTIC], blocks: [MOTOR] };
    expect(answered(fails)).toEqual({
      boxes: null,
      why: 'The library does not check (2 diagnostics); its blocks are offered once it does.',
    });
  });

  it('when an answer that checks has no blocks', () => {
    expect(answered({ ok: true, diagnostics: [] })).toEqual({ boxes: null, why: NOT_ANSWERED });
  });

  it('every entry of another shape makes the whole list unknown', () => {
    const bad: unknown[] = [
      null,
      'Motor',
      { ...MOTOR, name: '' },
      { ...MOTOR, kind: 'program' },
      { ...MOTOR, result: 'BOOL' },
      { ...SCALE, result: null },
      { ...SCALE, outputs: [{ name: 'extra', type: 'INT' }] },
      { ...MOTOR, inputs: [{ name: 'run' }] },
      { ...MOTOR, inputs: [{ name: 'run', type: '' }] },
      { ...MOTOR, outputs: undefined },
    ];
    for (const entry of bad) {
      // A good block before it is not offered alone.
      const answer = { ok: true, diagnostics: [], blocks: [SCALE, entry] } as unknown as LibraryCheck;
      expect(answered(answer), JSON.stringify(entry)).toEqual({ boxes: null, why: OTHER_SHAPE });
    }
  });

  it('an answer that names a block like a standard one offers no block, so the standard one is not shadowed', () => {
    const ton: LibraryBlock = { name: 'ton', kind: 'function_block', inputs: [{ name: 'go', type: 'INT' }], outputs: [], result: null };
    const known = answered(checksWith(SCALE, ton));
    expect(known).toEqual({ boxes: null, why: NAMES_UNREAD });
    expect(findBox('TON', callable([], known))?.inputs.map((p) => p.name)).toEqual(['IN', 'PT']);
  });

  it('an answer that names one block twice, in any case, offers no block', () => {
    expect(answered(checksWith(MOTOR, SCALE, { ...MOTOR, name: 'MOTOR', inputs: [] }))).toEqual({ boxes: null, why: NAMES_UNREAD });
  });
});

describe('a box the editor does not find', () => {
  it('with the library known, is said to be declared nowhere', () => {
    expect(unknownBoxWords('Pump', { boxes: [], why: null })).toEqual([
      "Pump is not a function or function block of this program or of the project's library, nor a standard one this editor knows.",
    ]);
  });

  it('with the library not known, keeps the words of D4c and says why the library is not known', () => {
    expect(unknownBoxWords('Pump', { boxes: null, why: BEING_CHECKED })).toEqual([
      editorDoesNotKnow('Pump'),
      "The project's library's blocks are not known here: the library is being checked; its blocks are offered once it answers.",
    ]);
  });

  it('with no library given, keeps the words of D4c alone', () => {
    expect(unknownBoxWords('Pump')).toEqual([editorDoesNotKnow('Pump')]);
  });
});

describe('a box whose block was declared again', () => {
  const motor = motorLibrary()[0] as BoxDef;
  const pins = (...formals: string[]) => formals.map((formal) => ({ formal }));

  it('says nothing while the box has the declared pins, EN and ENO aside', () => {
    expect(pinProblems(pins('EN', 'RUN', 'SPEED'), pins('ENO', 'RUNNING', 'RPM'), motor)).toEqual([]);
  });

  it('says a pin the declaration lost, an input it gained, and both of a renamed one', () => {
    expect(pinProblems(pins('RUN', 'SPEED', 'MODE'), pins('RUNNING', 'RPM', 'TORQUE'), motor)).toEqual([
      'MODE is not a pin of Motor as it is declared now.',
      'TORQUE is not a pin of Motor as it is declared now.',
    ]);
    expect(pinProblems(pins('RUN'), pins('RUNNING', 'RPM'), motor)).toEqual(['Motor declares the input SPEED, which this box does not have.']);
    expect(pinProblems(pins('RUN', 'VELOCITY'), pins('RUNNING', 'RPM'), motor)).toEqual([
      'VELOCITY is not a pin of Motor as it is declared now.',
      'Motor declares the input SPEED, which this box does not have.',
    ]);
  });

  it('compares a pin another tool wrote in lower case by name, not by spelling', () => {
    expect(pinProblems(pins('run', 'speed'), pins('running', 'rpm'), motor)).toEqual([]);
  });
});

/** Motor and Scale declared in the document itself, as the library declares them. */
const DOCUMENT = `<?xml version="1.0" encoding="utf-8"?>
<project xmlns="http://www.plcopen.org/xml/tc6_0201">
  <types><dataTypes/><pous>
    <pou name="Motor" pouType="functionBlock">
      <interface>
        <inputVars><variable name="run"><type><BOOL/></type></variable><variable name="speed"><type><INT/></type></variable></inputVars>
        <outputVars><variable name="running"><type><BOOL/></type></variable><variable name="rpm"><type><INT/></type></variable></outputVars>
      </interface>
      <body><ST><xhtml:p xmlns:xhtml="http://www.w3.org/1999/xhtml">running := run;</xhtml:p></ST></body>
    </pou>
    <pou name="Scale" pouType="function">
      <interface>
        <returnType><INT/></returnType>
        <inputVars><variable name="raw"><type><INT/></type></variable><variable name="factor"><type><INT/></type></variable></inputVars>
      </interface>
      <body><ST><xhtml:p xmlns:xhtml="http://www.w3.org/1999/xhtml">Scale := raw * factor;</xhtml:p></ST></body>
    </pou>
  </pous></types>
  <instances><configurations/></instances>
</project>
`;

describe('a library block is placed as a document POU of the same declaration is', () => {
  it('in a Ladder Diagram: the same pins, power pins and types', () => {
    const opened = openLd(DOCUMENT);
    if (!opened.ok) throw new Error(opened.error);
    const document = ownBoxes(opened.source);
    for (const type of ['Motor', 'Scale']) {
      expect(newElement({ kind: 'box', type }, 7, motorLibrary())).toEqual(newElement({ kind: 'box', type }, 7, document));
    }
  });

  it('in a Function Block Diagram: the same pins and types, no EN or ENO', () => {
    const opened = openFbd(DOCUMENT);
    if (!opened.ok) throw new Error(opened.error);
    const document = ownBoxes(opened.source);
    for (const type of ['Motor', 'Scale']) {
      expect(newBox(type, 7, motorLibrary())).toEqual(newBox(type, 7, document));
    }
  });
});

describe('the palette groups', () => {
  const known: LibraryBoxes = { boxes: motorLibrary(), why: null };
  const document: BoxDef = { name: 'Valve', kind: 'functionBlock', inputs: [], outputs: [], group: 'This document' };

  it('put the library between the standard groups and the document, in each kit', () => {
    for (const kit of [LD_KIT, FBD_KIT]) {
      const groups = kit.groups(callable([document], known), known);
      expect(groups.map(([title]) => title).slice(-3), kit.prefix).toEqual(['Library', 'This document', 'Program control']);
      const library = groups.find(([title]) => title === 'Library');
      expect(library?.[1].map((x) => x.id)).toEqual(['box:Motor', 'box:Scale']);
      expect(library?.[1].map((x) => x.name)).toEqual(['Motor (function block)', 'Scale (function)']);
      expect(groups.find(([title]) => title === 'This document')?.[1].map((x) => x.id)).toEqual(['box:Valve']);
    }
  });
});

const FIXTURES = resolve(__dirname, '../../../../../crates/mythos-logic-compiler/tests');
/** What the editors wrote with Motor and Scale, compiled and run by the compiler's tests. */
const LD_WRITTEN = readFileSync(resolve(FIXTURES, 'ladder_library/editor_library.xml'), 'utf8');
const FBD_WRITTEN = readFileSync(resolve(FIXTURES, 'fbd_library/editor_library.xml'), 'utf8');
const everything = (p: { elements: Map<number, string[]>; networks: Map<number, string[]> }) => [...p.elements.values(), ...p.networks.values()].flat();

/** What the LD editor's check says of the first POU of `text`, with `own` and the library `known`. */
function ldSaid(text: string, known: LibraryBoxes, own: BoxDef[] = known.boxes ?? []) {
  const o = openLd(text);
  if (!o.ok) throw new Error(o.error);
  const i = o.source.pous.findIndex((x) => x.name === 'Main');
  return everything(checkLd(o.model.pous[i] as never, o.source.pous[i]?.vars ?? [], own, known));
}
function fbdSaid(text: string, known: LibraryBoxes, own: BoxDef[] = known.boxes ?? []) {
  const o = openFbd(text);
  if (!o.ok) throw new Error(o.error);
  const i = o.source.pous.findIndex((x) => x.name === 'Main');
  return everything(checkFbd(o.model.pous[i] as never, o.source.pous[i]?.vars ?? [], own, known));
}

describe('a box of a standard block the compiler declares and this editor does not hold', () => {
  // CTU_DINT and TON_TIME are standard blocks of the compiler (mythos-logic-st library); the
  // editor's table does not hold them, so it cannot say they are not standard.
  const known = answered(checksWith(SCALE));
  it('in a Ladder Diagram, is not said to be no standard block', () => {
    expect(ldSaid(LD_WRITTEN.replaceAll('Motor', 'CTU_DINT'), known)).toEqual([
      "CTU_DINT is not a function or function block of this program or of the project's library, nor a standard one this editor knows.",
    ]);
  });
  it('in a Function Block Diagram, is not said to be no standard block', () => {
    expect(fbdSaid(FBD_WRITTEN.replaceAll('Motor', 'TON_TIME'), known)).toEqual([
      "TON_TIME is not a function or function block of this program or of the project's library, nor a standard one this editor knows.",
    ]);
  });
});

describe('a library block that declares a pin named EN', () => {
  it('gets each formal parameter once in a Ladder Diagram, EN being the box\'s enable', () => {
    const gate: LibraryBlock = {
      name: 'Gate',
      kind: 'function_block',
      inputs: [
        { name: 'x', type: 'INT' },
        { name: 'EN', type: 'BOOL' },
      ],
      outputs: [
        { name: 'q', type: 'INT' },
        { name: 'ENO', type: 'BOOL' },
      ],
      result: null,
    };
    const el = newElement({ kind: 'box', type: 'Gate' }, 7, answered(checksWith(gate)).boxes ?? []);
    expect(el.inputs.map((p) => p.formal)).toEqual(['EN', 'X']);
    expect(el.outputs.map((p) => p.formal)).toEqual(['ENO', 'Q']);
  });
});

describe('a library block changed under a diagram opened from its text', () => {
  it('Join with OR at a library pin of another family is refused, as at a box placed now', () => {
    const own = motorLibrary();
    const o = openFbd(FBD_WRITTEN);
    if (!o.ok) throw new Error(o.error);
    const ids = { next: o.source.firstFree, used: new Set<number>() };
    // Scale (box 6) is read from the text: its pins were typed when no library was known.
    const raw = { kind: 'pin', pou: 0, net: 1, box: 6, formal: 'RAW' } as const;
    const nested = insertFbd(o.model, ids, raw, { kind: 'box', type: 'ADD' }, own);
    if (!nested.ok) throw new Error(nested.error);
    const ored = orAt(nested.model, ids, raw, own);
    expect(ored.ok ? 'inserted' : ored.error).toBe('An OR joins BOOL signals; RAW takes a INT.');
  });

  it('LD: a power pin the declaration retyped is said', () => {
    const retyped = answered(checksWith({ ...MOTOR, inputs: [{ name: 'run', type: 'INT' }, MOTOR.inputs[1] as never] }, SCALE));
    expect(ldSaid(LD_WRITTEN, retyped)).toEqual(["RUN carries the rung's power, a BOOL; Motor declares it INT now."]);
  });

  it('a function block declared a function now: the instance the box keeps is said', () => {
    const fn = answered(checksWith({ name: 'Motor', kind: 'function', inputs: MOTOR.inputs, outputs: [], result: 'BOOL' }, SCALE));
    const words = [
      'RUNNING is not a pin of Motor as it is declared now.',
      'RPM is not a pin of Motor as it is declared now.',
      'Motor is a function as it is declared now, which has no instance; this box still names motor1.',
    ];
    expect(ldSaid(LD_WRITTEN, fn)).toEqual(words);
    expect(fbdSaid(FBD_WRITTEN, fn)).toEqual(words);
  });
});

describe('the declaration a box is checked against', () => {
  const known = answered(checksWith(MOTOR, SCALE));
  it('LD: an operand at a library output is checked against the output\'s declared type', () => {
    const o = openLd(LD_WRITTEN);
    if (!o.ok) throw new Error(o.error);
    const ids = { next: o.source.firstFree, used: new Set<number>() };
    const motor = (o.model.pous[0]?.networks ?? []).flatMap((n) => n.rung ?? []).find((x) => x.t === 'el' && x.el.typeName === 'Motor');
    if (motor?.t !== 'el') throw new Error('no Motor');
    const e = setPin(o.model, ids, 0, motor.el.id, 'outputs', 'RPM', 'done');
    if (!e.ok) throw new Error(e.error);
    expect(everything(checkLd(e.model.pous[0] as never, o.source.pous[0]?.vars ?? [], known.boxes ?? [], known))).toEqual([
      'RPM: done is BOOL; this takes a INT.',
    ]);
  });

  it('FBD: a library output assigned to a variable is checked against the output\'s declared type', () => {
    const o = openFbd(FBD_WRITTEN);
    if (!o.ok) throw new Error(o.error);
    const ids = { next: o.source.firstFree, used: new Set<number>() };
    // The assignment of RPM (5) made to the BOOL `running`.
    const e = setSinkText(o.model, ids, 0, 0, 5, 'running');
    if (!e.ok) throw new Error(e.error);
    expect(everything(checkFbd(e.model.pous[0] as never, o.source.pous[0]?.vars ?? [], known.boxes ?? [], known))).toEqual([
      'running is BOOL; this takes a INT.',
    ]);
  });

  it('a box of the document\'s own POU whose declaration lost an input says so', () => {
    // Motor declared by the document itself, without speed; the box still has SPEED.
    const motorPou = `<pou name="Motor" pouType="functionBlock"><interface><inputVars><variable name="run"><type><BOOL/></type></variable></inputVars><outputVars><variable name="running"><type><BOOL/></type></variable><variable name="rpm"><type><INT/></type></variable></outputVars></interface><body><ST><xhtml:p xmlns:xhtml="http://www.w3.org/1999/xhtml">running := run;</xhtml:p></ST></body></pou>`;
    for (const [written, said] of [
      [LD_WRITTEN, ldSaid],
      [FBD_WRITTEN, fbdSaid],
    ] as const) {
      const text = written.replace('<pous>', `<pous>\n      ${motorPou}`);
      const o = openLd(text);
      if (!o.ok) throw new Error(o.error);
      const scaleOnly = answered(checksWith(SCALE));
      const own = [...ownBoxes(o.source).filter((d) => d.name !== 'Main'), ...(scaleOnly.boxes ?? [])];
      expect(said(text, scaleOnly, own)).toEqual(['SPEED is not a pin of Motor as it is declared now.']);
    }
  });
});

describe('a library block named like one of the document\'s own', () => {
  const known = answered(checksWith(MOTOR, SCALE));
  const documentMotor: BoxDef = { name: 'Motor', kind: 'functionBlock', inputs: [{ name: 'X', type: 'INT' }], outputs: [{ name: 'DONE', type: 'BOOL' }], group: 'This document' };

  it('is not offered in the Library group, which names it, and every lookup finds the document\'s', () => {
    for (const kit of [LD_KIT, FBD_KIT]) {
      const own = callable([documentMotor], known);
      expect(own.map((d) => `${d.group}:${d.name}`)).toEqual(['This document:Motor', 'Library:Scale']);
      const library = kit.groups(own, known).find(([title]) => title === 'Library');
      expect(library?.[1].map((x) => x.id), kit.prefix).toEqual(['box:Scale']);
      expect(library?.[3], kit.prefix).toBe('Not offered: Motor, which this document declares too.');
      expect(findBox('Motor', own)).toBe(documentMotor);
    }
  });

  it('a box of the document\'s block says the library declares the same name', () => {
    const motorPou = `<pou name="Motor" pouType="functionBlock"><interface><inputVars><variable name="run"><type><BOOL/></type></variable><variable name="speed"><type><INT/></type></variable></inputVars><outputVars><variable name="running"><type><BOOL/></type></variable><variable name="rpm"><type><INT/></type></variable></outputVars></interface><body><ST><xhtml:p xmlns:xhtml="http://www.w3.org/1999/xhtml">running := run;</xhtml:p></ST></body></pou>`;
    for (const [written, said] of [
      [LD_WRITTEN, ldSaid],
      [FBD_WRITTEN, fbdSaid],
    ] as const) {
      const text = written.replace('<pous>', `<pous>\n      ${motorPou}`);
      const o = openLd(text);
      if (!o.ok) throw new Error(o.error);
      const own = callable(ownBoxes(o.source).filter((d) => d.name !== 'Main'), known);
      expect(said(text, known, own)).toEqual(["Motor is declared both by this document and by the project's library; this box has this document's pins."]);
    }
  });
});
