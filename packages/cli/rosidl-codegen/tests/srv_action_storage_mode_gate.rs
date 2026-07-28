//! Issues 0343 + 0344 — RFC-0033 storage modes on SERVICE and ACTION payloads.
//!
//! 0343: the modes were resolved for srv/action (`srv.rs`/`action.rs` call
//! `field_to_nros_field_with_mode` exactly like `msg.rs`) but implemented only in
//! the MESSAGE templates, so a heap-configured `.srv` field got the heap TYPE
//! with an owned serde body — generated code that does not compile.
//!
//! 0344: the Rust emitters now share the message deserialize arm
//! (`templates/_nros_field.jinja`), so `heap` works. Two rejections remain, and
//! each is a real capability gap rather than an oversight:
//!
//! * **C `heap`** — the C message emitter frees heap fields in a generated
//!   `{Struct}_fini()`; the C service/action templates emit no `_fini` at all,
//!   and every C consumer would have to learn to call it. Allocating structs
//!   nobody frees would be a leak.
//! * **`borrowed`** — works by emitting a `{Msg}View<'a>` alongside the owned
//!   struct; srv/action emit no view type, so the mode would silently degrade to
//!   `owned` (a wrong answer, not an error).
//!
//! These tests assert generated TEXT and diagnostics, so they need no toolchain
//! and run in the default lane — unlike the `#[ignore]`d `*_heap_compile_check`
//! suites (issue 0328).

use rosidl_codegen::{
    CapacityResolver, generate_c_action_package, generate_c_message_package,
    generate_c_service_package, generate_nros_inline_service, generate_nros_service_package,
};
use rosidl_parser::{parse_action, parse_message, parse_service};
use std::collections::HashSet;

const SRV: &str = "int64[] values\n---\nstring summary\n";
const ACT: &str = "int64[] waypoints\n---\nint64 total\n---\nint64 done\n";

fn resolver(entries: &str) -> CapacityResolver {
    CapacityResolver::from_toml_str(&format!("[fields]\n{entries}\n")).unwrap()
}

fn gen_srv(r: &CapacityResolver) -> Result<String, String> {
    generate_nros_service_package(
        "test_msgs",
        "Adder",
        &parse_service(SRV).unwrap(),
        &HashSet::new(),
        "0.1.0",
        "hash",
        "hash",
        "hash",
        r,
    )
    .map(|g| g.service_rs)
    .map_err(|e| e.to_string())
}

// ── 0344: heap now WORKS on Rust service/action payloads ────────────────────

/// The defect 0343 filed, inverted into a regression gate: the struct field type
/// and the deserialize body must agree. Before 0344 the struct said `heap::Vec`
/// while the body said `heapless::Vec::new()` — which does not compile, and
/// which no test would have caught.
#[test]
fn heap_service_field_gets_heap_type_and_heap_deserialize() {
    let rs = gen_srv(&resolver(
        r#""test_msgs/Adder_Request.values" = { cap = 8, mode = "heap" }"#,
    ))
    .expect("heap on a Rust service payload is supported since 0344");

    assert!(
        rs.contains("pub values: nros_core::heap::Vec<i64>"),
        "struct field must use the heap container:\n{rs}"
    );
    assert!(
        rs.contains("let mut vec = nros_core::heap::Vec::new();"),
        "deserialize must build the heap container:\n{rs}"
    );
    assert!(
        !rs.contains("let mut vec = heapless::Vec::new();"),
        "the owned arm must not survive alongside a heap field — that is the 0343 defect:\n{rs}"
    );
    // heap `push` is infallible, so the owned capacity mapping must be gone.
    assert!(
        rs.contains("vec.push(reader.read_i64()?);"),
        "heap push is infallible:\n{rs}"
    );
}

#[test]
fn heap_service_string_gets_heap_string() {
    let rs = gen_srv(&resolver(
        r#""test_msgs/Adder_Response.summary" = { cap = 8, mode = "heap" }"#,
    ))
    .expect("heap strings are supported on Rust service payloads");

    assert!(rs.contains("pub summary: nros_core::heap::String"), "{rs}");
    assert!(rs.contains("nros_core::heap::String::from(s)"), "{rs}");
}

#[test]
fn the_inline_service_emitter_supports_heap_too() {
    let inline = generate_nros_inline_service(
        "test_msgs",
        "Adder",
        &parse_service(SRV).unwrap(),
        "h",
        "h",
        "h",
        &resolver(r#""test_msgs/Adder_Request.values" = { cap = 8, mode = "heap" }"#),
    )
    .expect("the inline emitter shares the same macro");
    assert!(inline.contains("nros_core::heap::Vec"), "{inline}");
}

// ── still rejected, for stated reasons ──────────────────────────────────────

/// `borrowed` needs a view type that srv/action do not emit; without this gate it
/// would silently degrade to `owned`.
#[test]
fn borrowed_on_a_service_payload_is_rejected_with_the_field_named() {
    let err = gen_srv(&resolver(
        r#""test_msgs/Adder_Response.summary" = { cap = 8, mode = "borrowed" }"#,
    ))
    .expect_err("borrowed has no view type on srv/action");

    assert!(err.contains("summary"), "must name the field: {err}");
    assert!(err.contains("borrowed"), "must name the mode: {err}");
    assert!(err.contains("service"), "must name the entity: {err}");
}

/// C has no `_fini` on service/action payloads, so heap there would leak.
#[test]
fn heap_on_a_c_service_payload_is_rejected() {
    let r = resolver(r#""test_msgs/Adder_Request.values" = { cap = 8, mode = "heap" }"#);
    assert!(
        generate_c_service_package("test_msgs", "Adder", &parse_service(SRV).unwrap(), "h", &r)
            .is_err(),
        "C service payloads have no _fini to free heap fields with"
    );
}

#[test]
fn heap_on_a_c_action_payload_is_rejected() {
    let r = resolver(r#""test_msgs/Move_Goal.waypoints" = { cap = 8, mode = "heap" }"#);
    let err = match generate_c_action_package(
        "test_msgs",
        "Move",
        &parse_action(ACT).unwrap(),
        "h",
        &r,
    ) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("C action payloads have no _fini to free heap fields with"),
    };
    assert!(err.contains("waypoints"), "{err}");
    assert!(err.contains("action"), "{err}");
}

// ── the untouched paths ─────────────────────────────────────────────────────

/// Messages implement every mode; the srv/action policy must not touch them.
#[test]
fn heap_on_a_message_still_generates() {
    let r = resolver(r#""test_msgs/Values.values" = { cap = 8, mode = "heap" }"#);
    assert!(
        generate_c_message_package(
            "test_msgs",
            "Values",
            &parse_message("int64[] values\n").unwrap(),
            "h",
            &r
        )
        .is_ok(),
        "messages implement heap — the srv/action policy must not touch them"
    );
}

#[test]
fn owned_services_are_untouched() {
    let r = CapacityResolver::empty();
    assert!(
        generate_c_service_package("test_msgs", "Adder", &parse_service(SRV).unwrap(), "h", &r)
            .is_ok(),
        "owned must still generate"
    );
    assert!(
        gen_srv(&r).is_ok(),
        "owned Rust services must still generate"
    );
}
