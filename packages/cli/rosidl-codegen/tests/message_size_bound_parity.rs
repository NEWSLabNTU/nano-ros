//! phase-408 W1 — one type, one number, in every emitter.
//!
//! The C header states `<PREFIX>_TX/RX_MAX_SERIALIZED_SIZE`, the C++ header
//! states `Msg::TX/RX_MAX_SERIALIZED_SIZE`, and the Rust runtime states
//! `M::MAX_SERIALIZED_SIZE_XCDR{1,2}`. All three must be the same fact.
//!
//! That is the whole reason this campaign exists. The sizes-header mirror
//! (0088 → 0114 → 0122 → 0123 → 0245 → 0268) is the defect where two
//! implementations of one size rule drift apart, and adding a C++ emitter for a
//! number the C emitter already had is exactly the shape it takes. So the C++
//! numbers are asserted equal to:
//!
//! * `nros_serdes::size::max_serialized_size` over the type's schema, which is
//!   the FUNCTION the Rust `Message::MAX_SERIALIZED_SIZE_XCDR*` const is
//!   computed by — same input, same function, so the constant a generated Rust
//!   crate carries is this number by construction; and
//! * the C header's own constants for the same type, so the two languages
//!   cannot state different bytes.
//!
//! Both encodings, because they genuinely differ: TX is XCDR1 (the only
//! encoding this stack writes) and RX is `max(XCDR1, XCDR2)` (a non-default
//! peer may negotiate either, RFC-0055).

use rosidl_codegen::{
    CapacityResolver, generate_c_message_package_with_lookup,
    generate_cpp_message_package_with_lookup,
    schema_value::{MsgLookup, build_schema},
};
use rosidl_parser::{Message, parse_message};

const SHAPES_MSG: &str = include_str!("fixtures/fingerprint-corpus/msg/Shapes.msg");
const NESTED_MSG: &str = include_str!("fixtures/fingerprint-corpus/msg/Nested.msg");
const BOUNDED_MSG: &str = include_str!("fixtures/fingerprint-corpus/msg/Bounded.msg");
const CAPPED_MSG: &str = include_str!("fixtures/fingerprint-corpus/msg/Capped.msg");
const CODEGEN_TOML: &str = include_str!("fixtures/fingerprint-corpus/nros-codegen.toml");

const PKG: &str = "fingerprint-corpus";

/// The corpus, resolved: `Nested` embeds `Shapes`, so a lookup is what turns
/// "could not be resolved" into a real classification. Without it the nested
/// arm is never exercised at all.
fn corpus_lookup(fqn: &str) -> Option<Message> {
    let leaf = fqn.rsplit('/').next()?;
    match leaf {
        "Shapes" => parse_message(SHAPES_MSG).ok(),
        "Nested" => parse_message(NESTED_MSG).ok(),
        "Bounded" => parse_message(BOUNDED_MSG).ok(),
        "Capped" => parse_message(CAPPED_MSG).ok(),
        _ => None,
    }
}

/// What `M::MAX_SERIALIZED_SIZE_XCDR{1,2}` would be for this type: the same
/// `nros_serdes::size::max_serialized_size` the runtime const calls, over the
/// same schema. `None` means UNBOUNDED — never "unknown"; an unresolvable
/// nested type panics rather than reporting no bound, which is phase-380's rule
/// and the trap issue 0896 layer 1 names.
fn rust_consts(
    message_name: &str,
    msg: &Message,
    caps: &CapacityResolver,
    lookup: &MsgLookup<'_>,
) -> (Option<usize>, Option<usize>) {
    use nros_serdes::cdr::EncodingVersion;
    let fields = build_schema(&format!("{PKG}/{message_name}"), msg, caps, lookup)
        .unwrap_or_else(|e| panic!("{message_name}: schema could not be built: {e:?}"));
    (
        nros_serdes::size::max_serialized_size(fields, EncodingVersion::Xcdr1),
        nros_serdes::size::max_serialized_size(fields, EncodingVersion::Xcdr2),
    )
}

/// `Msg::TX_MAX_SERIALIZED_SIZE` / `Msg::RX_MAX_SERIALIZED_SIZE` as the C++
/// header states them, or `None` when the header states neither.
fn cpp_constants(header: &str) -> (Option<usize>, Option<usize>) {
    let read = |dir: &str| -> Option<usize> {
        header
            .lines()
            .find_map(|l| {
                l.split(&format!("size_t {dir}_MAX_SERIALIZED_SIZE = "))
                    .nth(1)
            })
            .and_then(|t| t.trim().trim_end_matches(';').parse().ok())
    };
    (read("TX"), read("RX"))
}

/// The C header's constants for the same type.
fn c_constants(header: &str) -> (Option<usize>, Option<usize>) {
    let read = |dir: &str| -> Option<usize> {
        header
            .lines()
            .find_map(|l| l.split(&format!("_{dir}_MAX_SERIALIZED_SIZE ")).nth(1))
            .and_then(|t| t.trim().parse().ok())
    };
    (read("TX"), read("RX"))
}

fn resolvers() -> Vec<(&'static str, CapacityResolver)> {
    vec![
        ("inline", CapacityResolver::empty()),
        (
            "configured",
            CapacityResolver::from_toml_str(CODEGEN_TOML).expect("corpus nros-codegen.toml parses"),
        ),
    ]
}

/// THE claim: for every corpus type under every resolver, in both encodings,
/// the C++ header, the C header and `max_serialized_size` agree.
#[test]
fn the_cpp_header_states_the_same_bound_as_the_c_header_and_the_rust_const() {
    let msgs = [
        ("Shapes", SHAPES_MSG),
        ("Nested", NESTED_MSG),
        ("Bounded", BOUNDED_MSG),
        ("Capped", CAPPED_MSG),
    ];

    let mut bounded_seen = 0usize;
    let mut unbounded_seen = 0usize;

    for (mode, caps) in resolvers() {
        for (name, src) in &msgs {
            let msg = parse_message(src).unwrap_or_else(|e| panic!("{name} parses: {e:?}"));
            let lookup: &MsgLookup<'_> = &corpus_lookup;

            let (x1, x2) = rust_consts(name, &msg, &caps, lookup);
            // TX writes XCDR1, exactly: we write what we serialise.
            let want_tx = x1;
            // RX must hold whichever encoding arrives, AND the framing the
            // transport adds on top of it — `transport_framed`, which is the
            // sender's RTPS 4-byte submessage alignment (issues 0969/0970), not
            // padding anyone chose.
            //
            // Called, not restated. This expectation was written as a bare
            // `a.max(b)` one commit AFTER `ec63d4ed9` introduced the framing, so
            // it asserted the pre-framing number and went red the moment the C++
            // pack started emitting a bound at all. An open-coded `+ 4` here
            // would be a THIRD implementation of the same rounding — there are
            // already two, `rosidl_codegen::bounds::transport_framed` and
            // `nros_node::rmw_type_registry::transport_framed`, which cite each
            // other precisely so they cannot drift.
            let want_rx = match (x1, x2) {
                (Some(a), Some(b)) => Some(rosidl_codegen::bounds::transport_framed(a.max(b))),
                _ => None,
            };

            let cpp = generate_cpp_message_package_with_lookup(PKG, name, &msg, "h", &caps, lookup)
                .unwrap_or_else(|e| panic!("{mode}/{name}: C++ generate failed: {e}"))
                .header;
            let c = generate_c_message_package_with_lookup(PKG, name, &msg, "h", &caps, lookup)
                .unwrap_or_else(|e| panic!("{mode}/{name}: C generate failed: {e}"))
                .header;

            let (cpp_tx, cpp_rx) = cpp_constants(&cpp);
            let (c_tx, c_rx) = c_constants(&c);

            assert_eq!(
                cpp_tx, want_tx,
                "{mode}/{name}: the C++ header's TX bound must be the XCDR1 \
                 max_serialized_size, which is what M::MAX_SERIALIZED_SIZE_XCDR1 is"
            );
            assert_eq!(
                cpp_rx, want_rx,
                "{mode}/{name}: the C++ header's RX bound must be \
                 max(MAX_SERIALIZED_SIZE_XCDR1, MAX_SERIALIZED_SIZE_XCDR2)"
            );
            assert_eq!(
                (cpp_tx, cpp_rx),
                (c_tx, c_rx),
                "{mode}/{name}: the C and C++ headers state different bounds for one type — \
                 that is the sizes-header mirror class along the language axis"
            );

            if want_tx.is_some() {
                bounded_seen += 1;
            } else {
                unbounded_seen += 1;
                // The unbounded arm states no constant at all, and poisons the
                // uniform spelling instead.
                assert!(
                    cpp.contains("size_bound_dependent_false"),
                    "{mode}/{name}: an unbounded type must POISON tx_size_bound/rx_size_bound, \
                     not simply omit them — an absent member reports \"no member named\", which \
                     names neither the type nor the offending field:\n{cpp}"
                );
            }
        }
    }

    // Negative control: a corpus that was all-bounded (or all-unbounded) would
    // pass the equalities above while exercising one arm. Both must be reached.
    assert!(
        bounded_seen >= 3 && unbounded_seen >= 3,
        "the corpus must exercise BOTH arms: {bounded_seen} bounded, {unbounded_seen} unbounded"
    );
}

/// A resolvable nested type must produce a REAL bound in the C++ header, not
/// "unresolved". Without this, the equality above could hold trivially by both
/// emitters failing to resolve anything.
#[test]
fn a_resolved_nested_type_gets_a_number_and_an_unresolved_one_gets_none() {
    let outer = parse_message("Inner inner\nint32 tag\n").unwrap();
    let inner_src = "int64 a\nfloat64 b\n";
    let lookup = |fqn: &str| -> Option<Message> {
        fqn.ends_with("Inner")
            .then(|| parse_message(inner_src).unwrap())
    };
    let caps = CapacityResolver::empty();

    let resolved =
        generate_cpp_message_package_with_lookup("p", "Outer", &outer, "h", &caps, &lookup)
            .unwrap()
            .header;
    let (tx, rx) = cpp_constants(&resolved);
    assert_eq!(
        (tx, rx),
        rust_nested_expectation(&outer, &caps, &lookup),
        "a resolved nested type must state the derived bound"
    );
    assert!(
        tx.is_some(),
        "the resolved case must be bounded:\n{resolved}"
    );

    // Same message, no resolver: the header must state NO number and say the
    // nested type could not be resolved. "could not look" is not "no bound
    // exists", and a guessed number here is the defect this campaign is about.
    let unresolved =
        generate_cpp_message_package_with_lookup("p", "Outer", &outer, "h", &caps, &|_| None)
            .unwrap()
            .header;
    assert_eq!(cpp_constants(&unresolved), (None, None));
    assert!(
        unresolved.contains("NROS_UNRESOLVED__p_msg_outer__nested_type_p_Inner"),
        "the poison token must say the bound was NOT COMPUTED, and name the nested \
         type:\n{unresolved}"
    );
}

fn rust_nested_expectation(
    msg: &Message,
    caps: &CapacityResolver,
    lookup: &MsgLookup<'_>,
) -> (Option<usize>, Option<usize>) {
    use nros_serdes::cdr::EncodingVersion;
    let fields = build_schema("p/Outer", msg, caps, lookup).expect("schema builds");
    let x1 = nros_serdes::size::max_serialized_size(fields, EncodingVersion::Xcdr1);
    let x2 = nros_serdes::size::max_serialized_size(fields, EncodingVersion::Xcdr2);
    (
        x1,
        match (x1, x2) {
            (Some(a), Some(b)) => Some(a.max(b)),
            _ => None,
        },
    )
}
