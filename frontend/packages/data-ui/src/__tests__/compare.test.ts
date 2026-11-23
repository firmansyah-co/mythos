import { describe, expect, it } from 'vitest';
import type { BlockAnswer, RuntimeBlock, RuntimeInstance } from '../bridge';
import { compareBlock, comparisonText, goneBlocks, layoutText, NOT_KNOWN, retainOf } from '../model';

const A = '0123456789abcdef';
const B = 'fedcba9876543210';

const saved = (over: Partial<BlockAnswer> = {}): BlockAnswer => ({
  name: 'Line1',
  type: 'Recipe',
  retain: false,
  layout: A,
  size: 4,
  members: [{ name: 'SPEED', type: 'INT', offset: 0, size: 2 }],
  ...over,
});
// As the configuration file has it: `retain` absent when false.
const configured = (over: Partial<RuntimeBlock> = {}): RuntimeBlock => ({ name: 'Line1', type: 'Recipe', layout: A, size: 4, members: [], ...over });

describe('how a configured block compares with the saved one', () => {
  const table: Array<[string, BlockAnswer, RuntimeBlock, string]> = [
    ['all three equal: laid out as saved', saved(), configured(), 'laid out as saved'],
    ['the type in another case is the same type', saved(), configured({ type: 'RECIPE' }), 'laid out as saved'],
    ['another layout: both hashes', saved(), configured({ layout: B }), `built with layout ${B}, saved as ${A}`],
    ['another type: both names', saved(), configured({ type: 'Alarm' }), 'of type Alarm there, Recipe here'],
    ['retained there, not here', saved(), configured({ retain: true }), 'retained there, not here'],
    ['retain absent there is not retained', saved({ retain: true }), configured(), 'not retained there, retained here'],
    [
      'every difference is named',
      saved({ retain: true }),
      configured({ layout: B, type: 'Alarm' }),
      `built with layout ${B}, saved as ${A}; of type Alarm there, Recipe here; not retained there, retained here`,
    ],
  ];
  for (const [label, here, there, text] of table) {
    it(label, () => {
      expect(comparisonText(compareBlock(here, true, there))).toBe(text);
    });
  }

  it('names each difference with both values', () => {
    expect(compareBlock(saved({ retain: true }), true, configured({ layout: B, type: 'Alarm' }))).toEqual({
      kind: 'compared',
      differences: [
        { what: 'layout', there: B, here: A },
        { what: 'type', there: 'Alarm', here: 'Recipe' },
        { what: 'retain', there: false, here: true },
      ],
    });
  });

  it('never says "up to date": an initial value is outside the layout', () => {
    for (const [, here, there] of table) {
      expect(comparisonText(compareBlock(here, true, there))).not.toMatch(/up to date/i);
    }
  });

  it('when the saved state does not check, says the saved layout is NOT KNOWN and compares nothing', () => {
    // The module answers no layout then; a configured one that differs in everything is not called different.
    const unknown = saved({ layout: undefined, size: undefined, members: undefined });
    const c = compareBlock(unknown, false, configured({ layout: B, type: 'Alarm', retain: true }));
    expect(c).toEqual({ kind: 'not-known' });
    expect(comparisonText(c)).toBe(`the saved layout is ${NOT_KNOWN}: the project's types and data blocks do not check, so nothing is compared`);
    // Nor when it says it does not check and still had a layout from an earlier answer.
    expect(compareBlock(saved(), false, configured({ layout: B }))).toEqual({ kind: 'not-known' });
  });

  it('a block the project no longer has is gone, not different', () => {
    expect(compareBlock(null, true, configured())).toEqual({ kind: 'gone' });
    expect(comparisonText({ kind: 'gone' })).toBe('the project no longer has this data block');
  });
});

describe('what is read from a configuration and an answer', () => {
  it('retain absent in the configuration is false, its own default; only true is retained', () => {
    expect(retainOf(configured())).toBe(false);
    expect(retainOf(configured({ retain: false }))).toBe(false);
    expect(retainOf(configured({ retain: true }))).toBe(true);
  });

  it('a layout the module did not answer reads NOT KNOWN, never blank', () => {
    expect(layoutText(saved({ layout: undefined }))).toBe('NOT KNOWN');
    expect(layoutText(saved())).toBe(A);
  });

  it('lists each block a runtime carries that the project does not have, by runtime', () => {
    const instances: RuntimeInstance[] = [
      { id: 'press', data: [configured(), configured({ name: 'Old' })] },
      { id: 'oven' },
      { id: 'kiln', data: [configured({ name: 'LINE1' })] },
    ];
    expect(goneBlocks([saved()], instances)).toEqual([{ runtime: 'press', block: 'Old' }]);
    expect(goneBlocks([], instances)).toEqual([
      { runtime: 'press', block: 'Line1' },
      { runtime: 'press', block: 'Old' },
      { runtime: 'kiln', block: 'LINE1' },
    ]);
  });
});
