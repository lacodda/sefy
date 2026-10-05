//! Just enough XML for KeePass's export: a tree to read, and escaping to
//! write one.
//!
//! The files are a few megabytes at most and are read whole, so a small tree
//! is the simplest thing to walk; quick-xml does the parsing, entities and
//! all, and this turns its events into elements.

use quick_xml::XmlVersion;
use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

/// One element: its name, attributes, children and the text directly in it.
#[derive(Debug, Default)]
pub(crate) struct Element {
    pub name: String,
    pub attributes: Vec<(String, String)>,
    pub children: Vec<Element>,
    /// Text directly inside the element, references resolved. For an element
    /// holding others, that is mostly the indentation between them.
    pub text: String,
}

impl Element {
    /// The first child with this name.
    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|child| child.name == name)
    }

    /// Every child with this name, in document order.
    pub fn children<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.children.iter().filter(move |child| child.name == name)
    }

    /// The text of the first child with this name.
    pub fn text_of(&self, name: &str) -> Option<&str> {
        self.child(name).map(|child| child.text.as_str())
    }

    /// The value of an attribute.
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

/// Parses a document into its root element.
///
/// Line ends inside text are normalized the way XML requires, so a value
/// written as `\r\n` reads as `\n`; one written as `&#13;\n` — which is how
/// [`escape`] writes a carriage return — keeps it.
pub(crate) fn parse(text: &str) -> Result<Element, String> {
    let mut reader = Reader::from_str(text);
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;

    loop {
        let event = reader
            .read_event()
            .map_err(|error| format!("at byte {}: {error}", reader.error_position()))?;
        match event {
            Event::Start(start) => stack.push(element(&start)?),
            Event::Empty(start) => {
                let element = element(&start)?;
                attach(&mut stack, &mut root, element)?;
            }
            Event::End(_) => {
                let element = stack.pop().ok_or("an end tag closes nothing")?;
                attach(&mut stack, &mut root, element)?;
            }
            Event::Text(text) => {
                if let Some(current) = stack.last_mut() {
                    current.text.push_str(&text.xml10_content());
                }
            }
            Event::CData(data) => {
                if let Some(current) = stack.last_mut() {
                    current.text.push_str(&data.xml10_content());
                }
            }
            Event::GeneralRef(reference) => {
                let Some(current) = stack.last_mut() else {
                    continue;
                };
                if reference.is_char_ref() {
                    let character = reference
                        .resolve_char_ref()
                        .map_err(|error| error.to_string())?
                        .ok_or("a character reference names no character")?;
                    current.text.push(character);
                } else {
                    let name = reference.xml10_content();
                    let resolved = resolve_predefined_entity(&name)
                        .ok_or_else(|| format!("unknown entity &{name};"))?;
                    current.text.push_str(resolved);
                }
            }
            Event::Eof => break,
            // The declaration, comments, processing instructions and a DTD say
            // nothing about the entries.
            _ => {}
        }
    }

    if !stack.is_empty() {
        return Err("the document ends inside an element".to_owned());
    }
    root.ok_or_else(|| "the document is empty".to_owned())
}

fn element(start: &quick_xml::events::BytesStart<'_>) -> Result<Element, String> {
    let name = start.name().as_ref().to_owned();
    let mut attributes = Vec::new();
    for attribute in start.attributes() {
        let attribute = attribute.map_err(|error| error.to_string())?;
        let key = attribute.key.as_ref().to_owned();
        let value = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|error| error.to_string())?
            .into_owned();
        attributes.push((key, value));
    }
    Ok(Element {
        name,
        attributes,
        ..Element::default()
    })
}

fn attach(
    stack: &mut [Element],
    root: &mut Option<Element>,
    element: Element,
) -> Result<(), String> {
    match stack.last_mut() {
        Some(parent) => parent.children.push(element),
        None if root.is_none() => *root = Some(element),
        None => return Err("the document has more than one root element".to_owned()),
    }
    Ok(())
}

/// Escapes text for an element or a quoted attribute.
///
/// A carriage return is written as a reference rather than as itself: XML
/// turns a literal `\r\n` into `\n` on reading, and a value must come back
/// byte for byte. A character XML 1.0 cannot carry at all — most control
/// characters — is returned as the error, because dropping it would change
/// the value without saying so.
pub(crate) fn escape(text: &str) -> Result<String, char> {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            '\r' => escaped.push_str("&#13;"),
            '\t' | '\n' => escaped.push(character),
            '\u{0}'..='\u{1f}' | '\u{fffe}' | '\u{ffff}' => return Err(character),
            _ => escaped.push(character),
        }
    }
    Ok(escaped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tree_comes_out_with_text_attributes_and_references_resolved() {
        let root = parse(
            "<?xml version=\"1.0\"?>\n<!-- a comment -->\n<a x=\"1 &amp; 2\">\
             <b>one &lt;two&gt; &#x41;&#66;</b><b/><c><![CDATA[<raw>]]></c></a>",
        )
        .unwrap();
        assert_eq!(root.name, "a");
        assert_eq!(root.attribute("x"), Some("1 & 2"));
        assert_eq!(root.children("b").count(), 2);
        assert_eq!(root.text_of("b"), Some("one <two> AB"));
        assert_eq!(root.text_of("c"), Some("<raw>"));
    }

    #[test]
    fn whatever_escape_writes_parses_back_byte_for_byte() {
        let value = "line one\r\nline two\n\t\"quoted\" & 'apostrophe' <tag> \u{1F511}";
        let document = format!("<v>{}</v>", escape(value).unwrap());
        assert_eq!(parse(&document).unwrap().text, value);

        let attribute = "\"one\" & 'two' <3>";
        let document = format!("<v a=\"{}\"/>", escape(attribute).unwrap());
        assert_eq!(parse(&document).unwrap().attribute("a"), Some(attribute));
    }

    #[test]
    fn a_character_xml_cannot_carry_is_refused_rather_than_dropped() {
        assert_eq!(escape("bell\u{7}"), Err('\u{7}'));
        assert_eq!(escape("nul\u{0}"), Err('\u{0}'));
    }

    #[test]
    fn broken_documents_are_errors() {
        assert!(parse("<a><b></a>").is_err());
        assert!(parse("<a>").is_err());
        assert!(parse("<a/><b/>").is_err());
        assert!(parse("<a>&nonsense;</a>").is_err());
        assert!(parse("").is_err());
    }
}
