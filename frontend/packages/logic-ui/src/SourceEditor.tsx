// The Structured Text editor: a text area, with the same text painted beneath it in colour and
// a gutter of line numbers. The text area is the editor; the paint and the gutter only show.

import { forwardRef, useImperativeHandle, useMemo, useRef } from 'react';
import type { KeyboardEvent } from 'react';
import type { Diagnostic } from './bridge';
import { offsetOf, rangeOf, tokenize } from './st';

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

  // What the diagnostics are about, as stretches of the text in order, overlapping ones
  // joined: each is underlined whole, over as many words and lines as it takes.
  const marked = useMemo(() => {
    const ranges = diagnostics
      .map((diagnostic) => rangeOf(source, diagnostic))
      .filter((range): range is { from: number; to: number } => range !== null)
      .sort((a, b) => a.from - b.from);
    const joined: { from: number; to: number }[] = [];
    for (const range of ranges) {
      const last = joined[joined.length - 1];
      if (last && range.from <= last.to) last.to = Math.max(last.to, range.to);
      else joined.push({ ...range });
    }
    return joined;
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
    const end = token.offset + token.text.length;
    const className = token.kind === 'text' ? undefined : `logic-tok logic-tok--${token.kind}`;
    const inside = marked.filter((range) => range.from < end && range.to > token.offset);
    if (inside.length === 0) {
      painted.push(
        className === undefined ? (
          token.text
        ) : (
          <span key={key} className={className}>
            {token.text}
          </span>
        )
      );
    } else {
      // The parts of the token a diagnostic is about are underlined; the rest as it is.
      const parts = [];
      let at = token.offset;
      for (const range of inside) {
        const from = Math.max(range.from, token.offset);
        const to = Math.min(range.to, end);
        if (from > at) parts.push(source.slice(at, from));
        parts.push(
          <span key={from} className="logic-tok--error" data-testid="source-error-mark">
            {source.slice(from, to)}
          </span>
        );
        at = to;
      }
      if (at < end) parts.push(source.slice(at, end));
      painted.push(
        <span key={key} className={className}>
          {parts}
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
