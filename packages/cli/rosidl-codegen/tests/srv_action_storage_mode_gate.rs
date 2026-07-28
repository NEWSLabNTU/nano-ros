//! Issue 0343 — a non-`owned` RFC-0033 storage mode on a SERVICE or ACTION
//! payload must fail at config time, not emit code that cannot compile.
//!
//! Background: `srv.rs` / `action.rs` resolve storage modes exactly like
//! `msg.rs` does, but only the MESSAGE templates implement them
//! (`message_nros.rs.jinja` has 12 `is_heap` branches, `message_c.c.jinja` 6;
//! the service/action templates have zero). So a heap-configured `.srv` field
//! used to get the heap TYPE in the struct with an owned-mode serde body —
//! generated Rust/C that does not compile, surfacing as a confusing rustc/cc
//! error over generated output instead of a diagnostic naming the field.
//!
//! These tests assert the DIAGNOSTIC, so unlike the `*_heap_compile_check`
//! suites they need no toolchain and run in the default lane.

use rosidl_codegen::{
    CapacityResolver, generate_c_action_package, generate_c_message_package,
    generate_c_service_package, generate_nros_inline_service, generate_nros_service_package,
};
use rosidl_parser::{parse_action, parse_message, parse_service};
use std::collections::HashSet;

/// `Adder.srv` with an unbounded sequence in the request and a string in the
/// response — the two shapes RFC-0033 makes configurable.
const SRV: &str = "int64[] values\n---\nstring summary\n";

fn heap_resolver(entries: &str) -> CapacityResolver {
    CapacityResolver::from_toml_str(&format!("[fields]\n{entries}\n")).unwrap()
}

fn deps() -> HashSet<String> {
    HashSet::new()
}

#[test]
fn heap_on_a_service_request_is_rejected_with_the_field_named() {
    let srv = parse_service(SRV).unwrap();
    let resolver =
        heap_resolver(r#""test_msgs/Adder_Request.values" = { cap = 64, mode = "heap" }"#);

    let generated = generate_nros_service_package(
        "test_msgs",
        "Adder",
        &srv,
        &deps(),
        "0.1.0",
        "hash",
        "hash",
        "hash",
        &resolver,
    );
    let err = match generated {
        Err(e) => e,
        Ok(_) => panic!("heap on a service request must be rejected, not emitted"),
    };

    let msg = err.to_string();
    assert!(
        msg.contains("values"),
        "diagnostic must name the field: {msg}"
    );
    assert!(msg.contains("heap"), "diagnostic must name the mode: {msg}");
    assert!(
        msg.contains("service"),
        "diagnostic must name the entity kind: {msg}"
    );
}

#[test]
fn borrowed_on_a_service_response_is_rejected() {
    let srv = parse_service(SRV).unwrap();
    let resolver =
        heap_resolver(r#""test_msgs/Adder_Response.summary" = { cap = 64, mode = "borrowed" }"#);

    let generated = generate_nros_service_package(
        "test_msgs",
        "Adder",
        &srv,
        &deps(),
        "0.1.0",
        "hash",
        "hash",
        "hash",
        &resolver,
    );
    let err = match generated {
        Err(e) => e,
        Ok(_) => panic!("borrowed on a service response must be rejected"),
    };
    assert!(err.to_string().contains("summary"), "{err}");
}

#[test]
fn the_inline_service_emitter_is_gated_too() {
    let srv = parse_service(SRV).unwrap();
    let resolver =
        heap_resolver(r#""test_msgs/Adder_Request.values" = { cap = 64, mode = "heap" }"#);

    assert!(
        generate_nros_inline_service("test_msgs", "Adder", &srv, "h", "h", "h", &resolver).is_err(),
        "the inline emitter shares the templates, so it needs the same gate"
    );
}

#[test]
fn the_c_service_emitter_is_gated_too() {
    let srv = parse_service(SRV).unwrap();
    let resolver =
        heap_resolver(r#""test_msgs/Adder_Request.values" = { cap = 64, mode = "heap" }"#);

    assert!(
        generate_c_service_package("test_msgs", "Adder", &srv, "hash", &resolver).is_err(),
        "service_c.{{h,c}}.jinja have no is_heap branches either"
    );
}

#[test]
fn heap_on_an_action_payload_is_rejected() {
    let action = parse_action("int64[] waypoints\n---\nint64 total\n---\nint64 done\n").unwrap();
    let resolver =
        heap_resolver(r#""test_msgs/Move_Goal.waypoints" = { cap = 64, mode = "heap" }"#);

    let err = match generate_c_action_package("test_msgs", "Move", &action, "hash", &resolver) {
        Err(e) => e,
        Ok(_) => panic!("heap on an action goal must be rejected, not emitted"),
    };
    let msg = err.to_string();
    assert!(msg.contains("waypoints"), "{msg}");
    assert!(
        msg.contains("action"),
        "diagnostic must name the entity: {msg}"
    );
}

/// The guard is scoped to srv/action payloads — MESSAGES genuinely implement
/// heap, and must keep working. This is the regression half of the fix: a
/// blanket rejection would have been the easy wrong answer.
#[test]
fn heap_on_a_message_still_generates() {
    let msg = parse_message("int64[] values\n").unwrap();
    let resolver = heap_resolver(r#""test_msgs/Values.values" = { cap = 64, mode = "heap" }"#);

    assert!(
        generate_c_message_package("test_msgs", "Values", &msg, "hash", &resolver).is_ok(),
        "messages implement heap — the srv/action gate must not touch them"
    );
}

/// An `owned` service (the default) is unaffected by the gate.
#[test]
fn owned_services_are_untouched() {
    let srv = parse_service(SRV).unwrap();
    let resolver = CapacityResolver::empty();

    assert!(
        generate_c_service_package("test_msgs", "Adder", &srv, "hash", &resolver).is_ok(),
        "owned is the supported mode and must still generate"
    );
}
