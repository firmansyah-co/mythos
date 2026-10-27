/**
 * Faceplate master-type types (TSK-309).
 *
 * The wire shapes are generated from the Rust microkernel by ts-rs (part B, PR #21) and are the
 * single source of truth; this module only re-exports them under a stable local import path.
 * Nothing here is hand-authored, and nothing here duplicates a generated shape.
 */

export type {
  UdtKind,
  UdtMember,
  UdtDefinition,
  BindingAccess,
  FaceplateBinding,
  FaceplateViewKind,
  FaceplateView,
  FaceplateType,
  FaceplateTypeSummary,
  InstanceBinding,
  FaceplateInstance,
  StrandedInstance,
  FaceplateReleaseResult,
  ResolvedInstance,
} from './generated';

/**
 * `version` is `0` until a type has been released: the backend assigns a real version on
 * release, so a draft-only type must never be presented as v0 to an engineer.
 */
export const versionLabel = (version: number | null | undefined): string =>
  version === null || version === undefined || version === 0 ? 'unreleased' : `v${version}`;