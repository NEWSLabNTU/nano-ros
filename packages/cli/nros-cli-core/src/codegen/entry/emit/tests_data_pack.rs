//! phase-474 W5 — the acceptance test for "a new entry language needs no Rust
//! emitter": a pack that is ONLY data renders an entry.
//!
//! The pack is `testdata/entry-packs/zig/` — a `pack.toml` and one template,
//! the language RFC-0091 §8b used as its probe. Nothing in this crate knows it
//! exists except this test, and this test adds nothing for it: it loads the
//! manifest through the same [`PackManifest`] the bundled packs parse into,
//! registers exactly the filters the manifest DECLARES (so the declaration is
//! what it renders with — an undeclared filter fails the render), and admits
//! and lowers each plan through [`admit_and_lower`], the code `nros codegen
//! entry` runs for the C and C++ packs.
//!
//! Every golden plan the C pack renders is rendered here too, and each result
//! is a golden (`testdata/entry/toy_<case>.zig.golden`). They are NOT compiled:
//! the host that wrote them has no Zig toolchain, so they pin what the pack
//! renders, not that a compiler accepts it. What they DO prove is RFC-0091
//! §8b's claim, by construction: a third language renders from `LoweredEntry`
//! without a Stage 2 change.
//!
//! Recording a NEW toy golden takes `NROS_RECORD_TOY_PACK_GOLDEN=1`, scoped to
//! this test; it deliberately is not `NROS_UPDATE_GOLDEN`, which re-records the
//! shipping packs' goldens too.

use std::path::{Path, PathBuf};

use minijinja::Environment;

use super::admit_and_lower;
use crate::codegen::entry::{
    golden::{Emitter, cases},
    lower::LowerOptions,
    pack::{PackContext, PackManifest, manifests},
};

fn pack_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/entry-packs/zig")
}

/// The data-only pack, loaded the way a bundled one is parsed.
fn load() -> (PackManifest, Environment<'static>) {
    let dir = pack_dir();
    let text = std::fs::read_to_string(dir.join("pack.toml")).expect("the toy pack's manifest");
    let manifest: PackManifest = toml::from_str(&text).expect("the toy manifest parses");
    let mut env = Environment::new();
    env.set_keep_trailing_newline(true);
    super::super::filters::register_declared(&mut env, &manifest.filters)
        .expect("every filter the toy declares is registered");
    for row in &manifest.templates {
        let src = std::fs::read_to_string(dir.join(&row.file)).expect("the toy's template");
        // `add_template` wants `'static`; a test leaking two small strings is
        // simpler than enabling minijinja's owned-template feature for it.
        env.add_template(
            Box::leak(row.key.clone().into_boxed_str()),
            Box::leak(src.into_boxed_str()),
        )
        .expect("the toy template parses");
    }
    (manifest, env)
}

#[test]
fn the_toy_pack_is_data_and_names_no_language() {
    let (m, _) = load();
    assert!(m.fixture, "the toy must declare itself a fixture");
    assert_eq!(
        m.language, None,
        "a fixture names no `Language` — that is the one Rust a real language adds"
    );
    assert_eq!(m.context, Some(PackContext::LoweredEntry));
    assert!(
        !pack_dir().join("emit.rs").exists(),
        "a data-only pack carries no Rust"
    );
}

/// Every plan the C pack renders, rendered by the toy, against a golden.
#[test]
fn a_pack_added_as_data_renders_every_c_golden_plan() {
    let (m, env) = load();
    let all = manifests().expect("bundled manifests parse");
    let template = m
        .entry_template
        .clone()
        .expect("the toy names its entry template");
    let record = std::env::var_os("NROS_RECORD_TOY_PACK_GOLDEN").is_some();
    let goldens = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/entry");

    let mut drift = Vec::new();
    let mut rendered = 0usize;
    for (name, plan, emitter) in cases() {
        if emitter != Emitter::C {
            continue;
        }
        let got =
            admit_and_lower("zig", &m, &all, &plan, &LowerOptions::default()).and_then(|lowered| {
                env.get_template(&template)
                    .and_then(|t| t.render(&lowered))
                    .map_err(|e| format!("toy template failed to render: {e:#}"))
            });
        let got = match got {
            Ok(s) => {
                rendered += 1;
                s
            }
            Err(e) => format!("PACK REFUSED: {e}\n"),
        };
        let path = goldens.join(format!("toy_{name}.zig.golden"));
        if record {
            std::fs::write(&path, &got).expect("record toy golden");
            continue;
        }
        match std::fs::read_to_string(&path) {
            Ok(want) if want == got => {}
            Ok(_) => drift.push(format!("  {name}: differs from {}", path.display())),
            Err(_) => drift.push(format!("  {name}: no golden at {}", path.display())),
        }
    }
    // A pack that refused every plan would pass a golden comparison of its
    // refusals; the point is that it RENDERS.
    assert!(
        rendered >= 10,
        "the toy rendered {rendered} plans — it must render, not just refuse"
    );
    assert!(
        drift.is_empty(),
        "toy pack goldens drifted:\n{}",
        drift.join("\n")
    );
}

/// The declaration is what the pack renders with: drop a filter it calls from
/// its manifest, and the render fails rather than finding the filter anyway.
#[test]
fn a_pack_renders_with_only_the_filters_it_declares() {
    let (mut m, _) = load();
    m.filters.retain(|f| f != "pkg_ident");
    let mut env = Environment::new();
    super::super::filters::register_declared(&mut env, &m.filters).unwrap();
    let src = std::fs::read_to_string(pack_dir().join("entry.zig.jinja")).unwrap();
    env.add_template("t", Box::leak(src.into_boxed_str()))
        .unwrap();
    let all = manifests().unwrap();
    let (_, plan, _) = cases()
        .into_iter()
        .find(|(n, _, _)| *n == "c_native_one")
        .expect("the c_native_one golden plan");
    let lowered = admit_and_lower("zig", &m, &all, &plan, &LowerOptions::default()).unwrap();
    let err = env
        .get_template("t")
        .unwrap()
        .render(&lowered)
        .expect_err("an undeclared filter must not be found");
    assert_eq!(err.kind(), minijinja::ErrorKind::UnknownFilter, "{err}");
}
