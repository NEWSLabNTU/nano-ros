//! RFC-0068 Stage 3 — Render.
//!
//! A runtime (`minijinja`) template engine over data packs. A language backend
//! is a set of `.jinja` templates plus a `serde`-serializable data context;
//! nothing about a language lives in Rust here. Templates are bundled at build
//! time via `include_str!` (fast, no I/O) and rendered from a view struct.
//!
//! Migration note (phase-335 W2/W3): backends move off the compile-time askama
//! templates one at a time. This module renders the converted ones (C, then the
//! rmw Rust backend) while askama still serves the rest.

use std::sync::LazyLock;

use minijinja::{Environment, value::ViaDeserialize};

/// The neutral facts a `CField` carries for the C-type pack filters (RFC-0068
/// step 2). Deserialized from the field's serialized form; extra CField keys are
/// ignored.
#[derive(serde::Deserialize)]
struct CTypeSpell {
    field_type: rosidl_parser::FieldType,
    is_configurable: bool,
    is_heap: bool,
    cap: usize,
    current_package: String,
}

/// Neutral facts the Rust-type pack filters (`rust_type_rmw` / `rust_type_idiomatic`)
/// compose the Rust type string from. `current_package` drives the self-ref
/// (`crate::` vs `pkg::`) choice inside `rust_type_for_field`.
#[derive(serde::Deserialize)]
struct RustTypeSpell {
    field_type: rosidl_parser::FieldType,
    current_package: String,
}

impl RustTypeSpell {
    fn pkg(&self) -> Option<&str> {
        (!self.current_package.is_empty()).then_some(self.current_package.as_str())
    }
}

/// Neutral facts the `cpp_type` / `cpp_array_suffix` pack filters compose the
/// C++ header type from.
#[derive(serde::Deserialize)]
struct CppTypeSpell {
    field_type: rosidl_parser::FieldType,
    is_borrowed: bool,
    is_heap: bool,
    cap: Option<usize>,
    current_package: String,
}

/// Neutral facts the `nros_type` pack filter composes the nros Rust type from.
#[derive(serde::Deserialize)]
struct NrosTypeSpell {
    field_type: rosidl_parser::FieldType,
    is_configurable: bool,
    is_heap: bool,
    cap: usize,
    mode: crate::types::NrosCodegenMode,
    current_package: String,
}

/// One environment holding every converted pack, keyed by a stable template
/// name (`"message.h"`, `"message_rmw.rs"`, …). Shared macros are registered
/// under the name their importers use (`_field.jinja`).
static ENV: LazyLock<Environment<'static>> = LazyLock::new(|| {
    let mut env = Environment::new();
    // Generated sources carry their own trailing newline in the template body;
    // do not let the engine append another.
    env.set_keep_trailing_newline(false);
    // Custom filter parity with the askama path (`templates::filters::snake_case`).
    env.add_filter("snake_case", |s: &str| crate::utils::to_snake_case(s));
    // RFC-0068 step 2 — the C type spelling composed in the pack from a CField's
    // neutral facts (was pre-baked as `CField.c_type` / `.array_suffix`).
    env.add_filter("c_type", |v: ViaDeserialize<CTypeSpell>| {
        let c = &v.0;
        let cp = (!c.current_package.is_empty()).then_some(c.current_package.as_str());
        crate::types::c_type_spelling(&c.field_type, c.is_configurable, c.is_heap, c.cap, cp)
    });
    env.add_filter("c_array_suffix", |v: ViaDeserialize<CTypeSpell>| {
        let c = &v.0;
        crate::types::c_array_suffix_spelling(&c.field_type, c.is_configurable, c.is_heap, c.cap)
    });
    // RFC-0068 step 2 — Rust type spelling in the pack (rmw layer = true,
    // idiomatic layer = false), was pre-baked as `RmwField/IdiomaticField.rust_type`.
    env.add_filter("rust_type_rmw", |v: ViaDeserialize<RustTypeSpell>| {
        crate::types::rust_type_for_field(&v.0.field_type, true, v.0.pkg())
    });
    env.add_filter("rust_type_idiomatic", |v: ViaDeserialize<RustTypeSpell>| {
        crate::types::rust_type_for_field(&v.0.field_type, false, v.0.pkg())
    });
    // RFC-0068 step 2 — C++ header type spelling in the pack, was pre-baked as
    // `CppField.cpp_type` / `.array_suffix`.
    env.add_filter("cpp_type", |v: ViaDeserialize<CppTypeSpell>| {
        let c = &v.0;
        let cp = (!c.current_package.is_empty()).then_some(c.current_package.as_str());
        crate::types::cpp_type_spelling(&c.field_type, c.is_borrowed, c.is_heap, c.cap, cp)
    });
    env.add_filter("cpp_array_suffix", |v: ViaDeserialize<CppTypeSpell>| {
        crate::types::cpp_array_suffix_spelling(&v.0.field_type, v.0.is_borrowed)
    });
    // RFC-0068 step 2 — nros embedded Rust type spelling in the pack (storage +
    // codegen-mode dependent), was pre-baked as `NrosField.rust_type`.
    env.add_filter("nros_type", |v: ViaDeserialize<NrosTypeSpell>| {
        let n = &v.0;
        let cp = (!n.current_package.is_empty()).then_some(n.current_package.as_str());
        crate::types::nros_type_spelling(
            &n.field_type,
            n.is_configurable,
            n.is_heap,
            n.cap,
            n.mode,
            cp,
        )
    });

    // --- C pack (packs/c) ---
    env.add_template("_field.jinja", include_str!("../packs/c/_field.jinja"))
        .expect("packs/c/_field.jinja must parse");
    env.add_template("message.h", include_str!("../packs/c/message.h.jinja"))
        .expect("packs/c/message.h.jinja must parse");
    env.add_template("message.c", include_str!("../packs/c/message.c.jinja"))
        .expect("packs/c/message.c.jinja must parse");
    env.add_template("service.h", include_str!("../packs/c/service.h.jinja"))
        .expect("packs/c/service.h.jinja must parse");
    env.add_template("service.c", include_str!("../packs/c/service.c.jinja"))
        .expect("packs/c/service.c.jinja must parse");
    env.add_template("action.h", include_str!("../packs/c/action.h.jinja"))
        .expect("packs/c/action.h.jinja must parse");
    env.add_template("action.c", include_str!("../packs/c/action.c.jinja"))
        .expect("packs/c/action.c.jinja must parse");

    // --- rmw Rust pack (packs/rmw) — RRR-compatible message layer ---
    env.add_template(
        "message_rmw.rs",
        include_str!("../packs/rmw/message.rs.jinja"),
    )
    .expect("packs/rmw/message.rs.jinja must parse");
    env.add_template(
        "service_rmw.rs",
        include_str!("../packs/rmw/service.rs.jinja"),
    )
    .expect("packs/rmw/service.rs.jinja must parse");
    env.add_template(
        "action_rmw.rs",
        include_str!("../packs/rmw/action.rs.jinja"),
    )
    .expect("packs/rmw/action.rs.jinja must parse");

    // --- nros embedded Rust pack (packs/nros) ---
    env.add_template(
        "nros_field.jinja",
        include_str!("../packs/nros/nros_field.jinja"),
    )
    .expect("packs/nros/nros_field.jinja must parse");
    env.add_template(
        "message_nros.rs",
        include_str!("../packs/nros/message.rs.jinja"),
    )
    .expect("packs/nros/message.rs.jinja must parse");
    env.add_template(
        "service_nros.rs",
        include_str!("../packs/nros/service.rs.jinja"),
    )
    .expect("packs/nros/service.rs.jinja must parse");
    env.add_template(
        "action_nros.rs",
        include_str!("../packs/nros/action.rs.jinja"),
    )
    .expect("packs/nros/action.rs.jinja must parse");

    // --- idiomatic Rust pack (packs/rust) ---
    env.add_template(
        "message_idiomatic.rs",
        include_str!("../packs/rust/message.rs.jinja"),
    )
    .expect("packs/rust/message.rs.jinja must parse");
    env.add_template(
        "service_idiomatic.rs",
        include_str!("../packs/rust/service.rs.jinja"),
    )
    .expect("packs/rust/service.rs.jinja must parse");
    env.add_template(
        "action_idiomatic.rs",
        include_str!("../packs/rust/action.rs.jinja"),
    )
    .expect("packs/rust/action.rs.jinja must parse");

    // --- C++ pack (packs/cpp) — headers + the Rust FFI glue ---
    env.add_template(
        "message_cpp.hpp",
        include_str!("../packs/cpp/message.hpp.jinja"),
    )
    .expect("packs/cpp/message.hpp.jinja must parse");
    env.add_template(
        "message_cpp_types.rs",
        include_str!("../packs/cpp/message_types.rs.jinja"),
    )
    .expect("packs/cpp/message_types.rs.jinja must parse");
    env.add_template(
        "message_cpp_exports.rs",
        include_str!("../packs/cpp/message_exports.rs.jinja"),
    )
    .expect("packs/cpp/message_exports.rs.jinja must parse");
    env.add_template(
        "service_cpp.hpp",
        include_str!("../packs/cpp/service.hpp.jinja"),
    )
    .expect("packs/cpp/service.hpp.jinja must parse");
    env.add_template(
        "action_cpp.hpp",
        include_str!("../packs/cpp/action.hpp.jinja"),
    )
    .expect("packs/cpp/action.hpp.jinja must parse");

    env
});

/// Render a bundled pack template with the given serializable context.
pub fn render(template: &str, ctx: impl serde::Serialize) -> Result<String, minijinja::Error> {
    ENV.get_template(template)?.render(ctx)
}

/// Back-compat alias for the C call sites (phase-335 W2).
pub fn render_c(template: &str, ctx: impl serde::Serialize) -> Result<String, minijinja::Error> {
    render(template, ctx)
}
