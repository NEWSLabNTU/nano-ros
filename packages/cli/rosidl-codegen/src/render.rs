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

use minijinja::Environment;

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
