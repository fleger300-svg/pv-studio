//! A small in-memory XML tree over `quick-xml`: PV*SOL files are a few hundred kilobytes, and
//! walking a tree is much simpler than streaming their deeply nested, repetitive layout.

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::ReadError;

/// One element: its name (without namespace prefix), its text and its child elements.
#[derive(Debug, Clone, Default)]
pub struct Node {
    pub name: String,
    pub text: String,
    pub children: Vec<Node>,
}

impl Node {
    /// First child named `name`.
    #[must_use]
    pub fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }

    /// All children named `name`.
    pub fn children<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }

    /// Descendant along a path of child names (`a/b/c`).
    #[must_use]
    pub fn at(&self, path: &str) -> Option<&Node> {
        path.split('/').try_fold(self, |n, part| n.child(part))
    }

    /// Text of the descendant at `path`, trimmed.
    #[must_use]
    pub fn text_at(&self, path: &str) -> Option<&str> {
        self.at(path).map(|n| n.text.trim())
    }

    /// Number at `path` (PV*SOL writes `1.0E+0002`-style and plain decimals, `.` separator).
    #[must_use]
    pub fn f64_at(&self, path: &str) -> Option<f64> {
        self.text_at(path).and_then(parse_f64)
    }

    /// Number in this element's own text.
    #[must_use]
    pub fn f64(&self) -> Option<f64> {
        parse_f64(self.text.trim())
    }

    /// Value of PV*SOL's `<X><Value>…</Value></X>` wrapper at `path`.
    #[must_use]
    pub fn value_at(&self, path: &str) -> Option<f64> {
        self.at(path)
            .and_then(|n| n.child("Value"))
            .and_then(Node::f64)
    }

    /// Every descendant (depth first) named `name`.
    #[must_use]
    pub fn descendants<'a>(&'a self, name: &str) -> Vec<&'a Node> {
        let mut out = Vec::new();
        self.collect(name, &mut out);
        out
    }

    fn collect<'a>(&'a self, name: &str, out: &mut Vec<&'a Node>) {
        for c in &self.children {
            if c.name == name {
                out.push(c);
            }
            c.collect(name, out);
        }
    }
}

/// Parses a number as PV*SOL writes it; `None` when it is not finite.
#[must_use]
pub fn parse_f64(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok().filter(|x| x.is_finite())
}

/// Parses a whole document into its root element.
///
/// # Errors
/// [`ReadError::Xml`] when the document is not well-formed.
pub fn parse(file: &str, bytes: &[u8]) -> Result<Node, ReadError> {
    let err = |e: &dyn std::fmt::Display| ReadError::Xml {
        file: file.to_owned(),
        message: e.to_string(),
    };
    let text = std::str::from_utf8(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes))
        .map_err(|e| err(&e))?;
    let mut reader = Reader::from_str(text);
    let mut stack: Vec<Node> = vec![Node::default()];
    loop {
        match reader.read_event().map_err(|e| err(&e))? {
            Event::Start(e) => stack.push(Node {
                name: local_name(e.name().as_ref()),
                ..Node::default()
            }),
            Event::Empty(e) => {
                let node = Node {
                    name: local_name(e.name().as_ref()),
                    ..Node::default()
                };
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                }
            }
            Event::End(_) => {
                let node = stack.pop().ok_or_else(|| err(&"unbalanced end tag"))?;
                let parent = stack.last_mut().ok_or_else(|| err(&"unbalanced end tag"))?;
                parent.children.push(node);
            }
            Event::Text(t) => {
                if let Some(n) = stack.last_mut() {
                    n.text.push_str(&t.into_inner());
                }
            }
            Event::GeneralRef(r) => {
                if let Some(n) = stack.last_mut() {
                    let s = format!("&{};", r.into_inner());
                    n.text
                        .push_str(&quick_xml::escape::unescape(&s).map_err(|e| err(&e))?);
                }
            }
            Event::CData(t) => {
                if let Some(n) = stack.last_mut() {
                    n.text.push_str(&t.into_inner());
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let mut doc = stack
        .pop()
        .filter(|_| stack.is_empty())
        .ok_or_else(|| err(&"unclosed element"))?;
    doc.children
        .pop()
        .filter(|_| doc.children.is_empty())
        .ok_or_else(|| err(&"no single root element"))
}

fn local_name(raw: &str) -> String {
    raw.rsplit(':').next().unwrap_or_default().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_values_and_entities() {
        let doc = parse(
            "t.xml",
            br#"<?xml version="1.0"?><Root xmlns:xsi="x"><A><Value> 1.5E+0002</Value></A><B>R&amp;D</B><C/><D>1</D><D>2</D></Root>"#,
        )
        .unwrap();
        assert_eq!(doc.name, "Root");
        assert_eq!(doc.value_at("A"), Some(150.0));
        assert_eq!(doc.text_at("B"), Some("R&D"));
        assert!(doc.child("C").is_some());
        assert_eq!(doc.children("D").count(), 2);
        assert_eq!(doc.descendants("Value").len(), 1);
    }

    #[test]
    fn rejects_broken_documents() {
        assert!(parse("t.xml", b"<a><b></a>").is_err());
        assert!(parse("t.xml", b"<a>").is_err());
    }
}
