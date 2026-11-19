import type { EditorNames } from '../data/derive';
import { useIoConfig } from '../data/ioConfig';

/** The current names of the things editors are scoped to, for titles that follow a rename. */
export function useEditorNames(): EditorNames {
  const draft = useIoConfig((state) => state.draft);
  return {
    connection: (key) => draft.find((connection) => connection.key === key)?.id,
  };
}
