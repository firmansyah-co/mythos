// The mark of a forced value (ADR-013 §2.5 "Forcing"). An engineer may hold a variable of a
// running control program at a value; every tag bound to it then shows that value, not the
// program's and not the field's. Wherever such a tag's value is shown, this mark is shown with
// it, so an operator never takes a forced value for a measured or computed one.

/**
 * Whether `uri` is among the tags the controller says are forced. `null` (not known) is not
 * forced: nothing is claimed either way, so nothing is marked.
 */
export function isForced(uri: string, forcedTags: readonly string[] | null): boolean {
  return forcedTags !== null && uri !== '' && forcedTags.includes(uri);
}

const FORCED_TITLE =
  "An engineer forces the program variable bound to this tag: this value is not the program's (or the field's).";

/** `FORCED`, beside a tag's value, when the tag is bound to a variable an engineer forces. */
export function ForcedMark({ uri, forcedTags }: { uri: string; forcedTags: readonly string[] | null }) {
  return isForced(uri, forcedTags) ? <ForcedFlag /> : null;
}

/** The mark itself, for a surface that says something of several forced tags at once. */
export function ForcedFlag({ title = FORCED_TITLE }: { title?: string }) {
  return (
    <span className="badge badge--warn" data-testid="tag-forced" title={title}>
      FORCED
    </span>
  );
}
