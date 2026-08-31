//! phase-403 W6 -- the derived bound has to LEAVE codegen.
//!
//! Codegen is the right place to DERIVE a message type's serialized-size bound.
//! It is the wrong place for the bound to STOP. Until this module existed the
//! number was emitted only as a `#define` inside a generated header, so every
//! later stage that needed it invented a substitute: the arena multiplied
//! `MAX_CBS` by an action-client worst case, the zenoh payload classes were two
//! hand-set constants, and `NROS_MAX_LARGE_SUBSCRIBERS` was a number a human
//! produced by reading generated headers with their eyes. That last one is
//! measured, not hypothetical -- bringing the island up on mr-canhubk344 meant
//! copying `Control 2052` and `Odometry 1804` out of generated C++ headers into
//! a board `.conf`.
//!
//! So this module carries ONE data model -- [`BoundInventory`] -- and renders it
//! into the transports the later stages already speak:
//!
//! * [`BoundInventory::to_json`] -- the canonical artifact, written beside the
//!   generated code as `nros_message_bounds.json`. This is the format; the other
//!   two are projections of it.
//! * [`BoundInventory::to_cmake`] -- a `.cmake` script the CMake/Kconfig lane
//!   `include()`s. Same shape as `nros codegen resolve-deps --output-cmake`,
//!   which is the existing mechanism for handing CMake a fact codegen computed.
//! * [`BoundInventory::to_build_rs`] -- the `build.rs` a generated Rust message
//!   crate ships, whose `cargo:` metadata reaches a dependent's build script as
//!   `DEP_<LINKS>_BOUNDS_JSON`. Same `links` channel `DEP_NROS_NODE_RX_BUF_SIZE`
//!   already uses.
//!
//! # A type NEVER appears with a fabricated number
//!
//! [`BoundState`] has three states, not two, for the reason
//! [`crate::schema_value::TypeBound`] does: "we looked and no bound exists" and
//! "we could not look" license completely different actions. Neither carries a
//! size, and neither emits a `_TX`/`_RX` key on any transport -- a consumer that
//! reads a number gets a number that was derived, or it reads nothing at all.
//!
//! # This is the REAL bound, not the C++ pack's estimate
//!
//! Every number here comes from [`crate::schema_value::bound_message`], i.e.
//! from `nros_serdes::size::max_serialized_size` -- THE size rule, the same
//! function the runtime's `M::MAX_SERIALIZED_SIZE_XCDR*` uses.
//!
//! It is deliberately NOT [`crate::types::compute_serialized_size_max`], which
//! the C++ pack still uses for its in-header `SERIALIZED_SIZE_MAX`. That
//! function ESTIMATES: it charges a flat 512 bytes per nested message and a flat
//! default capacity per string, and it always returns a value, so it can never
//! report "unbounded". A flat 512 for a nested type whose own bound exceeds 512
//! is an UNDER-estimate, which is the direction that matters. Exporting it as
//! authoritative build metadata would make that guess load-bearing across the
//! whole build, which is exactly what this wave exists to stop.

use crate::schema_value::TypeBound;

/// Bumped when the shape of the emitted inventory changes incompatibly.
/// A consumer that does not recognise the version must refuse, never guess.
pub const INVENTORY_SCHEMA_VERSION: u32 = 1;

/// Canonical artifact name, written into the generated package's output dir.
pub const INVENTORY_JSON_NAME: &str = "nros_message_bounds.json";

/// CMake projection of [`INVENTORY_JSON_NAME`], beside it.
pub const INVENTORY_CMAKE_NAME: &str = "nros_message_bounds.cmake";

/// What codegen concluded about one type's serialized-size bound.
///
/// The two encodings are kept apart because they genuinely differ: XCDR2 adds a
/// 4-byte DHEADER and aligns 8-byte primitives to 4 instead of 8. `tx` is the
/// XCDR1 number because this stack WRITES XCDR1; `rx` is the larger of the two
/// because a receive buffer must hold either.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundState {
    /// A bound exists. Bytes, encapsulation header included.
    Bounded { tx: usize, rx: usize },
    /// No bound EXISTS. Carries EVERY member that costs it, as `nros_serdes`
    /// names them. Fix by bounding the field (`string<=64`) or capping it
    /// `inline` in `nros-codegen.toml`.
    Unbounded { reason: String },
    /// The bound was not COMPUTED, because a nested type was not reachable
    /// through the resolver. A search-path problem, not a property of the
    /// message. Nothing may be sized from it.
    Unresolved { reason: String },
}

impl BoundState {
    /// The one place the two per-encoding answers become a TX/RX pair.
    ///
    /// The C header emitter (`generator::msg`) calls this too, so the constants
    /// in a generated header and the numbers in the inventory cannot drift into
    /// disagreeing about which encoding feeds which direction.
    pub fn classify(xcdr1: &TypeBound, xcdr2: &TypeBound) -> Self {
        match (xcdr1, xcdr2) {
            (TypeBound::Bounded(a), TypeBound::Bounded(b)) => BoundState::Bounded {
                tx: *a,
                rx: *a.max(b),
            },
            // Unbounded wins over Unresolved when both appear: "there is no
            // bound" is a fact about the message, and it stays true however the
            // search path is fixed.
            (TypeBound::Unbounded(w), _) | (_, TypeBound::Unbounded(w)) => BoundState::Unbounded {
                reason: Self::unbounded_reason(w),
            },
            (TypeBound::Unresolved(t), _) | (_, TypeBound::Unresolved(t)) => {
                BoundState::Unresolved {
                    reason: format!("nested type `{t}` could not be resolved"),
                }
            }
        }
    }

    /// The one spelling of "why this type has no bound", shared by the exported
    /// inventory and the generated C header.
    ///
    /// Singular for one member so the common case reads as prose, plural with a
    /// comma list otherwise. The list is ordered by declaration, which is the
    /// order the user reads their `.msg` in, not sorted — a sorted list of
    /// members is harder to walk against the file you are editing.
    pub fn unbounded_reason(members: &[String]) -> String {
        match members {
            [one] => format!("unbounded member: {one}"),
            many => format!("unbounded members: {}", many.join(", ")),
        }
    }

    /// The `state` word used on every transport.
    pub fn tag(&self) -> &'static str {
        match self {
            BoundState::Bounded { .. } => "bounded",
            BoundState::Unbounded { .. } => "unbounded",
            BoundState::Unresolved { .. } => "unresolved",
        }
    }
}

/// One type's row in the inventory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeBoundEntry {
    /// ROS fully-qualified name, `pkg/msg/Name` -- the spelling
    /// `rmw_subscription_options_t`'s `type_name` and the vtable's
    /// `required_rx_bytes` already use, so a consumer can key on it without a
    /// second naming convention.
    pub type_name: String,
    pub bound: BoundState,
}

/// Every generated message type of one interface package, with its bound.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BoundInventory {
    pub package: String,
    entries: Vec<TypeBoundEntry>,
}

impl BoundInventory {
    pub fn new(package: impl Into<String>) -> Self {
        Self {
            package: package.into(),
            entries: Vec::new(),
        }
    }

    /// Record one type. Later records for the same name replace earlier ones so
    /// a driver that regenerates a type in one pass cannot emit it twice.
    pub fn insert(&mut self, type_name: impl Into<String>, bound: BoundState) {
        let type_name = type_name.into();
        match self.entries.iter_mut().find(|e| e.type_name == type_name) {
            Some(existing) => existing.bound = bound,
            None => self.entries.push(TypeBoundEntry { type_name, bound }),
        }
    }

    /// Derive and record the bound for one parsed message.
    ///
    /// `lookup` resolves nested types. A lookup that cannot reach a nested type
    /// yields `Unresolved`, never a number.
    ///
    /// `caps` is the SAME `nros-codegen.toml` resolver the emitters were handed.
    /// It is a required argument rather than an optional refinement because the
    /// inventory and the generated header must agree: a field capped `inline`
    /// bounds the type in both, or the exported number and the `#define` say
    /// different things about one type and nothing in the build compares them.
    pub fn record_message(
        &mut self,
        type_name: &str,
        message: &rosidl_parser::Message,
        caps: &crate::CapacityResolver,
        lookup: &crate::schema_value::MsgLookup<'_>,
    ) {
        use crate::schema_value::bound_message;
        use nros_serdes::cdr::EncodingVersion;
        let x1 = bound_message(type_name, message, EncodingVersion::Xcdr1, caps, lookup);
        let x2 = bound_message(type_name, message, EncodingVersion::Xcdr2, caps, lookup);
        self.insert(type_name, BoundState::classify(&x1, &x2));
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Entries in emission order: sorted by type name, so the artifact is
    /// byte-stable across runs and `write_if_changed` keeps mtimes still.
    pub fn entries(&self) -> Vec<&TypeBoundEntry> {
        let mut v: Vec<&TypeBoundEntry> = self.entries.iter().collect();
        v.sort_by(|a, b| a.type_name.cmp(&b.type_name));
        v
    }

    /// The canonical artifact. Pretty-printed for the on-disk file; the
    /// `build.rs` transport uses [`Self::to_json_compact`] because a `cargo:`
    /// metadata value cannot contain a newline.
    pub fn to_json(&self) -> String {
        format!("{}\n", self.json_value_string(true))
    }

    /// One line, no newline. Same document as [`Self::to_json`].
    pub fn to_json_compact(&self) -> String {
        self.json_value_string(false)
    }

    fn json_value_string(&self, pretty: bool) -> String {
        let types: Vec<serde_json::Value> = self
            .entries()
            .into_iter()
            .map(|e| {
                let mut m = serde_json::Map::new();
                m.insert("type_name".into(), e.type_name.clone().into());
                m.insert("state".into(), e.bound.tag().into());
                match &e.bound {
                    BoundState::Bounded { tx, rx } => {
                        m.insert("tx_max_serialized_size".into(), (*tx).into());
                        m.insert("rx_max_serialized_size".into(), (*rx).into());
                    }
                    BoundState::Unbounded { reason } | BoundState::Unresolved { reason } => {
                        m.insert("reason".into(), reason.clone().into());
                    }
                }
                serde_json::Value::Object(m)
            })
            .collect();
        let doc = serde_json::json!({
            "schema_version": INVENTORY_SCHEMA_VERSION,
            "producer": "nros-codegen",
            "package": self.package,
            // Named so a reader cannot mistake these for the C++ pack's
            // `SERIALIZED_SIZE_MAX`, which is an estimate (see module docs).
            "derivation": "nros_serdes::size::max_serialized_size",
            "types": types,
        });
        if pretty {
            serde_json::to_string_pretty(&doc).unwrap_or_default()
        } else {
            serde_json::to_string(&doc).unwrap_or_default()
        }
    }

    /// The CMake/Kconfig projection.
    ///
    /// Appends to global lists so several packages' fragments compose into one
    /// image-wide inventory, which is the shape W4 needs to turn the zenoh
    /// payload classes into "the distinct sizes this image's types actually
    /// need". `include()` it at the scope you want the variables in -- CMake
    /// function scope does not leak.
    ///
    /// An unbounded or unresolved type gets a `_STATE` and a `_REASON` and NO
    /// `_TX`/`_RX`, so a consumer that reads a size either reads a derived
    /// number or reads nothing.
    pub fn to_cmake(&self) -> String {
        let mut s = String::new();
        s.push_str("# GENERATED by nros codegen (phase-403 W6). Do not edit.\n");
        s.push_str(&format!(
            "# Derived with nros_serdes::size::max_serialized_size -- the same rule the\n\
             # runtime's M::MAX_SERIALIZED_SIZE_XCDR* uses. NOT the C++ pack's estimate.\n\
             set(NROS_MESSAGE_BOUNDS_SCHEMA_VERSION {INVENTORY_SCHEMA_VERSION})\n"
        ));
        s.push_str(&format!(
            "list(APPEND NROS_MESSAGE_BOUND_PACKAGES \"{}\")\n",
            self.package
        ));
        for e in self.entries() {
            let key = cmake_key(&e.type_name);
            s.push_str(&format!(
                "list(APPEND NROS_MESSAGE_BOUND_TYPES \"{}\")\n",
                e.type_name
            ));
            s.push_str(&format!(
                "set(NROS_MESSAGE_BOUND_{key}_STATE \"{}\")\n",
                e.bound.tag()
            ));
            match &e.bound {
                BoundState::Bounded { tx, rx } => {
                    s.push_str(&format!("set(NROS_MESSAGE_BOUND_{key}_TX {tx})\n"));
                    s.push_str(&format!("set(NROS_MESSAGE_BOUND_{key}_RX {rx})\n"));
                }
                BoundState::Unbounded { reason } | BoundState::Unresolved { reason } => {
                    s.push_str(&format!(
                        "set(NROS_MESSAGE_BOUND_{key}_REASON \"{}\")\n",
                        cmake_escape(reason)
                    ));
                }
            }
        }
        s.push_str("list(REMOVE_DUPLICATES NROS_MESSAGE_BOUND_PACKAGES)\n");
        s.push_str("list(REMOVE_DUPLICATES NROS_MESSAGE_BOUND_TYPES)\n");
        s
    }

    /// The `build.rs` a generated Rust message crate ships.
    ///
    /// The crate declares `links = "<links_key>"`, so cargo hands every
    /// `cargo:KEY=VALUE` line below to the build script of each crate that
    /// depends on it, as `DEP_<LINKS_KEY_UPPERCASE>_<KEY_UPPERCASE>`. That is
    /// the channel `nros-c`'s build script already reads `DEP_NROS_NODE_MAX_CBS`
    /// on; this wave adds a producer to it rather than a second mechanism.
    ///
    /// The value is the same JSON document as the on-disk artifact, compacted,
    /// because a `cargo:` metadata value may not contain a newline.
    pub fn to_build_rs(&self) -> String {
        format!(
            r#"// GENERATED by nros codegen (phase-403 W6). Do not edit; regenerate with
// `nros sync`.
//
// This crate carries `links` purely so these lines reach a dependent's build
// script as `DEP_<LINKS>_BOUNDS_*`. It links no native library. The channel is
// the one `nros-c` already reads `DEP_NROS_NODE_RX_BUF_SIZE` on.
//
// A type whose bound does not exist, or could not be computed, appears with a
// `state` of "unbounded"/"unresolved" and NO size. Never a substituted default:
// phase-380's rule is that `None` means "no bound EXISTS", never "unknown", and
// a receive buffer sized from a fallback is one that silently mismatches the
// wire.
fn main() {{
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:bounds_schema={schema}");
    // The payload is a Rust STRING LITERAL, not a format string: the document
    // is full of `"` and `{{`/`}}`, and putting it in the format position emits
    // a file that does not parse.
    println!("cargo:bounds_json={{}}", "{json}");
}}
"#,
            schema = INVENTORY_SCHEMA_VERSION,
            json = rust_string_literal_body(&self.to_json_compact()),
        )
    }

    /// The `links` key for a generated crate of `package`.
    ///
    /// Cargo requires `links` to be unique across the dependency graph; a
    /// generated crate is named after its ament package, which already is.
    pub fn links_key(package: &str) -> String {
        format!("nros_msgs_{}", package.replace(['-', '.', '/'], "_"))
    }
}

/// A CMake variable name fragment for a ROS type name.
fn cmake_key(type_name: &str) -> String {
    type_name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// CMake `set(... "...")` is quote- and backslash-sensitive; a reason is prose
/// from `nros_serdes` and must not be able to end the string early.
fn cmake_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// The body of a Rust `"..."` literal holding `s`.
///
/// The inventory is JSON, so it is ALL quotes; emitting it raw produced a
/// `build.rs` that did not parse. Not a raw string (`r#"..."#`) either -- a
/// reason is arbitrary prose from `nros_serdes` and could in principle carry
/// the closing delimiter.
fn rust_string_literal_body(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 16);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CapacityResolver;
    use rosidl_parser::{Message, parse_message};

    fn no_lookup(_: &str) -> Option<Message> {
        None
    }

    fn inv() -> BoundInventory {
        let mut i = BoundInventory::new("test_msgs");
        let flat = parse_message("int64 b\n").unwrap();
        i.record_message(
            "test_msgs/msg/Flat",
            &flat,
            &CapacityResolver::empty(),
            &no_lookup,
        );
        let open = parse_message("string s\n").unwrap();
        i.record_message(
            "test_msgs/msg/Open",
            &open,
            &CapacityResolver::empty(),
            &no_lookup,
        );
        let nested = parse_message("other_pkg/Thing t\n").unwrap();
        i.record_message(
            "test_msgs/msg/Nested",
            &nested,
            &CapacityResolver::empty(),
            &no_lookup,
        );
        i
    }

    /// The numbers are the ones `nros_serdes` computes, and the two encodings
    /// really do differ for this type (12 vs 16), which is why TX and RX are
    /// separate fields rather than one constant.
    #[test]
    fn a_bounded_type_carries_the_derived_tx_and_rx() {
        let i = inv();
        let e = i
            .entries()
            .into_iter()
            .find(|e| e.type_name == "test_msgs/msg/Flat")
            .unwrap()
            .clone();
        assert_eq!(e.bound, BoundState::Bounded { tx: 12, rx: 16 });
    }

    /// The whole point of the wave: an unbounded type is a MARKER plus the
    /// member that costs the bound, never a number a later stage could read as
    /// authoritative.
    #[test]
    fn an_unbounded_type_carries_a_reason_and_no_number() {
        let i = inv();
        let e = i
            .entries()
            .into_iter()
            .find(|e| e.type_name == "test_msgs/msg/Open")
            .unwrap()
            .clone();
        match &e.bound {
            BoundState::Unbounded { reason } => assert!(
                reason.contains('s'),
                "the reason must name the member: {reason}"
            ),
            other => panic!("expected Unbounded, got {other:?}"),
        }
        // No transport may carry a size for it.
        assert!(!i.to_cmake().contains("Open_TX"));
        assert!(!i.to_cmake().contains("Open_RX"));
        assert!(!i.to_json().contains("\"test_msgs/msg/Open\",\n    \"tx"));
    }

    /// "We could not look" is not "there is no bound", and the inventory keeps
    /// them apart because they license different fixes.
    #[test]
    fn an_unreachable_nested_type_is_unresolved_not_unbounded() {
        let i = inv();
        let e = i
            .entries()
            .into_iter()
            .find(|e| e.type_name == "test_msgs/msg/Nested")
            .unwrap()
            .clone();
        assert_eq!(e.bound.tag(), "unresolved");
        assert!(!i.to_cmake().contains("Nested_TX"));
    }

    #[test]
    fn the_cmake_projection_sets_a_size_only_for_a_bounded_type() {
        let c = inv().to_cmake();
        assert!(c.contains("set(NROS_MESSAGE_BOUND_test_msgs_msg_Flat_TX 12)"));
        assert!(c.contains("set(NROS_MESSAGE_BOUND_test_msgs_msg_Flat_RX 16)"));
        assert!(c.contains("set(NROS_MESSAGE_BOUND_test_msgs_msg_Open_STATE \"unbounded\")"));
        assert!(c.contains("list(APPEND NROS_MESSAGE_BOUND_PACKAGES \"test_msgs\")"));
        // Composable: nothing here assigns a list, everything appends.
        assert!(!c.contains("set(NROS_MESSAGE_BOUND_TYPES"));
    }

    /// The payload is EMITTED RUST, and it is a document made almost entirely
    /// of `"` and `{`/`}`. The first version of this emitter interpolated it
    /// straight into the `println!` FORMAT string and produced a `build.rs`
    /// that did not parse; nothing caught it until a real package was generated
    /// and the file was read. So this asserts the escaping, not just that the
    /// bytes are somewhere in the file:
    ///
    /// * the literal is in ARGUMENT position, so `{`/`}` need no doubling;
    /// * every `"` inside it is backslash-escaped, so the literal does not end
    ///   early;
    /// * un-escaping the literal body gives back the exact document.
    #[test]
    fn the_build_rs_payload_is_an_escaped_rust_string_literal() {
        let i = inv();
        let build_rs = i.to_build_rs();
        let line = build_rs
            .lines()
            .find(|l| l.contains("cargo:bounds_json="))
            .expect("build.rs emits the inventory");

        // Argument position, never format position.
        assert!(
            line.contains(r#"println!("cargo:bounds_json={}", ""#),
            "the document must be an argument, not the format string: {line}"
        );
        // A raw, unescaped document would show a bare `{"` here.
        assert!(
            !build_rs.contains("{\"derivation"),
            "the document leaked into the file unescaped: {build_rs}"
        );

        let body = line
            .split_once(r#"cargo:bounds_json={}", ""#)
            .unwrap()
            .1
            .trim_end()
            .trim_end_matches(");")
            .trim_end_matches('"');
        let unescaped = body.replace("\\\"", "\"").replace("\\\\", "\\");
        assert_eq!(unescaped, i.to_json_compact());
        let parsed: serde_json::Value = serde_json::from_str(&unescaped).expect("valid JSON");
        assert_eq!(parsed["schema_version"], INVENTORY_SCHEMA_VERSION);
        assert_eq!(parsed["types"].as_array().unwrap().len(), 3);

        // A `cargo:` metadata value may not contain a newline.
        assert!(!i.to_json_compact().contains('\n'));
    }

    #[test]
    fn the_json_and_the_compact_json_are_the_same_document() {
        let i = inv();
        let a: serde_json::Value = serde_json::from_str(&i.to_json()).unwrap();
        let b: serde_json::Value = serde_json::from_str(&i.to_json_compact()).unwrap();
        assert_eq!(a, b);
    }

    /// Emission order must not depend on the order the driver walked the
    /// package, or the artifact churns and `write_if_changed` stops keeping
    /// mtimes still -- which re-stales every fixture keyed on it.
    #[test]
    fn emission_is_sorted_so_the_artifact_is_byte_stable() {
        let mut a = BoundInventory::new("p");
        let mut b = BoundInventory::new("p");
        let m = parse_message("int32 x\n").unwrap();
        for n in ["p/msg/C", "p/msg/A", "p/msg/B"] {
            a.record_message(n, &m, &CapacityResolver::empty(), &no_lookup);
        }
        for n in ["p/msg/B", "p/msg/C", "p/msg/A"] {
            b.record_message(n, &m, &CapacityResolver::empty(), &no_lookup);
        }
        assert_eq!(a.to_json(), b.to_json());
        assert_eq!(a.to_cmake(), b.to_cmake());
    }

    #[test]
    fn a_reason_cannot_break_out_of_the_cmake_string() {
        let mut i = BoundInventory::new("p");
        i.insert(
            "p/msg/M",
            BoundState::Unbounded {
                reason: "he said \"x\" and \\ then".to_string(),
            },
        );
        let c = i.to_cmake();
        assert!(c.contains(r#"\"x\""#), "{c}");
        assert!(c.contains(r"\\"), "{c}");
    }

    #[test]
    fn the_links_key_is_unique_per_package_and_a_legal_ident() {
        assert_eq!(
            BoundInventory::links_key("nav_msgs"),
            "nros_msgs_nav_msgs".to_string()
        );
        assert_eq!(
            BoundInventory::links_key("my-msgs"),
            "nros_msgs_my_msgs".to_string()
        );
    }

    /// The C++ pack's in-header `SERIALIZED_SIZE_MAX` is an ESTIMATE, and the
    /// inventory must never carry it. Pinned here rather than left as prose,
    /// because the direction matters: the estimate charges a FLAT 512 bytes per
    /// nested message, so a nested type whose own bound exceeds 512 makes the
    /// C++ constant SMALLER than the real bound. That is not a conservative
    /// over-estimate; it is a number that would under-size a receive buffer.
    #[test]
    fn the_cpp_packs_constant_under_estimates_a_large_nested_type() {
        let inner_src = "float64[100] samples\n";
        let outer_src = "p/Inner inner\n";
        let outer = rosidl_parser::parse_message(outer_src).unwrap();
        let lookup = |fqn: &str| -> Option<Message> {
            fqn.ends_with("Inner")
                .then(|| rosidl_parser::parse_message(inner_src).unwrap())
        };

        let mut i = BoundInventory::new("p");
        i.record_message("p/msg/Outer", &outer, &CapacityResolver::empty(), &lookup);
        let derived = match i.entries()[0].bound {
            BoundState::Bounded { rx, .. } => rx,
            ref other => panic!("expected a derived bound, got {other:?}"),
        };

        let cpp = crate::generate_cpp_message_package(
            "p",
            "Outer",
            &outer,
            "h",
            &crate::CapacityResolver::empty(),
        )
        .unwrap();
        let estimate: usize = cpp
            .header
            .lines()
            .find_map(|l| l.split("SERIALIZED_SIZE_MAX = ").nth(1))
            .and_then(|t| t.trim_end_matches(';').trim().parse().ok())
            .expect("the C++ header states a SERIALIZED_SIZE_MAX");

        // 100 doubles is 800 bytes of payload; the flat 512 cannot cover it.
        assert!(
            derived > 800,
            "the derived bound must actually bound the type: {derived}"
        );
        assert!(
            estimate < derived,
            "phase-403 W6 finding: the C++ pack estimates {estimate} where the \
             derived bound is {derived}. If this ever stops holding, the C++ \
             pack was fixed -- delete the test and say so, do not relax it."
        );
    }

    // -- phase-403 W0 -- a cap reaches the inventory, and BOTH transports agree --

    /// The exported inventory and the generated C header must say the same thing
    /// about the same type, because W6 made `BoundState::classify` shared
    /// between them precisely so they could not drift. Handing one a resolver
    /// and the other none would have reintroduced the drift through the
    /// argument list instead of through a second implementation, so the
    /// agreement is asserted over a type whose bound EXISTS ONLY BECAUSE OF THE
    /// CONFIG.
    #[test]
    fn a_capped_type_gets_the_same_number_in_the_inventory_and_the_header() {
        let m = parse_message("string label\nint64 v\n").unwrap();
        let caps = CapacityResolver::from_toml_str("[fields]\n\"p/M.label\" = 24\n").unwrap();

        let mut i = BoundInventory::new("p");
        i.record_message("p/msg/M", &m, &caps, &no_lookup);
        let (tx, rx) = match i.entries()[0].bound {
            BoundState::Bounded { tx, rx } => (tx, rx),
            ref other => panic!("a capped type must be bounded, got {other:?}"),
        };

        let header = crate::generate_c_message_package("p", "M", &m, "h", &caps)
            .unwrap()
            .header;
        let read = |suffix: &str| -> usize {
            header
                .lines()
                .find_map(|l| l.split(&format!("_{suffix}_MAX_SERIALIZED_SIZE ")).nth(1))
                .and_then(|t| t.trim().parse().ok())
                .unwrap_or_else(|| panic!("the header states a {suffix} bound:\n{header}"))
        };
        assert_eq!(read("TX"), tx);
        assert_eq!(read("RX"), rx);

        // Control: with no config the same `.msg` gets a number from NEITHER, so
        // the agreement above is about the cap and not about a type that was
        // bounded all along.
        let mut plain = BoundInventory::new("p");
        plain.record_message("p/msg/M", &m, &CapacityResolver::empty(), &no_lookup);
        assert_eq!(plain.entries()[0].bound.tag(), "unbounded");
        assert!(
            !crate::generate_c_message_package("p", "M", &m, "h", &CapacityResolver::empty())
                .unwrap()
                .header
                .contains("_TX_MAX_SERIALIZED_SIZE 3")
        );
    }

    /// One member reads as prose, several read as a list. The plural form is
    /// what makes a stock ROS type actionable in ONE build.
    #[test]
    fn the_reason_names_one_member_or_all_of_them() {
        assert_eq!(
            BoundState::unbounded_reason(&["a (string)".to_string()]),
            "unbounded member: a (string)"
        );
        assert_eq!(
            BoundState::unbounded_reason(&[
                "header.frame_id (string)".to_string(),
                "child_frame_id (string)".to_string(),
            ]),
            "unbounded members: header.frame_id (string), child_frame_id (string)"
        );
    }
}
