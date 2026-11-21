//! A reader for the CTF stream Zephyr's tracing subsystem writes, decoded
//! against the TSDL metadata of the Zephyr tree that built the image (issue
//! 2766).
//!
//! Zephyr's metadata (`subsys/tracing/ctf/tsdl/metadata`) is a small, regular
//! subset of TSDL: byte-aligned integer typealiases, one `event_header` struct
//! (a `uint32_t timestamp` and an id, `uint8_t` on 3.7 and `uint16_t` on 4.4),
//! and events whose fields are those integers or fixed `[N]` arrays of them.
//! Every event therefore has a fixed size, so the stream, a plain run of
//! events with no packet headers (the posix backend's `channel0_0`), decodes by
//! walking it: header, then the fields its id names.
//!
//! Decoding the whole stream, not grepping it for a byte pattern, is the point:
//! an id the metadata does not know, or a stream that ends mid-event, is an
//! error, so a passing read means every byte was accounted for.

use std::collections::BTreeMap;

/// One field of an event, sized from its typealias.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    /// Bytes of one element.
    pub size: usize,
    /// `[N]`, or 1.
    pub count: usize,
}

/// One event declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventDecl {
    pub name: String,
    pub fields: Vec<Field>,
}

impl EventDecl {
    /// The payload's size in bytes.
    pub fn payload_size(&self) -> usize {
        self.fields.iter().map(|f| f.size * f.count).sum()
    }
}

/// What the metadata says about the stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Metadata {
    /// Bytes of the header's timestamp.
    pub timestamp_size: usize,
    /// Bytes of the header's id.
    pub id_size: usize,
    pub events: BTreeMap<u64, EventDecl>,
}

/// One decoded event: its id, name and payload fields as little-endian
/// integers (an array field contributes one value per element).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub timestamp: u64,
    pub id: u64,
    pub name: String,
    pub fields: Vec<(String, Vec<u64>)>,
}

impl Event {
    /// The first value of field `name`.
    pub fn field(&self, name: &str) -> Option<u64> {
        self.fields
            .iter()
            .find(|(n, _)| n == name)
            .and_then(|(_, v)| v.first().copied())
    }
}

/// Strip `/* ... */` and `// ...` comments.
fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix("/*") {
            rest = r.find("*/").map_or("", |i| &r[i + 2..]);
        } else if let Some(r) = rest.strip_prefix("//") {
            rest = r.find('\n').map_or("", |i| &r[i..]);
        } else {
            let c = rest.chars().next().unwrap();
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

/// The body of the first `{ ... }` after `from`, with nested braces, and the
/// index just past it.
fn braced(text: &str, from: usize) -> Result<(&str, usize), String> {
    let open = text[from..]
        .find('{')
        .map(|i| from + i)
        .ok_or("expected `{`")?;
    let mut depth = 0usize;
    for (i, c) in text[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok((&text[open + 1..open + i], open + i + 1));
                }
            }
            _ => {}
        }
    }
    Err("unbalanced braces".into())
}

fn parse_int(s: &str) -> Result<u64, String> {
    let s = s.trim();
    let r = match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(h) => u64::from_str_radix(h, 16),
        None => s.parse(),
    };
    r.map_err(|e| format!("`{s}` is not an integer: {e}"))
}

/// `type name;` or `type name[N];` lines of a struct body.
fn parse_fields(body: &str, sizes: &BTreeMap<String, usize>) -> Result<Vec<Field>, String> {
    let mut fields = Vec::new();
    for decl in body.split(';').map(str::trim).filter(|d| !d.is_empty()) {
        let mut parts = decl.split_whitespace();
        let (Some(ty), Some(name), None) = (parts.next(), parts.next(), parts.next()) else {
            return Err(format!("field `{decl}` is not `<type> <name>`"));
        };
        let size = *sizes
            .get(ty)
            .ok_or_else(|| format!("field `{decl}`: unknown type `{ty}`"))?;
        let (name, count) = match name.split_once('[') {
            Some((n, rest)) => {
                let n_str = rest
                    .strip_suffix(']')
                    .ok_or_else(|| format!("field `{decl}`: bad array"))?;
                (n.to_string(), parse_int(n_str)? as usize)
            }
            None => (name.to_string(), 1),
        };
        fields.push(Field { name, size, count });
    }
    Ok(fields)
}

/// Parse Zephyr's TSDL metadata.
pub fn parse_metadata(text: &str) -> Result<Metadata, String> {
    let text = strip_comments(text);
    // typealias integer { size = N; ... } := name;
    let mut sizes = BTreeMap::new();
    let mut at = 0;
    while let Some(i) = text[at..].find("typealias integer") {
        let start = at + i;
        let (body, end) = braced(&text, start)?;
        let bits = body
            .split(';')
            .filter_map(|kv| kv.split_once('='))
            .find(|(k, _)| k.trim() == "size")
            .map(|(_, v)| parse_int(v))
            .transpose()?
            .ok_or("typealias with no size")?;
        let tail = &text[end..];
        let name = tail
            .trim_start()
            .strip_prefix(":=")
            .and_then(|r| r.split(';').next())
            .map(str::trim)
            .ok_or("typealias with no `:= name;`")?;
        if bits % 8 != 0 {
            return Err(format!(
                "typealias `{name}` is {bits} bits, not whole bytes"
            ));
        }
        sizes.insert(name.to_string(), (bits / 8) as usize);
        at = end;
    }

    let hdr_at = text
        .find("struct event_header")
        .ok_or("no `struct event_header`")?;
    let (hdr_body, _) = braced(&text, hdr_at)?;
    let hdr = parse_fields(hdr_body, &sizes)?;
    let size_of = |n: &str| {
        hdr.iter()
            .find(|f| f.name == n)
            .map(|f| f.size)
            .ok_or(format!("event_header has no `{n}`"))
    };
    let (timestamp_size, id_size) = (size_of("timestamp")?, size_of("id")?);
    if hdr.len() != 2 {
        return Err(format!(
            "event_header has {} fields, expected timestamp and id",
            hdr.len()
        ));
    }

    let mut events = BTreeMap::new();
    let mut at = 0;
    while let Some(i) = text[at..].find("event {") {
        let start = at + i;
        let (body, end) = braced(&text, start)?;
        let value = |key: &str| {
            body.split(';')
                .filter_map(|kv| kv.split_once('='))
                .find(|(k, _)| k.trim() == key)
                .map(|(_, v)| v.trim().to_string())
        };
        let name = value("name").ok_or("event with no name")?;
        let id = parse_int(&value("id").ok_or_else(|| format!("event `{name}` has no id"))?)?;
        let fields = match body.find("fields") {
            Some(f) => parse_fields(braced(body, f)?.0, &sizes)?,
            None => Vec::new(),
        };
        if events
            .insert(
                id,
                EventDecl {
                    name: name.clone(),
                    fields,
                },
            )
            .is_some()
        {
            return Err(format!(
                "two events have id {id:#x} (the second is `{name}`)"
            ));
        }
        at = end;
    }
    if events.is_empty() {
        return Err("no events".into());
    }
    Ok(Metadata {
        timestamp_size,
        id_size,
        events,
    })
}

fn le(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .rev()
        .fold(0u64, |acc, b| (acc << 8) | u64::from(*b))
}

/// Decode a whole stream. Every byte must belong to an event the metadata
/// declares.
pub fn decode(meta: &Metadata, stream: &[u8]) -> Result<Vec<Event>, String> {
    let header = meta.timestamp_size + meta.id_size;
    let mut out = Vec::new();
    let mut at = 0;
    while at < stream.len() {
        if stream.len() - at < header {
            return Err(format!(
                "stream ends {} byte(s) into an event header at offset {at}",
                stream.len() - at
            ));
        }
        let timestamp = le(&stream[at..at + meta.timestamp_size]);
        let id = le(&stream[at + meta.timestamp_size..at + header]);
        let decl = meta.events.get(&id).ok_or_else(|| {
            format!(
                "unknown event id {id:#x} at offset {at} (event {}): the metadata does not \
                 match the tree that wrote the stream",
                out.len()
            )
        })?;
        let mut p = at + header;
        if stream.len() - p < decl.payload_size() {
            return Err(format!("stream ends inside `{}` at offset {at}", decl.name));
        }
        let mut fields = Vec::with_capacity(decl.fields.len());
        for f in &decl.fields {
            let vals = (0..f.count)
                .map(|k| le(&stream[p + k * f.size..p + (k + 1) * f.size]))
                .collect();
            p += f.size * f.count;
            fields.push((f.name.clone(), vals));
        }
        out.push(Event {
            timestamp,
            id,
            name: decl.name.clone(),
            fields,
        });
        at = p;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape of Zephyr 4.4's metadata: a 16-bit id, a string array.
    const META: &str = r#"/* CTF 1.8 */
typealias integer { size = 8; align = 8; signed = false; } := uint8_t;
typealias integer { size = 16; align = 8; signed = false; } := uint16_t;
typealias integer { size = 32; align = 8; signed = false; } := uint32_t;
typealias integer { size = 8; align = 8; signed = false; encoding = ASCII; } := ctf_bounded_string_t;
struct event_header {
	uint32_t timestamp;
	uint16_t id;
};
trace { major = 1; minor = 8; byte_order = le; };
stream { event.header := struct event_header; };
event {
	name = thread_switched_in;
	id = 0x11;
	fields := struct {
		uint32_t thread_id;
		ctf_bounded_string_t name[20];
	};
};
event {
	name = idle;
	id = 0x20;
};
event {
	name = app_marker;
	id = 0xF000;
	fields := struct {
		uint32_t marker_id;
		uint32_t arg;
	};
};
"#;

    fn ev(ts: u32, id: u16, payload: &[u8]) -> Vec<u8> {
        let mut v = ts.to_le_bytes().to_vec();
        v.extend_from_slice(&id.to_le_bytes());
        v.extend_from_slice(payload);
        v
    }

    #[test]
    fn the_metadata_sizes_every_event() {
        let m = parse_metadata(META).unwrap();
        assert_eq!((m.timestamp_size, m.id_size), (4, 2));
        assert_eq!(m.events[&0x11].payload_size(), 24);
        assert_eq!(m.events[&0x20].payload_size(), 0);
        assert_eq!(m.events[&0xF000].name, "app_marker");
    }

    #[test]
    fn a_stream_decodes_event_by_event() {
        let m = parse_metadata(META).unwrap();
        let mut s = ev(5, 0x11, &[[7, 0, 0, 0].as_slice(), &[b'x'; 20]].concat());
        s.extend(ev(6, 0x20, &[]));
        s.extend(ev(9, 0xF000, &[18, 0, 0, 0, 42, 0, 0, 0]));
        let e = decode(&m, &s).unwrap();
        assert_eq!(e.len(), 3);
        assert_eq!(e[2].name, "app_marker");
        assert_eq!(
            (e[2].field("marker_id"), e[2].field("arg")),
            (Some(18), Some(42))
        );
        assert_eq!(e[2].timestamp, 9);
    }

    #[test]
    fn an_unknown_id_is_an_error_not_a_skip() {
        let m = parse_metadata(META).unwrap();
        let err = decode(&m, &ev(1, 0x99, &[])).unwrap_err();
        assert!(err.contains("unknown event id 0x99"), "{err}");
    }

    #[test]
    fn a_truncated_stream_is_an_error() {
        let m = parse_metadata(META).unwrap();
        let mut s = ev(9, 0xF000, &[18, 0, 0, 0, 42, 0, 0, 0]);
        s.truncate(s.len() - 1);
        assert!(
            decode(&m, &s)
                .unwrap_err()
                .contains("ends inside `app_marker`")
        );
        assert!(decode(&m, &s[..3]).unwrap_err().contains("event header"));
    }

    #[test]
    fn an_eight_bit_id_header_reads_too() {
        let m = parse_metadata(
            &META
                .replace("uint16_t id;", "uint8_t id;")
                .replace("0xF000", "0x70"),
        )
        .unwrap();
        assert_eq!(m.id_size, 1);
        let mut s = 3u32.to_le_bytes().to_vec();
        s.push(0x70);
        s.extend_from_slice(&[19, 0, 0, 0, 1, 0, 0, 0]);
        assert_eq!(decode(&m, &s).unwrap()[0].field("marker_id"), Some(19));
    }
}
