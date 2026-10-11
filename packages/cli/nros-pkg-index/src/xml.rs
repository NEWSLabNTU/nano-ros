//! The two quick-xml reads every `package.xml` / launch-file reader here needs:
//! an element's TEXT and an attribute's VALUE, each with entities resolved.
//!
//! One spelling, because quick-xml 0.38 changed both under every reader at once
//! (issue 1805, the 0.37 -> 0.41 bump for RUSTSEC-2026-0194/0195):
//!
//! - **Text is no longer one event.** The reader stops at every `&` and reports
//!   the reference as its own [`Event::GeneralRef`], so `a &amp; b` arrives as
//!   `Text("a ")`, `GeneralRef("amp")`, `Text(" b")`. `BytesText::unescape` is
//!   gone. A reader that kept handling only `Event::Text` would take `"a "` as
//!   the whole value and drop the rest silently. [`ElementText`] accumulates the
//!   pieces until the element ends.
//! - **`Attribute::unescape_value` is deprecated** for `normalized_value`, which
//!   also applies XML's attribute-value normalization (a tab or newline becomes
//!   a space), as every conforming XML reader does, ROS's Python launch loader
//!   included.
//!
//! [`Event::GeneralRef`]: quick_xml::events::Event::GeneralRef

use eyre::{Result, eyre};
use quick_xml::{
    XmlVersion,
    escape::resolve_predefined_entity,
    events::{BytesRef, BytesText, attributes::Attribute},
};

/// The text content of the element being read, assembled from its `Text` and
/// `GeneralRef` events.
///
/// Feed it with [`text`](Self::text) and [`reference`](Self::reference), take
/// the result with [`take`](Self::take) at the element's `End`, and
/// [`clear`](Self::clear) it at every `Start`, so that text between sibling
/// elements never reaches the next one.
///
/// The reader must NOT be configured with `trim_text(true)`: that trims each
/// PIECE, so the spaces around an entity would be lost. [`take`](Self::take)
/// trims the assembled whole instead.
#[derive(Debug, Default)]
pub struct ElementText(String);

impl ElementText {
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Append a `Text` event's content.
    pub fn text(&mut self, e: &BytesText<'_>) -> Result<()> {
        let s = e.decode().map_err(|err| eyre!("undecodable text: {err}"))?;
        self.0.push_str(&s);
        Ok(())
    }

    /// Append a `GeneralRef` event: a character reference (`&#x41;`) or one of
    /// XML's five predefined entities. Any other entity needs a DTD, which no
    /// `package.xml` or launch file declares, so it is an error rather than
    /// text quietly missing a piece.
    pub fn reference(&mut self, r: &BytesRef<'_>) -> Result<()> {
        if let Some(c) = r
            .resolve_char_ref()
            .map_err(|err| eyre!("bad character reference: {err}"))?
        {
            self.0.push(c);
            return Ok(());
        }
        let name = r
            .decode()
            .map_err(|err| eyre!("undecodable entity: {err}"))?;
        let value = resolve_predefined_entity(&name)
            .ok_or_else(|| eyre!("unknown entity `&{name};` (only XML's predefined five)"))?;
        self.0.push_str(value);
        Ok(())
    }

    /// The element's text, trimmed, and reset for the next element. `None`
    /// when it is empty, which is what `trim_text(true)` reported as "no Text
    /// event at all".
    pub fn take(&mut self) -> Option<String> {
        let t = self.0.trim().to_string();
        self.0.clear();
        (!t.is_empty()).then_some(t)
    }
}

/// An attribute's value, entities resolved and normalized as XML 1.0 specifies.
pub fn attr_value(a: &Attribute<'_>) -> Result<String> {
    a.normalized_value(XmlVersion::Implicit1_0)
        .map(|v| v.into_owned())
        .map_err(|err| eyre!("bad attribute value: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use quick_xml::{events::Event, reader::Reader};

    fn element_texts(xml: &str) -> Vec<(String, String)> {
        let mut reader = Reader::from_str(xml);
        let mut text = ElementText::default();
        let mut out = Vec::new();
        loop {
            match reader.read_event().unwrap() {
                Event::Start(_) => text.clear(),
                Event::Text(e) => text.text(&e).unwrap(),
                Event::GeneralRef(r) => text.reference(&r).unwrap(),
                Event::End(e) => {
                    if let Some(t) = text.take() {
                        out.push((String::from_utf8_lossy(e.name().as_ref()).into(), t));
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }
        out
    }

    #[test]
    fn an_entity_does_not_cut_the_text_short() {
        assert_eq!(
            element_texts("<p><d>  a &amp; b&#x21; </d></p>"),
            vec![("d".into(), "a & b!".into())]
        );
    }

    #[test]
    fn text_between_siblings_does_not_reach_the_next_element() {
        assert_eq!(
            element_texts("<p>\n  <name>x</name>\n  <version>1</version>\n</p>"),
            vec![("name".into(), "x".into()), ("version".into(), "1".into())]
        );
    }

    #[test]
    fn an_undeclared_entity_is_refused() {
        let mut reader = Reader::from_str("<d>&nope;</d>");
        let mut text = ElementText::default();
        loop {
            match reader.read_event().unwrap() {
                Event::GeneralRef(r) => {
                    assert!(text.reference(&r).is_err());
                    return;
                }
                Event::Eof => panic!("no reference event"),
                _ => {}
            }
        }
    }

    #[test]
    fn an_attribute_value_resolves_entities() {
        let mut reader = Reader::from_str(r#"<a v="x &lt; y"/>"#);
        let Event::Empty(e) = reader.read_event().unwrap() else {
            panic!()
        };
        let a = e.attributes().next().unwrap().unwrap();
        assert_eq!(attr_value(&a).unwrap(), "x < y");
    }
}
