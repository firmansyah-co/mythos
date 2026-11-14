import type { EditorNames } from '../data/derive';
import { useIoConfig } from '../data/ioConfig';
import { useScreens } from '../data/screens';

/** The current names of the things editors are scoped to, for titles that follow a rename. */
export function useEditorNames(): EditorNames {
  const draft = useIoConfig((state) => state.draft);
  const screens = useScreens((state) => state.screens);
  return {
    connection: (key) => draft.find((connection) => connection.key === key)?.id,
    screen: (id) => screens.find((screen) => screen.id === id)?.name,
  };
}
