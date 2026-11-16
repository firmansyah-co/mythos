// How screen elements are drawn. Shared by the editor and the operator runtime, so an element
// looks and behaves the same in both. Each kind's drawing is in `elementViews`.

import type { ScreenElement } from './screenModel';
import { viewOf } from './elementViews';

export function ElementBody({ element }: { element: ScreenElement }) {
  if (element.type === 'unknown') return null;
  const { Body } = viewOf(element);
  return <Body element={element} />;
}
