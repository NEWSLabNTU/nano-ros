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
//! 0345: C `heap` also works now. The blocker recorded in 0343/0344 — "every C
//! consumer must be taught to free" — was a wrong model: `nros_service_callback_t`
//! hands the callback `request_data`/`request_len`, so nros-c never builds a typed
//! payload struct. The CALLER declares it and calls `_deserialize`, so the caller
//! `_fini`s it, exactly as for messages. The fix was the shared C arms
//! (`_c_field.jinja`) plus a generated `_fini` per payload struct.
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

/// 0346 — `borrowed` emits a `{Payload}View<'a>` beside the owned struct, so the
/// owned publish path is untouched and the view aliases the raw callback buffer.
/// Both directions read from a buffer (the server reads `request_data`, the client
/// reads `response`), so the lifetime story matches a subscription's.
#[test]
fn borrowed_service_payload_gets_a_view_type() {
    let rs = gen_srv(&resolver(
        r#""test_msgs/Adder_Response.summary" = { cap = 8, mode = "view" }"#,
    ))
    .expect("borrowed on a service payload is supported since 0346");

    assert!(
        rs.contains("pub struct AdderResponseView<'a>"),
        "a borrowed view must be emitted:\n{rs}"
    );
    assert!(
        rs.contains("pub summary: &'a str"),
        "the borrowed field must take the view type:\n{rs}"
    );
    assert!(
        rs.contains("impl<'a> nros_core::DeserializeBorrowed<'a> for AdderResponseView<'a>"),
        "the view must implement DeserializeBorrowed:\n{rs}"
    );
    // The OWNED struct must survive untouched — it is still the publish path.
    assert!(
        rs.contains("pub struct AdderResponse {"),
        "the owned struct must remain for serialization:\n{rs}"
    );
}

/// 0345 — C heap works, and the generated `_fini` is what makes the ownership
/// expressible. Without the fini the struct would allocate with no way to free.
#[test]
fn heap_on_a_c_service_payload_generates_with_a_fini() {
    let r = resolver(r#""test_msgs/Adder_Request.values" = { cap = 8, mode = "heap" }"#);
    let pkg =
        generate_c_service_package("test_msgs", "Adder", &parse_service(SRV).unwrap(), "h", &r)
            .expect("C heap on service payloads is supported since 0345");

    assert!(
        pkg.header
            .contains("void test_msgs_srv_adder_request_fini("),
        "header must declare the payload fini:\n{}",
        pkg.header
    );
    assert!(
        pkg.source.contains("nros_platform_free(msg->values.data)"),
        "fini must free the heap sequence:\n{}",
        pkg.source
    );
    assert!(
        pkg.source.contains("nros_platform_malloc"),
        "deserialize must allocate the heap sequence:\n{}",
        pkg.source
    );
    // The allocator seam must be included, or the TU fails with an implicit
    // declaration (that was the one real bug this change introduced).
    assert!(
        pkg.source.contains("#include <nros/platform.h>"),
        "the platform allocator header must be included:\n{}",
        pkg.source
    );
}

#[test]
fn heap_on_a_c_action_payload_generates_with_a_fini() {
    let r = resolver(r#""test_msgs/Move_Goal.waypoints" = { cap = 8, mode = "heap" }"#);
    let pkg = generate_c_action_package("test_msgs", "Move", &parse_action(ACT).unwrap(), "h", &r)
        .expect("C heap on action payloads is supported since 0345");
    assert!(
        pkg.header.contains("void test_msgs_action_move_goal_fini("),
        "header must declare the goal fini:\n{}",
        pkg.header
    );
    assert!(
        pkg.source
            .contains("nros_platform_free(msg->waypoints.data)"),
        "goal fini must free the heap sequence:\n{}",
        pkg.source
    );
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
