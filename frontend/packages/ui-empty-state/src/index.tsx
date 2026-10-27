/**
 * One contract for "this surface has no authored content to show".
 *
 * SYS-REQ-096 (Zero Silent Fallback). A workbench surface that invents content when its data is
 * missing is worse than one that shows nothing: an engineer reads a ladder rung, a variable
 * declaration or a commissioned PLC rack as fact about their own plant. Four surfaces did exactly
 * that through their own `DEFAULT_*` parameter defaults, each with its own wording and styling.
 *
 * This module is the single place that decides what "nothing" looks like, so the surfaces cannot
 * drift apart again. It deliberately says nothing about *process values* — a bound tag with no
 * telemetry is a different condition with its own quality semantics (see the faceplate library's
 * process-data helpers). This is about content the project never authored.
 *
 * The rule the consumers follow: no data means empty, never a default. Do not add a parameter
 * default that restores a sample program, sample declaration or sample device.
 */

import React from 'react';

export interface EmptySurfaceCopy {
  /** What is absent, in the surface's own vocabulary. */
  readonly title: string;
  /** Why it is absent, and what would put something there. Never a fabricated value. */
  readonly detail: string;
  /** Stable hook for the negative test that asserts nothing fabricated rendered instead. */
  readonly testId: string;
}

export const EMPTY_SURFACE = {
  logic: {
    title: 'No logic in this POU',
    detail:
      'This editor shows only the program the project declares. None is loaded, so no rungs are drawn.',
    testId: 'empty-logic-state',
  },
  variables: {
    title: 'No variables declared',
    detail:
      'Declarations come from the project. None are loaded, so the table is empty rather than a sample.',
    testId: 'empty-variables-state',
  },
  dataTypes: {
    title: 'No data types declared',
    detail:
      'Data types come from the project. None are loaded, so no definitions are shown.',
    testId: 'empty-data-types-state',
  },
  devices: {
    title: 'No devices discovered or configured',
    detail:
      'Nothing has been discovered or added to this project. Add a device from the catalog to place it here.',
    testId: 'empty-devices-state',
  },
} as const satisfies Record<string, EmptySurfaceCopy>;

export interface EmptySurfaceProps {
  copy: EmptySurfaceCopy;
  className?: string;
}

/**
 * Quiescent "nothing here" surface.
 *
 * Presentation is deliberately neutral: an empty program is a normal state, not a fault, so it
 * carries no alarm colour. Only the absence itself is stated.
 */
export const EmptySurface: React.FC<EmptySurfaceProps> = ({ copy, className = '' }) => (
  <div
    data-testid={copy.testId}
    className={`flex flex-col items-center justify-center gap-1 h-full w-full min-h-[120px] p-6 text-center border border-dashed border-neutral-700 rounded select-none ${className}`}
  >
    <div className="text-xs font-semibold uppercase tracking-wide text-neutral-400">
      {copy.title}
    </div>
    <div className="text-[11px] font-mono text-neutral-500 max-w-md">{copy.detail}</div>
  </div>
);
