// The Structured Text editor: a text area, with the same text painted beneath it in colour and
// a gutter of line numbers. The text area is the editor; the paint and the gutter only show.

import { forwardRef, useImperativeHandle, useMemo, useRef } from 'react';
import type { KeyboardEvent } from 'react';
import type { Diagnostic } from './bridge';
import { offsetOf, tokenize } from './st';

export interface SourceEditorHandle {
  /** Put the caret at a 1-based line and column, and the editor in focus. */
  goTo: (line: number, col: number) => void;
}

interface Props {
  source: string;
  readOnly: boolean;
  /** The diagnostics of this very text; none when the last check was of another. */
  diagnostics: Diagnostic[];
  onChange: (source: string) => void;
}

const INDENT = '  ';

export const SourceEditor = forwardRef<SourceEditorHandle, Props>(function SourceEditor(
  { source, readOnly, diagnostics, onChange },
  ref
) {
  const area = useRef<HTMLTextAreaElement>(null);
  const lines = useMemo(() => source.split('\n'), [source]);
  const tokens = useMemo(() => tokenize(source), [source]);
  const columns = useMemo(() => lines.reduce((widest, line) => Math.max(widest, line.length), 0), [lines]);

  // Where each diagnostic points, as an offset: the token there is underlined.
  const marked = useMemo(() => {
    const offsets = new Set<number>();
    for (const diagnostic of diagnostics) {
      const offset = offsetOf(source, diagnostic.line, diagnostic.col);
      if (offset !== null) offsets.add(offset);
    }
    return offsets;
  }, [diagnostics, source]);
  const byLine = useMemo(() => {
    const map = new Map<number, string[]>();
    for (const diagnostic of diagnostics) {
      map.set(diagnostic.line, [...(map.get(diagnostic.line) ?? []), diagnostic.message]);
    }
    return map;
  }, [diagnostics]);

  useImperativeHandle(ref, () => ({
    goTo: (line, col) => {
      const element = area.current;
      const offset = offsetOf(source, line, col);
      if (!element || offset === null) return;
      element.focus();
      element.setSelectionRange(offset, offset);
    },
  }));

  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    // Tab indents instead of leaving the editor; Escape then Tab leaves it, as editors do.
    if (event.key !== 'Tab' || event.shiftKey || readOnly) return;
    event.preventDefault();
    const element = event.currentTarget;
    const { selectionStart, selectionEnd } = element;
    onChange(source.slice(0, selectionStart) + INDENT + source.slice(selectionEnd));
    // After React has put the new text in, the caret goes behind the indent.
    queueMicrotask(() => element.setSelectionRange(selectionStart + INDENT.length, selectionStart + INDENT.length));
  };

  const painted = [];
  let key = 0;
  for (const token of tokens) {
    const at = [...marked].find((offset) => offset >= token.offset && offset < token.offset + token.text.length);
    if (at === undefined) {
      painted.push(
        token.kind === 'text' ? token.text : (
          <span key={key} className={`logic-tok logic-tok--${token.kind}`}>
            {token.text}
          </span>
        )
      );
    } else {
      // Underline from where the diagnostic points to the end of the word there.
      const from = at - token.offset;
      const rest = token.text.slice(from);
      const word = /^[A-Za-z0-9_]+|^[^\s]/.exec(rest)?.[0] ?? rest.slice(0, 1);
      painted.push(
        <span key={key} className={token.kind === 'text' ? undefined : `logic-tok logic-tok--${token.kind}`}>
          {token.text.slice(0, from)}
          <span className="logic-tok--error" data-testid="source-error-mark">
            {word}
          </span>
          {rest.slice(word.length)}
        </span>
      );
    }
    key += 1;
  }

  return (
    <div className="logic-source" data-testid="source-editor">
      <div className="logic-source__gutter" aria-hidden>
        {lines.map((_line, index) => {
          const messages = byLine.get(index + 1);
          return (
            <span
              key={index}
              className={messages ? 'logic-source__line has-error' : 'logic-source__line'}
              title={messages?.join('\n')}
              data-testid={messages ? 'source-error-line' : undefined}
            >
              {index + 1}
            </span>
          );
        })}
      </div>
      <div className="logic-source__body" style={{ minWidth: `${columns + 2}ch` }}>
        <pre className="logic-source__paint" aria-hidden>
          {painted}
          {'\n'}
        </pre>
        <textarea
          ref={area}
          className="logic-source__area"
          aria-label="Structured Text source"
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          wrap="off"
          rows={lines.length}
          value={source}
          readOnly={readOnly}
          onChange={(event) => onChange(event.target.value)}
          onKeyDown={onKeyDown}
        />
      </div>
    </div>
  );
});
