//! Phase 212.M.5.a.1 — `nros::component!()` per-pkg symbol mangling.
//!
//! Pre-212.M.5.a.1 the macro emitted a hardcoded
//! `__nros_component_register` symbol. Linking two Component pkg
//! crates into one binary therefore failed with duplicate symbols —
//! every multi-component bringup (the H.4 ThreadX fixture, future
//! Pattern-B native bringups) was unbuildable.
//!
//! This test stages two minimal Component pkg crates (`talker_pkg`,
//! `listener_pkg`) into a tempdir, both calling
//! `nros::component!(UserType)`, and a top binary that depends on both.
//! When the macro mangles `__nros_component_<pkg>_register` per pkg the
//! link succeeds; otherwise rustc / lld reports a multiple-definition
//! error.
//!
//! Skip semantics mirror the rest of the Phase 212 suite: `nros_tests::skip!`
//! when prereqs (the workspace's own `cargo`) are missing.

use std::{fs, path::PathBuf, process::Command};

fn workspace_root() -> PathBuf {
    nros_tests::project_root()
}

fn write(path: &std::path::Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn pkg_lib_rs(user_ty: &str) -> String {
    format!(
        r#"#![no_std]

pub struct {ty};

impl nros::Component for {ty} {{
    const NAME: &'static str = "{ty_lower}";
    fn register(_ctx: &mut nros::ComponentContext<'_>) -> nros::ComponentResult<()> {{
        Ok(())
    }}
}}

// Phase 212.M.5.a.4 — the macro now emits parallel `_init` / `_dispatch`
// / `_tick` symbols alongside `_register`, all of which call into
// `ExecutableComponent`. Declarative pkgs satisfy that with the no-op
// blanket via `declarative_component!`.
nros::declarative_component!({ty});

nros::component!({ty});
"#,
        ty = user_ty,
        ty_lower = user_ty.to_lowercase(),
    )
}

fn pkg_cargo_toml(name: &str, nros_path: &str) -> String {
    format!(
        r#"[package]
name = "{name}"
version = "0.0.1"
edition = "2024"
publish = false

[lib]
crate-type = ["rlib"]
path = "src/lib.rs"

[dependencies]
nros = {{ path = "{nros_path}", default-features = false, features = ["std"] }}

[workspace]
"#,
    )
}

fn bin_cargo_toml(nros_path: &str, talker: &str, listener: &str) -> String {
    format!(
        r#"[package]
name = "two_component_bin"
version = "0.0.1"
edition = "2024"
publish = false

[[bin]]
name = "two_component_bin"
path = "src/main.rs"

[dependencies]
nros = {{ path = "{nros_path}", default-features = false, features = ["std"] }}
talker_pkg = {{ path = "{talker}" }}
listener_pkg = {{ path = "{listener}" }}

[workspace]
"#,
    )
}

fn bin_main_rs() -> &'static str {
    r#"// Phase 212.M.5.a.1 — link two Component pkg crates into one binary.
//
// The macros emit `__nros_component_talker_pkg_register` and
// `__nros_component_listener_pkg_register`; both are pulled in via the
// `#[used]` export-marker statics. We reference the crates to keep
// rustc from gc'ing them.

#[allow(unused_imports)]
use talker_pkg as _;
#[allow(unused_imports)]
use listener_pkg as _;

fn main() {
    println!("two_component_bin: linked");
}
"#
}

fn try_cargo() -> Option<()> {
    Command::new("cargo")
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|_| ())
}

#[test]
fn two_component_pkgs_link_into_one_binary() {
    if try_cargo().is_none() {
        nros_tests::skip!("cargo not on PATH");
    }

    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let nros_path = workspace_root().join("packages/core/nros");
    let nros_path_str = nros_path.to_str().unwrap();

    // talker_pkg
    let talker_dir = root.join("talker_pkg");
    write(&talker_dir.join("src/lib.rs"), &pkg_lib_rs("Talker"));
    write(
        &talker_dir.join("Cargo.toml"),
        &pkg_cargo_toml("talker_pkg", nros_path_str),
    );

    // listener_pkg
    let listener_dir = root.join("listener_pkg");
    write(&listener_dir.join("src/lib.rs"), &pkg_lib_rs("Listener"));
    write(
        &listener_dir.join("Cargo.toml"),
        &pkg_cargo_toml("listener_pkg", nros_path_str),
    );

    // top binary linking both
    let bin_dir = root.join("two_component_bin");
    write(&bin_dir.join("src/main.rs"), bin_main_rs());
    write(
        &bin_dir.join("Cargo.toml"),
        &bin_cargo_toml(
            nros_path_str,
            talker_dir.to_str().unwrap(),
            listener_dir.to_str().unwrap(),
        ),
    );

    // Build the bin. With per-pkg mangling both pkg crates expose
    // distinct register symbols and the linker is happy. Pre-fix, the
    // shared `__nros_component_register` collides on link.
    let out = Command::new("cargo")
        .arg("build")
        .arg("--manifest-path")
        .arg(bin_dir.join("Cargo.toml"))
        // Put cargo's target outside the workspace tmp so a host with a
        // populated sccache / shared registry warms up.
        .current_dir(root)
        .output()
        .expect("spawn cargo build");

    assert!(
        out.status.success(),
        "two-component bin failed to link (per-pkg mangle missing?):\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    // Phase 212.M.5.a.4 — confirm the new `_dispatch` / `_init` / `_tick`
    // symbols also got per-pkg mangling so they don't collide on link.
    // We probe each pkg's rlib (final bin may dead-strip unused exports)
    // with `nm` (best-effort: skip silently if not on PATH).
    if let Some(nm) = which_nm() {
        for pkg in ["talker_pkg", "listener_pkg"] {
            let rlibs = find_rlibs(&bin_dir.join("target/debug/deps"), pkg);
            assert!(
                !rlibs.is_empty(),
                "couldn't locate rlib for `{pkg}` under {}",
                bin_dir.join("target/debug/deps").display()
            );
            let out = match Command::new(&nm).arg(&rlibs[0]).output() {
                Ok(o) if o.status.success() => o,
                _ => continue,
            };
            let text = String::from_utf8_lossy(&out.stdout);
            for suffix in ["register", "init", "dispatch", "tick"] {
                let sym = format!("__nros_component_{pkg}_{suffix}");
                assert!(
                    text.contains(&sym),
                    "expected symbol `{sym}` in rlib `{}`:\n{text}",
                    rlibs[0].display()
                );
            }
        }
    }
}

fn find_rlibs(deps_dir: &std::path::Path, pkg: &str) -> Vec<PathBuf> {
    let prefix = format!("lib{pkg}-");
    fs::read_dir(deps_dir)
        .into_iter()
        .flat_map(|d| d.flatten())
        .map(|e| e.path())
        .filter(|p| {
            p.extension().and_then(|x| x.to_str()) == Some("rlib")
                && p.file_name()
                    .and_then(|x| x.to_str())
                    .is_some_and(|n| n.starts_with(&prefix))
        })
        .collect()
}

fn which_nm() -> Option<String> {
    for cand in ["llvm-nm", "nm"] {
        if Command::new(cand)
            .arg("--version")
            .output()
            .ok()
            .is_some_and(|o| o.status.success())
        {
            return Some(cand.to_string());
        }
    }
    None
}
