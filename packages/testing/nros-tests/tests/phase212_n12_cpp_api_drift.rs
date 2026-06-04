//! Phase 212.N.12 / Phase 220.B — C++ API drift lint.
//!
//! Phase 212.N.12 (commits `3d77f1349` + `1ff17e10d`) renamed several
//! `nros-cpp` symbols used by example sources:
//!
//! * `nros::EntityKind` → `nros::NodeEntityKind`
//! * `NodeEntityDescriptor::id` → `NodeEntityDescriptor::stable_id`
//! * Generated `<Service>::SERVICE_NAME` / `SERVICE_HASH` /
//!   `<Action>::ACTION_NAME` / `ACTION_HASH` constants were dropped;
//!   examples now pass plain string literals (e.g.
//!   `"example_interfaces/srv/AddTwoInts"`) for `type_name`.
//!
//! Phase 220 Track B fixed the threadx-linux cpp examples that still
//! used the retired spellings. This lint scans `examples/**/cpp/**/*.cpp`
//! for any remaining occurrences so a future N.12-shaped rename sweep
//! that misses a downstream consumer fails the test suite instead of
//! the C++ compile.

use std::{
    fs,
    path::{Path, PathBuf},
};

const RETIRED_NEEDLES: &[(&str, &str)] = &[
    // Symbol → reason / replacement.
    ("nros::EntityKind", "use nros::NodeEntityKind"),
    ("::EntityKind::", "use ::NodeEntityKind::"),
    (".id = ", "field renamed to stable_id"),
    (
        "::SERVICE_NAME",
        "constant retired — use plain \"pkg/srv/Name\" literal",
    ),
    ("::SERVICE_HASH", "constant retired — use \"\" literal"),
    (
        "::ACTION_NAME",
        "constant retired — use plain \"pkg/action/Name\" literal",
    ),
    ("::ACTION_HASH", "constant retired — use \"\" literal"),
];

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR points at packages/testing/nros-tests/.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(3)
        .expect("repo root from manifest dir")
        .to_path_buf()
}

fn walk_cpp(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for ent in entries.flatten() {
        let p = ent.path();
        // Skip generated/build trees.
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if matches!(name, "build" | "generated")
            || name.starts_with("build-")
            || name.starts_with("target")
        {
            continue;
        }
        if p.is_dir() {
            walk_cpp(&p, out);
        } else if p.extension().and_then(|s| s.to_str()) == Some("cpp") {
            out.push(p);
        }
    }
}

#[test]
fn examples_cpp_have_no_retired_n12_symbols() {
    let examples = repo_root().join("examples");
    assert!(
        examples.is_dir(),
        "examples/ missing at {}",
        examples.display()
    );
    let mut files = Vec::new();
    walk_cpp(&examples, &mut files);
    assert!(
        !files.is_empty(),
        "scanner found no .cpp files under examples/ — walker broken?"
    );

    let mut violations = Vec::new();
    for file in &files {
        let Ok(text) = fs::read_to_string(file) else {
            continue;
        };
        for (lineno, line) in text.lines().enumerate() {
            // Skip the lint-test itself (this very file lives outside
            // examples/, but be defensive about other meta-files).
            // We also ignore commented-out hits.
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("*") {
                continue;
            }
            for (needle, hint) in RETIRED_NEEDLES {
                if line.contains(needle) {
                    violations.push(format!(
                        "{}:{}: contains retired N.12 symbol `{}` — {}",
                        file.strip_prefix(repo_root()).unwrap_or(file).display(),
                        lineno + 1,
                        needle,
                        hint,
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "retired Phase 212.N.12 C++ symbols still present in examples (\
         {} violation(s)):\n{}",
        violations.len(),
        violations.join("\n"),
    );
}
