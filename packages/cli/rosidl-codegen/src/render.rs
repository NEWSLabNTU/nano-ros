//! RFC-0068 Stage 3 — Render.
//!
//! A runtime (`minijinja`) template engine over data packs. A language backend
//! is a directory of `.jinja` templates plus a data context; nothing about a
//! language lives in Rust here. Templates are bundled at build time via
//! `include_str!` (fast, no I/O) and rendered from a `serde`-serializable
//! context (a lowered/view struct).
//!
//! Migration note (phase-335 W2): backends move off the compile-time askama
//! templates one at a time; this module renders the ones already converted (C
//! first) while askama still serves the rest.

use std::sync::LazyLock;

use minijinja::Environment;

/// The C-backend template pack, bundled. Templates are authored in the
/// `minijinja` dialect under `packs/c/`.
static C_ENV: LazyLock<Environment<'static>> = LazyLock::new(|| {
    let mut env = Environment::new();
    // The generated sources carry their own trailing newline in the template
    // body, so do not let the engine append another one.
    env.set_keep_trailing_newline(false);
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
    env
});

/// Render a C-pack template with the given serializable context.
pub fn render_c(template: &str, ctx: impl serde::Serialize) -> Result<String, minijinja::Error> {
    C_ENV.get_template(template)?.render(ctx)
}
