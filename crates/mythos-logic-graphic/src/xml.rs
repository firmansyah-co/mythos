//! A document read into a tree of elements, each with where its tag is.
//!
//! TC6 XML is read with `quick-xml` (MIT), event by event, into [`Node`]s: an element's local
//! name, its attributes, its children and the character data directly inside it. Nothing is
//! validated against the schema here; the reader of TC6 (`read`) takes what it knows by name
//! and refuses the rest, so a diagnostic names the element and not a schema rule.
//!
//! The tree is built without recursion, and a document nested deeper than [`MAX_DEPTH`] is
//! refused: a tree is dropped recursively, and the compiler runs on a thread of a service
//! process whose stack is small (ADR-013 §2.8).

use mythos_logic_ir::{Diagnostic, Span};
use quick_xml::events::Event;
use quick_xml::Reader;

/// How deep elements may nest. A Ladder Diagram is about ten deep (project, types, pous,
/// pou, body, LD, element, connectionPointIn, connection, position); the rest is room for
/// the XHTML of documentation, which is not read.
pub const MAX_DEPTH: usize = 100;

/// One element of a document.
#[derive(Debug, Clone, Default)]
pub struct Node {
    /// Its local name: without a namespace prefix.
    pub name: String,
    /// Each attribute by its local name, with its value unescaped.
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
    /// The character data directly inside it, references resolved, CDATA included.
    pub text: String,
    /// Where its start tag is in the document: from its `<` to after its `>`.
    pub span: Span,
}

impl Node {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
    pub fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }
}

/// Lines and columns of a document, from byte offsets.
struct Lines<'a> {
    text: &'a str,
    /// The byte offset each line starts at.
    starts: Vec<usize>,
}

impl<'a> Lines<'a> {
    fn new(text: &'a str) -> Self {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        Lines { text, starts }
    }
    /// The line and the column (in characters) of byte `at`, each from 1.
    fn at(&self, at: usize) -> (u32, u32) {
        let at = at.min(self.text.len());
        let line = self.starts.partition_point(|s| *s <= at) - 1;
        let start = self.starts[line];
        let col = self
            .text
            .get(start..at)
            .map_or(at - start, |s| s.chars().count());
        (line as u32 + 1, col as u32 + 1)
    }
    fn span(&self, from: usize, to: usize) -> Span {
        let (line, col) = self.at(from);
        let (end_line, end_col) = self.at(to);
        Span {
            line,
            col,
            end_line,
            end_col,
        }
    }
}

fn local(name: &str) -> String {
    match name.rsplit_once(':') {
        Some((_, local)) => local.to_string(),
        None => name.to_string(),
    }
}

/// The root element of `text`, and the namespace its name is in; or where the document is
/// not well formed, and why.
pub fn parse(text: &str) -> Result<(Node, Option<String>), Diagnostic> {
    let lines = Lines::new(text);
    let mut reader = Reader::from_str(text);
    // An end tag must close the element it ends.
    reader.config_mut().check_end_names = true;
    let mut stack: Vec<Node> = Vec::new();
    let mut root: Option<Node> = None;
    let mut namespace = None;
    loop {
        let from = reader.buffer_position() as usize;
        let event = match reader.read_event() {
            Ok(e) => e,
            Err(e) => {
                let at = reader.error_position() as usize;
                return Err(Diagnostic::at(
                    lines.span(at, at),
                    format!("the document is not well-formed XML: {e}"),
                ));
            }
        };
        let to = reader.buffer_position() as usize;
        let here = lines.span(from, to);
        let not_xml = |why: String| Diagnostic::at(here, why);
        match event {
            Event::Start(_) | Event::Empty(_) if root.is_some() => {
                return Err(not_xml(
                    "the document has more than one root element".into(),
                ));
            }
            Event::Start(ref e) | Event::Empty(ref e) => {
                if stack.len() >= MAX_DEPTH {
                    return Err(not_xml(format!(
                        "the document nests elements more than {MAX_DEPTH} deep"
                    )));
                }
                let mut node = Node {
                    name: local(e.name().as_ref()),
                    span: here,
                    ..Node::default()
                };
                for a in e.attributes() {
                    let a = a.map_err(|e| not_xml(format!("an attribute cannot be read: {e}")))?;
                    let value = a
                        .normalized_value(quick_xml::XmlVersion::default())
                        .map_err(|e| not_xml(format!("an attribute cannot be read: {e}")))?;
                    let key = a.key.as_ref().to_string();
                    if stack.is_empty() && (key == "xmlns" || key.starts_with("xmlns:")) {
                        // The namespace of the root's own name.
                        let prefix = key.strip_prefix("xmlns").unwrap_or("");
                        let prefix = prefix.strip_prefix(':').unwrap_or(prefix);
                        let own = e.name().as_ref().to_string();
                        let own_prefix = own.rsplit_once(':').map_or("", |(p, _)| p);
                        if prefix == own_prefix {
                            namespace = Some(value.to_string());
                        }
                    }
                    node.attrs.push((local(a.key.as_ref()), value.into_owned()));
                }
                if matches!(event, Event::Start(_)) {
                    stack.push(node);
                } else {
                    match stack.last_mut() {
                        Some(parent) => parent.children.push(node),
                        None => root = Some(node),
                    }
                }
            }
            Event::End(_) => {
                let node = stack
                    .pop()
                    .expect("the reader checks every end tag has a start");
                match stack.last_mut() {
                    Some(parent) => parent.children.push(node),
                    None => root = Some(node),
                }
            }
            Event::Text(t) => {
                if let Some(node) = stack.last_mut() {
                    node.text.push_str(&t.xml10_content());
                } else if !t.xml10_content().trim().is_empty() {
                    return Err(not_xml("text outside the root element".into()));
                }
            }
            Event::CData(t) => {
                if let Some(node) = stack.last_mut() {
                    node.text.push_str(&t.into_inner());
                }
            }
            Event::GeneralRef(r) => {
                let resolved = match r.resolve_char_ref() {
                    Ok(Some(c)) => c.to_string(),
                    Ok(None) => match quick_xml::escape::resolve_predefined_entity(&r) {
                        Some(s) => s.to_string(),
                        None => {
                            return Err(not_xml(format!(
                                "the entity &{}; is not one of XML's own",
                                &*r
                            )))
                        }
                    },
                    Err(e) => return Err(not_xml(format!("a character reference: {e}"))),
                };
                if let Some(node) = stack.last_mut() {
                    node.text.push_str(&resolved);
                }
            }
            Event::Eof => break,
            Event::Decl(_) | Event::Comment(_) | Event::PI(_) | Event::DocType(_) => {}
        }
    }
    if let Some(open) = stack.last() {
        return Err(Diagnostic::at(
            open.span,
            format!("the element {} is never closed", open.name),
        ));
    }
    match root {
        Some(root) => Ok((root, namespace)),
        None => Err(Diagnostic::at(
            lines.span(0, 0),
            "the document has no root element",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elements_attributes_text_and_places_are_read() {
        let doc = "<?xml version=\"1.0\"?>\n<p:a xmlns:p=\"urn:x\" k=\"1 &amp; 2\">\n  <b>x &lt; y<![CDATA[ & z]]></b><c/>\n</p:a>";
        let (root, ns) = parse(doc).unwrap();
        assert_eq!(ns.as_deref(), Some("urn:x"));
        assert_eq!(root.name, "a");
        assert_eq!(root.attr("k"), Some("1 & 2"));
        assert_eq!(root.children.len(), 2);
        let b = root.child("b").unwrap();
        assert_eq!(b.text, "x < y & z");
        assert_eq!((b.span.line, b.span.col), (3, 3));
        assert_eq!((b.span.end_line, b.span.end_col), (3, 6));
    }

    #[test]
    fn a_document_that_is_not_well_formed_is_refused_with_its_place() {
        let e = parse("<a>\n  <b></c>\n</a>").unwrap_err();
        assert!(e.message.contains("not well-formed"), "{e}");
        assert_eq!(e.line, 2);
        let e = parse("<a><b>").unwrap_err();
        assert!(e.message.contains("never closed"), "{e}");
        let e = parse("").unwrap_err();
        assert!(e.message.contains("no root"), "{e}");
    }

    #[test]
    fn a_document_nested_too_deeply_is_refused_and_does_not_exhaust_the_stack() {
        let doc = "<a>".repeat(100_000);
        let e = parse(&doc).unwrap_err();
        assert!(e.message.contains("more than 100 deep"), "{e}");
    }
}
