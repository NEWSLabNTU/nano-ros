//! RFC-0068 Stage 3 — Render.
//!
//! A runtime (`minijinja`) template engine over data packs: a pack is a set of
//! `.jinja` templates rendered from a `serde`-serializable data context, so the
//! TEMPLATES are data and not Rust. A LANGUAGE is more than its templates — a
//! filter set (`crate::filters`) and a generator per kind — and RFC-0068
//! Amendment 2 measured it. Templates are bundled at build time via
//! `include_str!` (fast, no I/O) and rendered from a view struct.
//!
//! ## The registry is DISCOVERED (phase-469 W1)
//!
//! This module used to carry 28 authored `(key, include_str!(path))` rows, so a
//! new pack directory rendered nothing until someone remembered to add its rows
//! here. Each `packs/<dir>/pack.toml` now declares them — see `build.rs`, which
//! reads every manifest and generates [`PACKS`] from it. A pack directory is a
//! pack because it exists and describes itself; there is no list to join.
//!
//! The generated rows keep the ORDER the authored list had, from each manifest's
//! `registry_order`. That is not cosmetic: [`bundled_packs`] feeds
//! `codegen_fingerprint`, which hashes it as a SEQUENCE and stales every fixture
//! in the tree when it moves (RFC-0061 / phase-335 W4.a).
//!
//! Every backend AND the per-package scaffolding (Cargo/lib/build) render through
//! this one `Environment` now — askama is fully removed (phase-335 W6). Type
//! spelling that used to be pre-baked in the builders is composed in the pack by
//! filters (RFC-0068 step 2), and those filters are the LANGUAGE's contribution,
//! not this module's: see [`crate::filters`] for the set, who owns each one, and
//! the checks that keep the packs and the registry in agreement (RFC-0091 §6b /
//! phase-432 W2.5b). This module owns the templates, the loader and the globals.

use std::sync::LazyLock;

use minijinja::Environment;

// `PACKS` — every bundled pack template, keyed by the stable name a
// `render(name, …)` call and any `{% import %}` use.
//
// GENERATED: `build.rs` writes it from every `packs/<dir>/pack.toml`, doc
// comment included (a `///` here would attach to the `include!` invocation and
// document nothing, which `-D warnings` says out loud). A new language adds a
// pack DIRECTORY with a manifest and its `.jinja` files; its other Rust is a
// filter set (`crate::filters`) and a generator per kind (`crate::generator`).
// Nothing in this file names a template.
//
// Read the expansion at
// `packages/cli/target/**/build/rosidl-codegen-*/out/pack_registry.rs`.
include!(concat!(env!("OUT_DIR"), "/pack_registry.rs"));

/// What every discovered pack declares about itself.
///
/// GENERATED beside [`PACKS`] from the same manifests, so a test or a
/// diagnostic can ask which packs exist, which language each serves and which
/// registry keys it contributes without re-spelling a `pack.toml`.
mod info {
    include!(concat!(env!("OUT_DIR"), "/pack_info.rs"));
}

pub use info::{PACK_INFO, PackInfo};

/// Optional external pack directory (W4). When set (before the first render), a
/// file `<dir>/<name>` or `<dir>/<name>.jinja` OVERRIDES the bundled pack of that
/// name; anything absent falls back to bundled.
///
/// So this REPLACES the body of a registry key with no rebuild — which is what
/// `tests/external_pack_smoke.rs` proves, by writing over the bundled
/// `build.rs.jinja`. It does NOT add a language: the loader is only ever asked
/// for names the Rust generators request, so a file under a name no generator
/// requests is never read.
static OVERRIDE_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// Every bundled pack `(name, content)`. The codegen fingerprint (RFC-0061 /
/// phase-335 W4.a) hashes these so ANY pack edit — even one the emit corpus does
/// not exercise (rmw / idiomatic / scaffolding) — marks fixtures stale.
pub fn bundled_packs() -> &'static [(&'static str, &'static str)] {
    PACKS
}

/// Point the renderer at an external pack directory. Call once, before any
/// render (the `Environment` is built lazily on first use). Returns `Err` with
/// the passed dir if a directory was already set.
pub fn set_template_dir(dir: std::path::PathBuf) -> Result<(), std::path::PathBuf> {
    OVERRIDE_DIR.set(dir)
}

/// The environment. A `minijinja` loader resolves each name on demand: the
/// override dir wins (W4), else the bundled `PACKS`. Imports resolve the same way.
static ENV: LazyLock<Environment<'static>> = LazyLock::new(|| {
    let mut env = Environment::new();
    // Generated sources carry their own trailing newline in the template body;
    // do not let the engine append another.
    env.set_keep_trailing_newline(false);
    // RFC-0090 / phase-429 W1 — a GLOBAL, not a per-context field. Every
    // artifact this generator emits was emitted by this generator, so the
    // version is a property of the environment, not of any one message; adding
    // it to the six-odd context structs instead would be six places to forget.
    env.add_global(
        "codegen_version",
        crate::codegen_version::NROS_CODEGEN_VERSION,
    );
    // phase-432 W2.5b — the filters are not registered here any more. A
    // language's Rust surface area is a FILTER SET (`crate::filters`), so the
    // environment asks the registry rather than carrying ten `add_filter`
    // calls that say nothing about which language owns which spelling.
    crate::filters::register_all(&mut env);

    // W4.b — the override dir comes from `set_template_dir` (a CLI flag can call
    // it) or, with no cross-command plumbing, the `NROS_TEMPLATE_DIR` env var.
    let override_dir = OVERRIDE_DIR
        .get()
        .cloned()
        .or_else(|| std::env::var_os("NROS_TEMPLATE_DIR").map(std::path::PathBuf::from));
    env.set_loader(move |name| {
        if let Some(dir) = &override_dir {
            for cand in [dir.join(name), dir.join(format!("{name}.jinja"))] {
                match std::fs::read_to_string(&cand) {
                    Ok(s) => return Ok(Some(s)),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => {
                        return Err(minijinja::Error::new(
                            minijinja::ErrorKind::InvalidOperation,
                            format!("reading external pack {}: {e}", cand.display()),
                        ));
                    }
                }
            }
        }
        Ok(PACKS
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, s)| s.to_string()))
    });
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
