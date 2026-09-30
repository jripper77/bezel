//! A YAML document as an ordered tree, read with `serde-saphyr` under a
//! resource budget (theme files are untrusted).
//!
//! Map keys are always read as text (so `Y:` stays `"Y"` instead of the
//! YAML 1.1 boolean), and the values of the keys in [`TEXT_KEYS`] (and of
//! `TEXT` inside `static_text` blocks, where it is not a widget) are read as
//! text too, keeping `TEXT: 1.50` or `FORMAT: yes` exactly as written.
//! Duplicate keys are kept in order; lookups take the last one, like PyYAML.

use std::fmt;

use serde::de::{Deserialize, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};

/// Keys whose values are always free text in turing-smart-screen-python
/// themes.
const TEXT_KEYS: [&str; 11] = [
    "FORMAT",
    "PATH",
    "FONT",
    "AXIS_FONT",
    "BACKGROUND_IMAGE",
    "ANCHOR",
    "ALIGN",
    "DISPLAY_SIZE",
    "DISPLAY_ORIENTATION",
    "BAR_DECORATION",
    "author",
];

/// Largest theme file accepted.
pub const MAX_BYTES: usize = 1024 * 1024;

/// A YAML value.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    /// `~`, `null` or nothing.
    Null,
    /// A boolean (YAML 1.1 spellings included).
    Bool(bool),
    /// An integer.
    Int(i64),
    /// A float.
    Float(f64),
    /// Text.
    Str(String),
    /// A sequence.
    Seq(Vec<Node>),
    /// A mapping, in document order.
    Map(Vec<(String, Node)>),
}

impl Node {
    /// The value of `key` in a mapping (the last one when repeated).
    pub fn get(&self, key: &str) -> Option<&Node> {
        match self {
            Node::Map(entries) => entries.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// The entries of a mapping in order, without earlier duplicates.
    pub fn entries(&self) -> Vec<(&str, &Node)> {
        let Node::Map(entries) = self else {
            return Vec::new();
        };
        entries
            .iter()
            .enumerate()
            .filter(|(i, (k, _))| !entries[i + 1..].iter().any(|(later, _)| later == k))
            .map(|(_, (k, v))| (k.as_str(), v))
            .collect()
    }

    /// A lenient boolean: booleans, `yes`/`no`/`on`/`off`/`true`/`false`
    /// text in any case, and numbers (non-zero = true).
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Node::Bool(b) => Some(*b),
            Node::Int(i) => Some(*i != 0),
            Node::Str(s) => match s.trim().to_ascii_lowercase().as_str() {
                "true" | "yes" | "y" | "on" => Some(true),
                "false" | "no" | "n" | "off" => Some(false),
                _ => None,
            },
            _ => None,
        }
    }

    /// A number, also from numeric text.
    pub fn as_f64(&self) -> Option<f64> {
        let v = match self {
            Node::Int(i) => *i as f64,
            Node::Float(f) => *f,
            Node::Str(s) => s.trim().parse().ok()?,
            _ => return None,
        };
        v.is_finite().then_some(v)
    }

    /// Text; numbers and booleans are written back as text.
    pub fn as_text(&self) -> Option<String> {
        match self {
            Node::Str(s) => Some(s.clone()),
            Node::Int(i) => Some(i.to_string()),
            Node::Float(f) => Some(f.to_string()),
            Node::Bool(b) => Some(if *b { "True" } else { "False" }.to_string()),
            _ => None,
        }
    }
}

/// Parses one YAML document.
pub fn parse(text: &str) -> Result<Node, String> {
    if text.len() > MAX_BYTES {
        return Err(format!("the file is larger than {MAX_BYTES} bytes"));
    }
    let options = serde_saphyr::options! {
        budget: serde_saphyr::budget! {
            max_depth: 32,
            max_documents: 1,
            max_nodes: 200_000,
            max_total_scalar_bytes: MAX_BYTES,
            max_anchors: 1_000,
            max_aliases: 1_000,
        },
        duplicate_keys: serde_saphyr::DuplicateKeyPolicy::LastWins,
        emit_comments: false,
        with_snippet: false,
    };
    serde_saphyr::from_str_with_options::<Node>(text, options).map_err(|e| {
        e.to_string()
            .lines()
            .next()
            .unwrap_or("invalid YAML")
            .to_string()
    })
}

/// Where a value sits, to know when `TEXT` is text and when it is a widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ctx {
    Root,
    StaticText,
    TextBlock,
    Other,
}

struct NodeVisitor(Ctx);

impl<'de> Visitor<'de> for NodeVisitor {
    type Value = Node;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("any YAML value")
    }

    fn visit_bool<E>(self, v: bool) -> Result<Node, E> {
        Ok(Node::Bool(v))
    }

    fn visit_i64<E>(self, v: i64) -> Result<Node, E> {
        Ok(Node::Int(v))
    }

    fn visit_u64<E>(self, v: u64) -> Result<Node, E> {
        Ok(i64::try_from(v).map_or(Node::Float(v as f64), Node::Int))
    }

    fn visit_f64<E>(self, v: f64) -> Result<Node, E> {
        Ok(Node::Float(v))
    }

    fn visit_str<E>(self, v: &str) -> Result<Node, E> {
        Ok(Node::Str(v.to_string()))
    }

    fn visit_string<E>(self, v: String) -> Result<Node, E> {
        Ok(Node::Str(v))
    }

    fn visit_unit<E>(self) -> Result<Node, E> {
        Ok(Node::Null)
    }

    fn visit_none<E>(self) -> Result<Node, E> {
        Ok(Node::Null)
    }

    fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Node, D::Error> {
        d.deserialize_any(self)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Node, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element_seed(NodeSeed(Ctx::Other))? {
            items.push(item);
        }
        Ok(Node::Seq(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Node, A::Error> {
        let mut entries = Vec::new();
        while let Some(key) = map.next_key::<String>()? {
            let is_text =
                TEXT_KEYS.contains(&key.as_str()) || (self.0 == Ctx::TextBlock && key == "TEXT");
            let value = if is_text {
                map.next_value_seed(TextSeed)?
            } else {
                let child = match (self.0, key.as_str()) {
                    (Ctx::Root, "static_text") => Ctx::StaticText,
                    (Ctx::StaticText, _) => Ctx::TextBlock,
                    _ => Ctx::Other,
                };
                map.next_value_seed(NodeSeed(child))?
            };
            entries.push((key, value));
        }
        Ok(Node::Map(entries))
    }
}

/// Reads any value in a context.
struct NodeSeed(Ctx);

impl<'de> DeserializeSeed<'de> for NodeSeed {
    type Value = Node;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Node, D::Error> {
        d.deserialize_any(NodeVisitor(self.0))
    }
}

/// Reads a scalar as the text written in the file.
struct TextSeed;

impl<'de> DeserializeSeed<'de> for TextSeed {
    type Value = Node;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Node, D::Error> {
        Ok(Option::<String>::deserialize(d)?.map_or(Node::Null, Node::Str))
    }
}

impl<'de> Deserialize<'de> for Node {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Node, D::Error> {
        d.deserialize_any(NodeVisitor(Ctx::Root))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaml_1_1_quirks() {
        let doc = parse(
            "---\nY: 5\nSHOW: TRUE\nON: off\nstatic_text: {A: {TEXT: 1.50}}\nFORMAT: yes\nDISPLAY_SIZE: 3.5\"\n\
             FONT_COLOR: 255, 0, 0\nMAX_VALUE: 5.3\nBBOX: [0, -1, 80, 160]\nPATH:\nC: '#ff0000'\n\
             D: #comment\nY: 7\nBIG: 18446744073709551615\n",
        )
        .expect("parses");
        assert_eq!(doc.get("Y"), Some(&Node::Int(7)), "last duplicate wins");
        assert_eq!(doc.get("SHOW").and_then(Node::as_bool), Some(true));
        assert_eq!(doc.get("ON").and_then(Node::as_bool), Some(false));
        let block = doc.get("static_text").and_then(|t| t.get("A"));
        assert_eq!(
            block.and_then(|b| b.get("TEXT")),
            Some(&Node::Str("1.50".into()))
        );
        assert_eq!(doc.get("FORMAT"), Some(&Node::Str("yes".into())));
        assert_eq!(doc.get("DISPLAY_SIZE"), Some(&Node::Str("3.5\"".into())));
        assert_eq!(doc.get("FONT_COLOR"), Some(&Node::Str("255, 0, 0".into())));
        assert_eq!(doc.get("MAX_VALUE").and_then(Node::as_f64), Some(5.3));
        assert_eq!(
            doc.get("BBOX"),
            Some(&Node::Seq(vec![
                Node::Int(0),
                Node::Int(-1),
                Node::Int(80),
                Node::Int(160)
            ]))
        );
        assert_eq!(doc.get("PATH"), Some(&Node::Null));
        assert_eq!(
            doc.get("C").and_then(Node::as_text).as_deref(),
            Some("#ff0000")
        );
        assert_eq!(doc.get("D"), Some(&Node::Null));
        assert!(matches!(doc.get("BIG"), Some(Node::Float(_))));
        let keys: Vec<&str> = doc.entries().iter().map(|(k, _)| *k).collect();
        assert_eq!(keys.first(), Some(&"SHOW"), "the first Y was overridden");
        assert_eq!(keys.iter().filter(|k| **k == "Y").count(), 1);
        assert!(doc.get("missing").is_none());
        assert!(Node::Int(1).get("x").is_none());
        assert!(Node::Int(1).entries().is_empty());
    }

    #[test]
    fn conversions() {
        assert_eq!(Node::Str(" Yes ".into()).as_bool(), Some(true));
        assert_eq!(Node::Str("n".into()).as_bool(), Some(false));
        assert_eq!(Node::Str("maybe".into()).as_bool(), None);
        assert_eq!(Node::Int(0).as_bool(), Some(false));
        assert_eq!(Node::Null.as_bool(), None);
        assert_eq!(Node::Str(" 12 ".into()).as_f64(), Some(12.0));
        assert_eq!(Node::Float(f64::NAN).as_f64(), None);
        assert_eq!(Node::Bool(true).as_f64(), None);
        assert_eq!(Node::Float(1.5).as_text().as_deref(), Some("1.5"));
        assert_eq!(Node::Int(3).as_text().as_deref(), Some("3"));
        assert_eq!(Node::Bool(false).as_text().as_deref(), Some("False"));
        assert_eq!(Node::Seq(vec![]).as_text(), None);
    }

    #[test]
    fn refuses_bad_or_huge_documents() {
        assert!(parse("a: [1, 2").is_err());
        assert!(parse("FONT: [1]\n").is_err());
        let widget = parse("STATS: {CPU: {TEXT: {SHOW: yes}}}\n").expect("parses");
        let show = widget
            .get("STATS")
            .and_then(|s| s.get("CPU"))
            .and_then(|c| c.get("TEXT"))
            .and_then(|t| t.get("SHOW"));
        assert_eq!(show, Some(&Node::Bool(true)));
        assert!(parse(&"x".repeat(MAX_BYTES + 1)).is_err());
        let deep = format!("{}1{}", "[".repeat(40), "]".repeat(40));
        assert!(parse(&deep).is_err());
        assert!(parse("a: 1\n---\nb: 2\n").is_err());
    }
}
