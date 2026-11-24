// The project library's blocks as the LD and FBD editors take them (libraryblocks.ts): each
// answer of the module's check of the stored library, mapped to boxes or to the one line that
// says why there are none; an entry of another shape; what a box whose block changed is told;
// and a library box placed as a document POU of the same declaration is placed.

import { describe, expect, it } from 'vitest';
import { newBox } from '../fbdedit';
import { FBD_KIT } from '../FbdInstructions';
import type { BoxDef } from '../ldcatalog';
import { newElement } from '../ldedit';
import { callable, LD_KIT } from '../LdInstructions';
import { open as openLd, ownBoxes } from '../ldmodel';
import { open as openFbd } from '../fbdmodel';
import {
  BEING_CHECKED,
  editorDoesNotKnow,
  libraryBoxes,
  NOT_ANSWERED,
  OTHER_SHAPE,
  pinProblems,
  unknownBoxWords,
  type LibraryBoxes,
} from '../libraryblocks';
import type { LibraryCheck } from '../library';
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
});

describe('a box the editor does not find', () => {
  it('with the library known, is said to be declared nowhere', () => {
    expect(unknownBoxWords('Pump', { boxes: [], why: null })).toEqual([
      "Pump is not a function or function block of this program, of the project's library or of the standard ones.",
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
