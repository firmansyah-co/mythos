/**
 * Faceplate master-type contracts (TSK-309).
 *
 * These mirror the shapes agreed with the part-B backend slice for the
 * project-scoped `/api/v1/projects/:id/faceplates` routes. They are deliberately
 * NOT under `types/generated/`: the ts-rs bindings land from the backend slice and
 * replace this file. Until then this is the single local definition.
 */

export type FaceplateViewKind = 'PID_SYMBOL' | 'SUMMARY_CARD' | 'DIAGNOSTIC_POPUP' | 'ALARM';

export type FaceplateAccess = 'READ_ONLY' | 'READ_WRITE';

export type UdtKind = 'STRUCT' | 'ENUM' | 'ALIAS';

export interface UdtMember {
  name: string;
  dataType: string;
}

/**
 * The master type carries its UDT structurally. There is no project-scoped
 * data-type registry in the backend yet, so a named type reference would be a
 * free-form string; the server validates every view binding against these members.
 */
export interface UdtDefinition {
  name: string;
  kind: UdtKind;
  members: UdtMember[];
  enumConstants?: string[];
  baseType?: string;
}

export interface FaceplateViewBinding {
  faceplateKey: string;
  udtProperty: string;
  access: FaceplateAccess;
  unit?: string;
}

export interface FaceplateView {
  kind: FaceplateViewKind;
  bindings: FaceplateViewBinding[];
}

/** A master faceplate type: identity, the UDT it binds to, and its archetype views. */
export interface FaceplateType {
  id: string;
  name: string;
  version: string;
  udt: UdtDefinition;
  views: FaceplateView[];
  /** Release bookkeeping carried by the list route. */
  activeVersion?: string | null;
  hasDraft?: boolean;
  instanceCount?: number;
}

export interface FaceplateInstanceBinding {
  faceplateKey: string;
  tagUri: string;
}

/**
 * An instance stores no rendering data. `typeVersion` null means follow the
 * active master (resolved at read time); a number freezes the instance.
 */
export interface FaceplateInstance {
  id: string;
  name: string;
  typeId: string;
  typeVersion: number | null;
  bindings: FaceplateInstanceBinding[];
}

/** GET /instances resolves each instance against its master at read time. */
export interface ResolvedFaceplateInstance extends FaceplateInstance {
  resolvedVersion: string | null;
  frozen: boolean;
  /** null when the frozen version is missing from history: render nothing, never a placeholder. */
  udt: UdtDefinition | null;
  views: FaceplateView[] | null;
  warnings: string[];
}

export interface StrandedFaceplateInstance {
  id: string;
  name: string;
  typeVersion: number;
}

export interface FaceplateReleaseResult {
  id: string;
  version: string;
  instancesAffected: number;
  /** Frozen instances that stayed on their pinned version after this release. */
  strandedInstances: StrandedFaceplateInstance[];
}