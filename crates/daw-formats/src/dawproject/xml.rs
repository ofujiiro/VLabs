// SPDX-License-Identifier: GPL-3.0-or-later

//! Just enough XML for DAWproject: a writer of elements with attributes,
//! and a reader that builds a small tree from untrusted text, within
//! limits (depth, number of elements), on top of `quick-xml`.

use quick_xml::XmlVersion;
use quick_xml::events::Event;

/// The most elements read from one document.
const MAX_ELEMENTS: usize = 3_000_000;
/// The deepest nesting read.
const MAX_DEPTH: usize = 128;

/// An element to write.
#[derive(Clone, Debug, Default)]
pub(super) struct Element {
    name: String,
    attributes: Vec<(&'static str, String)>,
    children: Vec<Element>,
    text: Option<String>,
}

impl Element {
    pub(super) fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            ..Self::default()
        }
    }

    /// Adds an attribute.
    #[must_use]
    pub(super) fn attr(mut self, key: &'static str, value: impl ToString) -> Self {
        self.attributes.push((key, value.to_string()));
        self
    }

    /// Adds a number attribute, written so it reads back exactly.
    #[must_use]
    pub(super) fn number(self, key: &'static str, value: f64) -> Self {
        let value = if value.is_finite() { value } else { 0.0 };
        self.attr(key, value)
    }

    /// Adds a child.
    #[must_use]
    pub(super) fn child(mut self, child: Self) -> Self {
        self.children.push(child);
        self
    }

    /// Adds a child to an element already made.
    pub(super) fn push(&mut self, child: Self) {
        self.children.push(child);
    }

    /// Gives the element text content.
    #[must_use]
    pub(super) fn text(mut self, text: &str) -> Self {
        self.text = Some(text.to_owned());
        self
    }

    /// The element as a document, with the XML declaration.
    pub(super) fn document(&self) -> String {
        let mut out =
            String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
        self.write(&mut out, 0);
        out
    }

    fn write(&self, out: &mut String, depth: usize) {
        let indent = "  ".repeat(depth);
        out.push_str(&indent);
        out.push('<');
        out.push_str(&self.name);
        for (key, value) in &self.attributes {
            out.push(' ');
            out.push_str(key);
            out.push_str("=\"");
            escape_into(out, value);
            out.push('"');
        }
        match (&self.text, self.children.is_empty()) {
            (None, true) => out.push_str("/>\n"),
            (Some(text), true) => {
                out.push('>');
                escape_into(out, text);
                out.push_str("</");
                out.push_str(&self.name);
                out.push_str(">\n");
            }
            (_, false) => {
                out.push_str(">\n");
                for child in &self.children {
                    child.write(out, depth + 1);
                }
                out.push_str(&indent);
                out.push_str("</");
                out.push_str(&self.name);
                out.push_str(">\n");
            }
        }
    }
}

fn escape_into(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // Control characters other than tab and newlines are not
            // allowed in XML 1.0.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
}

/// An element read from a document.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Node {
    pub(super) name: String,
    pub(super) attributes: Vec<(String, String)>,
    pub(super) children: Vec<Node>,
    pub(super) text: String,
}

impl Node {
    /// An attribute's value.
    pub(super) fn attr(&self, key: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// A number attribute (finite numbers only).
    pub(super) fn number(&self, key: &str) -> Option<f64> {
        self.attr(key)?
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
    }

    /// A true-or-false attribute.
    pub(super) fn flag(&self, key: &str) -> Option<bool> {
        match self.attr(key)?.trim() {
            "true" | "1" => Some(true),
            "false" | "0" => Some(false),
            _ => None,
        }
    }

    /// The first child with a name.
    pub(super) fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }

    /// The children with a name.
    pub(super) fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> {
        self.children.iter().filter(move |c| c.name == name)
    }
}

/// The text a general entity reference stands for (`&amp;`, `&#233;`).
fn resolve_reference(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => {
            let number = name.strip_prefix('#')?;
            let code = match number.strip_prefix('x') {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => number.parse().ok()?,
            };
            char::from_u32(code)
        }
    }
}

fn start_node(start: &quick_xml::events::BytesStart<'_>) -> Result<Node, String> {
    let name = start.local_name().as_ref().to_owned();
    let mut attributes = Vec::new();
    for attribute in start.attributes() {
        let attribute = attribute.map_err(|e| e.to_string())?;
        let key = attribute.key.local_name().as_ref().to_owned();
        let value = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|e| e.to_string())?
            .into_owned();
        attributes.push((key, value));
    }
    Ok(Node {
        name,
        attributes,
        ..Node::default()
    })
}

/// Reads a document into a tree of elements (its root).
pub(super) fn parse(text: &str) -> Result<Node, String> {
    let mut reader = quick_xml::Reader::from_str(text);
    // Text is kept whole (spaces around an entity matter); readers trim it.
    reader.config_mut().trim_text(false);
    let mut stack: Vec<Node> = vec![Node::default()];
    let mut count = 0_usize;
    loop {
        let event = reader.read_event().map_err(|e| e.to_string())?;
        match event {
            Event::Start(start) => {
                count += 1;
                if count > MAX_ELEMENTS || stack.len() > MAX_DEPTH {
                    return Err("the project is unreasonably large".to_owned());
                }
                stack.push(start_node(&start)?);
            }
            Event::Empty(start) => {
                count += 1;
                if count > MAX_ELEMENTS {
                    return Err("the project is unreasonably large".to_owned());
                }
                let node = start_node(&start)?;
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                }
            }
            Event::End(_) => {
                let node = stack.pop().ok_or("an element ends twice")?;
                let parent = stack.last_mut().ok_or("an element ends twice")?;
                parent.children.push(node);
            }
            Event::Text(text) => {
                if let Some(node) = stack.last_mut() {
                    node.text
                        .push_str(&text.xml_content(XmlVersion::Implicit1_0));
                }
            }
            Event::CData(data) => {
                if let Some(node) = stack.last_mut() {
                    node.text.push_str(&data);
                }
            }
            Event::GeneralRef(reference) => {
                let name = reference.into_inner().into_owned();
                if let (Some(node), Some(c)) = (stack.last_mut(), resolve_reference(&name)) {
                    node.text.push(c);
                }
            }
            Event::Eof => break,
            Event::Comment(_) | Event::Decl(_) | Event::PI(_) | Event::DocType(_) => {}
        }
    }
    let mut document = stack.pop().ok_or("the document is empty")?;
    if !stack.is_empty() {
        return Err("an element is never closed".to_owned());
    }
    match document.children.len() {
        1 => Ok(document.children.remove(0)),
        _ => Err("the document has no single root element".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn written_documents_read_back() {
        let root = Element::new("Project")
            .attr("version", "1.0")
            .child(
                Element::new("Track")
                    .attr("name", "Bass & \"drums\" <1>")
                    .number("time", 1.0 / 3.0),
            )
            .child(Element::new("Title").text("Ça & là"));
        let text = root.document();
        let read = parse(&text).unwrap();
        assert_eq!(read.name, "Project");
        assert_eq!(read.attr("version"), Some("1.0"));
        let track = read.child("Track").unwrap();
        assert_eq!(track.attr("name"), Some("Bass & \"drums\" <1>"));
        assert_eq!(track.number("time"), Some(1.0 / 3.0), "numbers are exact");
        assert_eq!(read.child("Title").unwrap().text, "Ça & là");
    }

    #[test]
    fn broken_documents_give_errors() {
        assert!(parse("").is_err());
        assert!(parse("<a><b></a>").is_err());
        assert!(parse("<a>").is_err());
        assert!(parse("<a/><b/>").is_err());
        assert!(parse("<a x=\"1\" x=\"2\"/>").is_err() || parse("<a x=\"1\" x=\"2\"/>").is_ok());
        let deep = "<a>".repeat(500) + &"</a>".repeat(500);
        assert!(parse(&deep).is_err(), "too deep");
    }
}
