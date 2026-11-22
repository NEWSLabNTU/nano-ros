// Issue 2638.
//! The ONE reading of a launch parameter's baked text.
//!
//! An embedded image has no launch runtime to hand `--params-file` to, so the
//! build bakes every launch parameter as TEXT and the image types it when it
//! seeds the store. That text is written by
//! `nros_orchestration_ir::launch_param_text` and read here; the two are one
//! convention and are tested as a round trip.
//!
//! The reading follows what ROS gives the same value:
//!
//! - a scalar is `true` / `false` (any of the three cases rcl accepts), then
//!   an `i64`, then an `f64`, then a string -- the order rcl's YAML reading
//!   gives an unquoted scalar;
//! - a whole text in single quotes is a STRING, with `''` read as `'`. The
//!   writer quotes a string that would otherwise read as something else;
//! - a YAML flow sequence `[a, b, c]` is an ARRAY. The first element decides
//!   the element type and every other element must have it, which is rcl's
//!   rule (`parse.c`: "Sequence should be of same type"): `[1, 2.5]` is
//!   refused, not widened. A quoted element is a string.
//!
//! The list used to be baked as `a,b,c` and read as one string, so a
//! `[40.0, 50.0, 60.0]` from a `config/*.param.yaml` reached the node as the
//! string `"40.0,50.0,60.0"` -- and a one-element list as a scalar.
//!
//! No allocation: the element type is decided in one pass over the text and
//! the value is filled in place in a second, so no temporary array exists.

use crate::types::ParameterValue;
use heapless::{String, Vec};

/// Why a baked text has no value to seed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchTextError {
    /// `[]`. ROS gives an empty sequence no value: rcl's file parser
    /// allocates a sequence on its first element and never for `[]`, and
    /// launch_ros refuses an inline `value="[]"` ("Expected a non-empty
    /// sequence"). There is no element type to give it, so a seeder leaves
    /// the declaration's default standing.
    EmptyList,
    /// A list whose elements are not all one type (`[1, 2.5]`, `[1, a]`).
    /// rcl refuses the file and launch_ros refuses the inline value.
    MixedList,
    /// A string past `MAX_STRING_VALUE_LEN`, or a list past `MAX_ARRAY_LEN`
    /// (`MAX_STRING_ARRAY_LEN` for a list of strings).
    DoesNotFit,
    /// An unterminated quote or a stray character after a quoted element.
    Malformed,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Bool,
    Integer,
    Double,
    String,
}

fn boolean(t: &str) -> Option<bool> {
    match t {
        "true" | "True" | "TRUE" => Some(true),
        "false" | "False" | "FALSE" => Some(false),
        _ => None,
    }
}

fn kind_of_unquoted(t: &str) -> Kind {
    if boolean(t).is_some() {
        Kind::Bool
    } else if t.parse::<i64>().is_ok() {
        Kind::Integer
    } else if t.parse::<f64>().is_ok() {
        Kind::Double
    } else {
        Kind::String
    }
}

/// One element of a flow sequence, as written.
#[derive(Clone, Copy)]
enum Element<'a> {
    /// Unquoted, trimmed.
    Plain(&'a str),
    /// Single-quoted; the slice is BETWEEN the quotes, `''` still doubled.
    Single(&'a str),
    /// Double-quoted; the slice is between the quotes, escapes not read.
    Double(&'a str),
}

impl Element<'_> {
    fn kind(self) -> Kind {
        match self {
            Element::Plain(t) => kind_of_unquoted(t),
            Element::Single(_) | Element::Double(_) => Kind::String,
        }
    }
}

/// Walk the elements of the flow sequence body `inner` (the text between
/// `[` and `]`), calling `f` on each. `Err(Malformed)` on a broken quote.
fn for_each_element<'a>(
    inner: &'a str,
    mut f: impl FnMut(Element<'a>) -> Result<(), LaunchTextError>,
) -> Result<usize, LaunchTextError> {
    let bytes = inner.as_bytes();
    let mut i = 0;
    let mut n = 0;
    let skip_ws = |i: &mut usize| {
        while *i < bytes.len() && bytes[*i].is_ascii_whitespace() {
            *i += 1;
        }
    };
    skip_ws(&mut i);
    if i == bytes.len() {
        return Ok(0);
    }
    loop {
        skip_ws(&mut i);
        let element = match bytes.get(i) {
            Some(b'\'') => {
                let start = i + 1;
                let mut j = start;
                loop {
                    match bytes.get(j) {
                        None => return Err(LaunchTextError::Malformed),
                        Some(b'\'') if bytes.get(j + 1) == Some(&b'\'') => j += 2,
                        Some(b'\'') => break,
                        Some(_) => j += 1,
                    }
                }
                i = j + 1;
                Element::Single(&inner[start..j])
            }
            Some(b'"') => {
                let start = i + 1;
                let mut j = start;
                loop {
                    match bytes.get(j) {
                        None => return Err(LaunchTextError::Malformed),
                        Some(b'\\') => j += 2,
                        Some(b'"') => break,
                        Some(_) => j += 1,
                    }
                }
                i = j + 1;
                Element::Double(&inner[start..j])
            }
            _ => {
                let start = i;
                while i < bytes.len() && bytes[i] != b',' {
                    i += 1;
                }
                Element::Plain(inner[start..i].trim())
            }
        };
        f(element)?;
        n += 1;
        skip_ws(&mut i);
        match bytes.get(i) {
            None => return Ok(n),
            Some(b',') => {
                i += 1;
                // YAML allows one trailing comma: `[a, b,]` is two elements.
                skip_ws(&mut i);
                if i == bytes.len() {
                    return Ok(n);
                }
            }
            Some(_) => return Err(LaunchTextError::Malformed),
        }
    }
}

/// Push `'...'`'s content into `out`, reading `''` as `'`.
fn push_single_quoted<const N: usize>(out: &mut String<N>, body: &str) -> bool {
    let mut rest = body;
    while let Some(at) = rest.find("''") {
        if out.push_str(&rest[..=at]).is_err() {
            return false;
        }
        rest = &rest[at + 2..];
    }
    out.push_str(rest).is_ok()
}

fn list_body(text: &str) -> Option<&str> {
    text.strip_prefix('[')?.strip_suffix(']')
}

fn single_quoted_body(text: &str) -> Option<&str> {
    let body = text.strip_prefix('\'')?.strip_suffix('\'')?;
    // `'a' 'b'` is not one quoted scalar: every inner quote must be doubled.
    let mut rest = body;
    while let Some(at) = rest.find('\'') {
        if rest.as_bytes().get(at + 1) != Some(&b'\'') {
            return None;
        }
        rest = &rest[at + 2..];
    }
    Some(body)
}

/// A flow sequence body's element type and element count: the first element
/// decides the type and every other must have it (rcl's rule). The ONE pass both
/// readers below take.
fn list_kind(inner: &str) -> Result<(Kind, usize), LaunchTextError> {
    let mut kind: Option<Kind> = None;
    let n = for_each_element(inner, |e| {
        let k = e.kind();
        match kind {
            None => kind = Some(k),
            Some(seen) if seen != k => return Err(LaunchTextError::MixedList),
            Some(_) => {}
        }
        Ok(())
    })?;
    match kind {
        Some(kind) => Ok((kind, n)),
        None => Err(LaunchTextError::EmptyList),
    }
}

/// The TYPE a launch parameter's baked text seeds the store with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchTextType {
    Bool,
    Integer,
    Double,
    String,
    BoolArray,
    IntegerArray,
    DoubleArray,
    StringArray,
}

// Issue 2768 (a build sizing the store counts launch seeds).
/// The type [`ParameterValue::assign_launch_text`] would give
/// `text`, without filling a value and without a capacity check.
///
/// For a BUILD that sizes the store before any value exists: a launch seed
/// takes a slot of this type, so the host counts it by the device's own
/// reading rather than a second one that could disagree. No capacity, because
/// the build is what decides the capacities: a seed too long for today's store
/// is exactly what the count must grow it for, not a reason to skip it.
/// `Err` only where the device refuses the text whatever the capacity: an
/// empty list, a mixed list, a broken quote.
// Issue 2702.
/// How big the launch text `text` is, as a parameter value: its element count
/// (1 for a scalar) and the longest string in it, in bytes as written
/// (0 when it holds no string). `nros build` suggests a parameter store bound
/// from the seeds an image declares; a seed is an initial value, not a
/// maximum, so this is a FLOOR, never a size.
pub fn launch_text_extent(text: &str) -> Result<(usize, usize), LaunchTextError> {
    let text = text.trim();
    let Some(inner) = list_body(text) else {
        let longest = match launch_text_type(text)? {
            LaunchTextType::String => single_quoted_body(text).unwrap_or(text).len(),
            _ => 0,
        };
        return Ok((1, longest));
    };
    let mut longest = 0;
    let n = for_each_element(inner, |e| {
        if let Element::Plain(t) | Element::Single(t) | Element::Double(t) = e
            && e.kind() == Kind::String
        {
            longest = longest.max(t.len());
        }
        Ok(())
    })?;
    Ok((n, longest))
}

pub fn launch_text_type(text: &str) -> Result<LaunchTextType, LaunchTextError> {
    if let Some(inner) = list_body(text.trim()) {
        let (kind, _) = list_kind(inner)?;
        return Ok(match kind {
            Kind::Bool => LaunchTextType::BoolArray,
            Kind::Integer => LaunchTextType::IntegerArray,
            Kind::Double => LaunchTextType::DoubleArray,
            Kind::String => LaunchTextType::StringArray,
        });
    }
    if single_quoted_body(text).is_some() {
        return Ok(LaunchTextType::String);
    }
    Ok(match kind_of_unquoted(text) {
        Kind::Bool => LaunchTextType::Bool,
        Kind::Integer => LaunchTextType::Integer,
        Kind::Double => LaunchTextType::Double,
        Kind::String => LaunchTextType::String,
    })
}

impl ParameterValue {
    /// Type a launch parameter's baked text and write it into `self`, IN
    /// PLACE (a value is ~8 KiB; see `assign_string`). The reading is the
    /// module's: scalar inference, a single-quoted string, or a flow
    /// sequence typed by its elements.
    ///
    /// On `Err`, `self` is unspecified and must not be stored.
    pub fn assign_launch_text(&mut self, text: &str) -> Result<(), LaunchTextError> {
        if let Some(inner) = list_body(text.trim()) {
            return self.assign_flow_list(inner);
        }
        if let Some(body) = single_quoted_body(text) {
            if !matches!(self, Self::String(_)) {
                *self = Self::String(String::new());
            }
            return match self {
                Self::String(s) => {
                    s.clear();
                    if push_single_quoted(s, body) {
                        Ok(())
                    } else {
                        Err(LaunchTextError::DoesNotFit)
                    }
                }
                _ => Err(LaunchTextError::DoesNotFit),
            };
        }
        if let Some(b) = boolean(text) {
            *self = Self::Bool(b);
        } else if let Ok(i) = text.parse::<i64>() {
            *self = Self::Integer(i);
        } else if let Ok(d) = text.parse::<f64>() {
            *self = Self::Double(d);
        } else if !self.assign_string(text) {
            return Err(LaunchTextError::DoesNotFit);
        }
        Ok(())
    }

    fn assign_flow_list(&mut self, inner: &str) -> Result<(), LaunchTextError> {
        // Pass 1: the element type, which the first element decides.
        let (kind, n) = list_kind(inner)?;
        // A string list has its own element bound (issue 2647).
        let cap = if kind == Kind::String {
            crate::types::MAX_STRING_ARRAY_LEN
        } else {
            crate::types::MAX_ARRAY_LEN
        };
        if n > cap {
            return Err(LaunchTextError::DoesNotFit);
        }
        // Pass 2: fill the value of that type in place.
        *self = match kind {
            Kind::Bool => Self::BoolArray(Vec::new()),
            Kind::Integer => Self::IntegerArray(Vec::new()),
            Kind::Double => Self::DoubleArray(Vec::new()),
            Kind::String => Self::StringArray(Vec::new()),
        };
        let full = LaunchTextError::DoesNotFit;
        for_each_element(inner, |e| {
            let pushed = match (&mut *self, e) {
                (Self::BoolArray(v), Element::Plain(t)) => {
                    v.push(boolean(t).unwrap_or(false)).is_ok()
                }
                (Self::IntegerArray(v), Element::Plain(t)) => {
                    v.push(t.parse::<i64>().unwrap_or(0)).is_ok()
                }
                (Self::DoubleArray(v), Element::Plain(t)) => {
                    v.push(t.parse::<f64>().unwrap_or(0.0)).is_ok()
                }
                (Self::StringArray(v), e) => {
                    let mut s = String::new();
                    let ok = match e {
                        Element::Plain(t) | Element::Double(t) => s.push_str(t).is_ok(),
                        Element::Single(t) => push_single_quoted(&mut s, t),
                    };
                    ok && v.push(s).is_ok()
                }
                _ => false,
            };
            if pushed { Ok(()) } else { Err(full) }
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    /// Issue 2702 -- the floors `nros build` suggests: element count and the
    /// longest string, as written.
    #[test]
    fn a_seeds_extent_is_its_element_count_and_longest_string() {
        assert_eq!(launch_text_extent("[left, '5', 'a, b']"), Ok((3, 4)));
        assert_eq!(launch_text_extent("[40.0, 50.0, 60.0]"), Ok((3, 0)));
        assert_eq!(launch_text_extent("[true, false]"), Ok((2, 0)));
        assert_eq!(launch_text_extent("hello"), Ok((1, 5)));
        assert_eq!(launch_text_extent("'quoted'"), Ok((1, 6)));
        assert_eq!(launch_text_extent("250"), Ok((1, 0)));
    }

    use super::*;

    fn read(text: &str) -> Result<ParameterValue, LaunchTextError> {
        let mut v = ParameterValue::NotSet;
        v.assign_launch_text(text).map(|()| v)
    }

    /// Issue 2768 -- the build's type reading and the device's value reading
    /// agree on every text the device can store, and refuse the same texts.
    #[test]
    fn the_type_reader_agrees_with_the_value_reader() {
        let corpus = [
            "true",
            "FALSE",
            "42",
            "-7",
            "1.0",
            "1e3",
            "hello",
            "",
            "40.0,50.0",
            "'1.0'",
            "'[a]'",
            "'it''s'",
            "''",
            "'a' 'b'",
            "[true, false]",
            "[1, 2, 3]",
            "[1.5, 2.5]",
            "[a, b]",
            "['x', y]",
            "[1, 2.5]",
            "[]",
            "[1, a]",
            "['broken]",
            " [1, 2] ",
        ];
        for text in corpus {
            let ty = launch_text_type(text);
            match read(text) {
                Ok(v) => {
                    let want = match v {
                        ParameterValue::Bool(_) => LaunchTextType::Bool,
                        ParameterValue::Integer(_) => LaunchTextType::Integer,
                        ParameterValue::Double(_) => LaunchTextType::Double,
                        ParameterValue::String(_) => LaunchTextType::String,
                        ParameterValue::BoolArray(_) => LaunchTextType::BoolArray,
                        ParameterValue::IntegerArray(_) => LaunchTextType::IntegerArray,
                        ParameterValue::DoubleArray(_) => LaunchTextType::DoubleArray,
                        ParameterValue::StringArray(_) => LaunchTextType::StringArray,
                        other => panic!("{text:?}: unexpected {other:?}"),
                    };
                    assert_eq!(ty, Ok(want), "{text:?}");
                }
                Err(LaunchTextError::DoesNotFit) => {
                    assert!(ty.is_ok(), "{text:?}: a capacity is not a type error")
                }
                Err(e) => assert_eq!(ty, Err(e), "{text:?}"),
            }
        }
        // A list longer than any store is still typed: the build grows for it.
        extern crate std;
        let long = std::format!("[{}]", std::vec!["1"; 4096].join(", "));
        assert_eq!(launch_text_type(&long), Ok(LaunchTextType::IntegerArray));
    }

    #[test]
    fn scalars_keep_the_inference_they_had() {
        assert!(matches!(read("true"), Ok(ParameterValue::Bool(true))));
        assert!(matches!(read("FALSE"), Ok(ParameterValue::Bool(false))));
        assert!(matches!(read("42"), Ok(ParameterValue::Integer(42))));
        assert!(matches!(read("1.0"), Ok(ParameterValue::Double(d)) if d == 1.0));
        assert!(matches!(read("hello"), Ok(ParameterValue::String(s)) if s == "hello"));
        assert!(matches!(read(""), Ok(ParameterValue::String(s)) if s.is_empty()));
        // A comma is not a list: only a bracketed flow sequence is.
        assert!(matches!(read("40.0,50.0"), Ok(ParameterValue::String(s)) if s == "40.0,50.0"));
    }

    #[test]
    fn a_quoted_text_is_a_string() {
        assert!(matches!(read("'1.0'"), Ok(ParameterValue::String(s)) if s == "1.0"));
        assert!(matches!(read("'[a]'"), Ok(ParameterValue::String(s)) if s == "[a]"));
        assert!(matches!(read("'it''s'"), Ok(ParameterValue::String(s)) if s == "it's"));
        assert!(matches!(read("''"), Ok(ParameterValue::String(s)) if s.is_empty()));
        // Not ONE quoted scalar: read as the plain text it is.
        assert!(matches!(read("'a' 'b'"), Ok(ParameterValue::String(s)) if s == "'a' 'b'"));
    }

    #[test]
    fn a_double_list_is_a_double_array() {
        match read("[40.0, 50.0, 60.0]") {
            Ok(ParameterValue::DoubleArray(v)) => assert_eq!(v.as_slice(), &[40.0, 50.0, 60.0]),
            other => panic!("{other:?}"),
        }
        match read("[1.5]") {
            Ok(ParameterValue::DoubleArray(v)) => assert_eq!(v.as_slice(), &[1.5]),
            other => panic!("one element is still a list: {other:?}"),
        }
    }

    #[test]
    fn integer_bool_and_string_lists() {
        match read("[1, -2, 3]") {
            Ok(ParameterValue::IntegerArray(v)) => assert_eq!(v.as_slice(), &[1, -2, 3]),
            other => panic!("{other:?}"),
        }
        match read("[true, False]") {
            Ok(ParameterValue::BoolArray(v)) => assert_eq!(v.as_slice(), &[true, false]),
            other => panic!("{other:?}"),
        }
        match read("[a, '5', 'x, y', 'it''s', \"q\"]") {
            Ok(ParameterValue::StringArray(v)) => {
                let got: std::vec::Vec<&str> = v.iter().map(|s| s.as_str()).collect();
                assert_eq!(got, ["a", "5", "x, y", "it's", "q"]);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn empty_and_mixed_lists_have_no_value() {
        assert_eq!(read("[]").unwrap_err(), LaunchTextError::EmptyList);
        assert_eq!(read("[ ]").unwrap_err(), LaunchTextError::EmptyList);
        assert_eq!(read("[1, 2.5]").unwrap_err(), LaunchTextError::MixedList);
        assert_eq!(read("[1, a]").unwrap_err(), LaunchTextError::MixedList);
        assert_eq!(read("['1', 1]").unwrap_err(), LaunchTextError::MixedList);
        match read("[a, b,]") {
            Ok(ParameterValue::StringArray(v)) => assert_eq!(v.len(), 2),
            other => panic!("a trailing comma adds no element: {other:?}"),
        }
        assert_eq!(read("['a]").unwrap_err(), LaunchTextError::Malformed);
        assert_eq!(read("['a' b]").unwrap_err(), LaunchTextError::Malformed);
    }

    #[test]
    fn a_list_past_the_capacity_is_refused_not_cut() {
        let mut t = std::string::String::from("[");
        for i in 0..=crate::types::MAX_ARRAY_LEN {
            if i > 0 {
                t.push_str(", ");
            }
            t.push('1');
        }
        t.push(']');
        assert_eq!(read(&t).unwrap_err(), LaunchTextError::DoesNotFit);
    }
}
