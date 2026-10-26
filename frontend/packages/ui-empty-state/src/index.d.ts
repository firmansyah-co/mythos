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
export declare const EMPTY_SURFACE: {
    readonly logic: {
        readonly title: "No logic in this POU";
        readonly detail: "This editor shows only the program the project declares. None is loaded, so no rungs are drawn.";
        readonly testId: "empty-logic-state";
    };
    readonly variables: {
        readonly title: "No variables declared";
        readonly detail: "Declarations come from the project. None are loaded, so the table is empty rather than a sample.";
        readonly testId: "empty-variables-state";
    };
    readonly dataTypes: {
        readonly title: "No data types declared";
        readonly detail: "Data types come from the project. None are loaded, so no definitions are shown.";
        readonly testId: "empty-data-types-state";
    };
    readonly devices: {
        readonly title: "No devices discovered or configured";
        readonly detail: "Nothing has been discovered or added to this project. Add a device from the catalog to place it here.";
        readonly testId: "empty-devices-state";
    };
};
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
export declare const EmptySurface: React.FC<EmptySurfaceProps>;
//# sourceMappingURL=index.d.ts.map