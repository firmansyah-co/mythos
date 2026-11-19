// An XML document read into elements that know where they are in its text (ADR-013 §2.15,
// L3b-2b). The Ladder Diagram editor writes a TC6 document back by splicing: it replaces the
// bytes of what it changed and copies every other byte as it found it, so the file header,
// the other POUs and whatever it does not draw stay exactly as another tool wrote them. A DOM
// parser cannot do that: it keeps no offsets, and serialising its tree rewrites quotes, empty
// elements and namespace declarations.
//
// What is read: elements with their attributes and the character data directly inside them,
// references resolved; a comment inside an element, as a child named `#comment`, so that it is
// copied with the rest; processing instructions, CDATA and a DOCTYPE are stepped over (CDATA
// counts as text). It is not a validating parser: what is not well formed is
// refused with where, and the editor then shows the document read-only.

export interface XAttr {
  /** As written, with any prefix. */
  qname: string;
  /** Without a prefix. */
  name: string;
  value: string;
}

export interface XNode {
  /** The local name, without a prefix. */
  name: string;
  /** The name as written, with its prefix. */
  qname: string;
  attrs: XAttr[];
  children: XNode[];
  /** The character data directly inside it, references resolved. */
  text: string;
  /** Offset of its `<`. */
  start: number;
  /** Offset just after the `>` of its start tag. */
  openEnd: number;
  /** Offset of the `<` of its end tag; `openEnd` when it is empty (`<x/>`). */
  closeStart: number;
  /** Offset just after the whole element. */
  end: number;
  selfClosing: boolean;
}

export type Parsed = { ok: true; root: XNode } | { ok: false; error: string };

const ENTITIES: Record<string, string> = { lt: '<', gt: '>', amp: '&', quot: '"', apos: "'" };

/** Character references and the five predefined entities resolved. */
export function decode(text: string): string {
  return text.replace(/&(#x[0-9a-fA-F]+|#[0-9]+|[a-zA-Z]+);/g, (all, ref: string) => {
    if (ref.startsWith('#x')) return String.fromCodePoint(parseInt(ref.slice(2), 16));
    if (ref.startsWith('#')) return String.fromCodePoint(parseInt(ref.slice(1), 10));
    return ENTITIES[ref] ?? all;
  });
}

/** Text as element content. */
export const escapeText = (text: string) => text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

/** Text as an attribute value in double quotes. */
export const escapeAttr = (text: string) =>
  escapeText(text).replace(/"/g, '&quot;').replace(/\n/g, '&#10;').replace(/\r/g, '&#13;').replace(/\t/g, '&#9;');

const local = (qname: string) => qname.slice(qname.indexOf(':') + 1);

/** Line and column (from 1) of an offset, for what is said about the document. */
function where(src: string, at: number): string {
  const before = src.slice(0, at);
  const line = before.split('\n').length;
  return `line ${line}, column ${at - before.lastIndexOf('\n')}`;
}

/** Read `src`. */
export function parseXml(src: string): Parsed {
  const stack: XNode[] = [];
  let root: XNode | null = null;
  let i = 0;
  const fail = (what: string, at: number): Parsed => ({ ok: false, error: `${what} at ${where(src, at)}` });
  while (i < src.length) {
    const lt = src.indexOf('<', i);
    const textEnd = lt < 0 ? src.length : lt;
    if (textEnd > i) {
      const text = src.slice(i, textEnd);
      const top = stack[stack.length - 1];
      if (top) top.text += decode(text);
      else if (text.trim() !== '') return fail('text outside the root element', i);
    }
    if (lt < 0) break;
    if (src.startsWith('<!--', lt)) {
      const close = src.indexOf('-->', lt + 4);
      if (close < 0) return fail('a comment that does not end', lt);
      // Kept as a child named `#comment`, so a body written back keeps it.
      const top = stack[stack.length - 1];
      const end = close + 3;
      if (top) {
        top.children.push({ name: '#comment', qname: '#comment', attrs: [], children: [], text: '', start: lt, openEnd: end, closeStart: end, end, selfClosing: true });
      }
      i = end;
      continue;
    }
    if (src.startsWith('<![CDATA[', lt)) {
      const close = src.indexOf(']]>', lt + 9);
      if (close < 0) return fail('a CDATA section that does not end', lt);
      const top = stack[stack.length - 1];
      if (top) top.text += src.slice(lt + 9, close);
      i = close + 3;
      continue;
    }
    if (src.startsWith('<?', lt)) {
      const close = src.indexOf('?>', lt + 2);
      if (close < 0) return fail('a processing instruction that does not end', lt);
      i = close + 2;
      continue;
    }
    if (src.startsWith('<!', lt)) {
      const close = src.indexOf('>', lt + 2);
      if (close < 0) return fail('a declaration that does not end', lt);
      i = close + 1;
      continue;
    }
    if (src.startsWith('</', lt)) {
      const close = src.indexOf('>', lt + 2);
      if (close < 0) return fail('an end tag that does not end', lt);
      const qname = src.slice(lt + 2, close).trim();
      const top = stack.pop();
      if (!top || top.qname !== qname) return fail(`</${qname}> does not close ${top ? `<${top.qname}>` : 'any element'}`, lt);
      top.closeStart = lt;
      top.end = close + 1;
      i = close + 1;
      continue;
    }
    // A start tag: its name, then attributes up to `>` or `/>`.
    const nameMatch = /^<([A-Za-z_][\w.:-]*)/.exec(src.slice(lt, lt + 256));
    if (!nameMatch) return fail('a < that starts no tag', lt);
    const qname = nameMatch[1] as string;
    let at = lt + nameMatch[0].length;
    const attrs: XAttr[] = [];
    let selfClosing = false;
    for (;;) {
      while (at < src.length && /\s/.test(src[at] as string)) at += 1;
      if (src.startsWith('/>', at)) {
        selfClosing = true;
        at += 2;
        break;
      }
      if (src[at] === '>') {
        at += 1;
        break;
      }
      const attr = /^([A-Za-z_][\w.:-]*)\s*=\s*("([^"]*)"|'([^']*)')/.exec(src.slice(at, at + 65536));
      if (!attr) return fail(`an attribute of <${qname}> that cannot be read`, at);
      const aq = attr[1] as string;
      attrs.push({ qname: aq, name: local(aq), value: decode(attr[3] ?? attr[4] ?? '') });
      at += attr[0].length;
    }
    if (at > src.length) return fail(`<${qname}> does not end`, lt);
    const node: XNode = {
      name: local(qname),
      qname,
      attrs,
      children: [],
      text: '',
      start: lt,
      openEnd: at,
      closeStart: at,
      end: at,
      selfClosing,
    };
    const parent = stack[stack.length - 1];
    if (parent) parent.children.push(node);
    else if (root) return fail('a second root element', lt);
    else root = node;
    if (!selfClosing) stack.push(node);
    i = at;
  }
  if (stack.length > 0) return fail(`<${stack[stack.length - 1]?.qname}> is not closed`, src.length);
  if (!root) return { ok: false, error: 'the document has no element' };
  return { ok: true, root };
}

export const attr = (node: XNode, name: string): string | null => node.attrs.find((a) => a.name === name)?.value ?? null;
export const child = (node: XNode, name: string): XNode | null => node.children.find((c) => c.name === name) ?? null;
export const childrenNamed = (node: XNode, name: string): XNode[] => node.children.filter((c) => c.name === name);

/** A start tag written from a name and attributes, with `/>` when it is empty. */
export function openTag(qname: string, attrs: Array<[string, string]>, empty = false): string {
  const written = attrs.map(([n, v]) => ` ${n}="${escapeAttr(v)}"`).join('');
  return `<${qname}${written}${empty ? '/>' : '>'}`;
}

/** A number as an attribute writes it: no exponent, no trailing zeros. */
export function num(value: number): string {
  if (Number.isInteger(value)) return String(value);
  return String(Math.round(value * 1000) / 1000);
}

/** One change to a text: the bytes from `start` up to `end` become `text`. */
export interface Splice {
  start: number;
  end: number;
  text: string;
}

/** `src` with every splice made; splices must not overlap. */
export function splice(src: string, edits: Splice[]): string {
  const sorted = [...edits].sort((a, b) => a.start - b.start || a.end - b.end);
  let out = '';
  let at = 0;
  for (const e of sorted) {
    if (e.start < at) throw new Error(`two changes overlap at ${e.start}`);
    out += src.slice(at, e.start) + e.text;
    at = e.end;
  }
  return out + src.slice(at);
}

/**
 * The text of `node` with the `y` of every `position` inside it (its own and its connections'
 * points, which TC6 gives in the body's coordinates) moved by `dy`. A `relPosition` is
 * relative to its element and stays.
 */
export function shifted(src: string, node: XNode, dy: number): string {
  const text = src.slice(node.start, node.end);
  if (dy === 0) return text;
  const edits: Splice[] = [];
  const visit = (n: XNode) => {
    if (n.name === 'position') {
      const attrs: Array<[string, string]> = n.attrs.map((a) => {
        if (a.name !== 'y') return [a.qname, a.value];
        const y = Number(a.value);
        return [a.qname, Number.isFinite(y) ? num(y + dy) : a.value];
      });
      edits.push({ start: n.start - node.start, end: n.openEnd - node.start, text: openTag(n.qname, attrs, n.selfClosing) });
    }
    n.children.forEach(visit);
  };
  visit(node);
  return splice(text, edits);
}
