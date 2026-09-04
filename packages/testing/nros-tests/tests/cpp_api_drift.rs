//! C++ API drift guard (Phase 212.N.12 / 220.B).
//!
//! Two guards:
//!  1. A **static lint** over `examples/**/cpp/**/*.cpp` for symbols retired by
//!     the N.12 rename (`nros::EntityKind` → `NodeEntityKind`, `.id` →
//!     `.stable_id`, the dropped `::SERVICE_NAME`/`HASH` + `::ACTION_NAME`/`HASH`
//!     constants). No compilation — pure text scan.
//!  2. Two **C++ compat snippets** that must type-check against the public
//!     `nros-cpp` / `nros-c` / compat headers. Per issue 0034 / AGENTS.md "No
//!     compilation inside tests", these compile in the **build stage** — the
//!     `cpp_compat_snippets/*.cpp` fixtures are `c++ -fsyntax-only`'d by
//!     `compile-check-fixtures.sh` (run by `build-test-fixtures`), which stamps
//!     `.compile-ok`. The tests assert the stamps, and a missing stamp is a
//!     FAILURE in the full tier (`[SKIPPED]` only under
//!     `NROS_FIXTURES_OPTIONAL=1`) — see `assert_snippet_compiled`. The build
//!     stage still does not block `build-test-fixtures` on a snippet that will
//!     not compile; reporting it is this file's job, and until issue 1032 it
//!     was not being done.

use nros_tests::TestResult;
use std::{
    fs,
    path::{Path, PathBuf},
};

const RETIRED_NEEDLES: &[(&str, &str)] = &[
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

fn walk_cpp(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for ent in entries.flatten() {
        let p = ent.path();
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if nros_tests::treewalk::is_skipped_dir(name) {
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
fn examples_cpp_have_no_retired_symbols() {
    let examples = nros_tests::project_root().join("examples");
    assert!(
        examples.is_dir(),
        "examples/ missing at {}",
        examples.display()
    );
    let mut files = Vec::new();
    walk_cpp(&examples, &mut files);
    assert!(
        !files.is_empty(),
        "scanner found no .cpp files under examples/"
    );

    let mut violations = Vec::new();
    for file in &files {
        let Ok(text) = fs::read_to_string(file) else {
            continue;
        };
        for (lineno, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            for (needle, hint) in RETIRED_NEEDLES {
                if line.contains(needle) {
                    violations.push(format!(
                        "{}:{}: contains retired symbol `{}` — {}",
                        file.strip_prefix(nros_tests::project_root())
                            .unwrap_or(file)
                            .display(),
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
        "retired C++ symbols still present in examples ({} violation(s)):\n{}",
        violations.len(),
        violations.join("\n"),
    );
}

/// Assert a cpp-compat-snippet's build-stage `.compile-ok` stamp.
///
/// issue 1032 — this used to catch `Err(_)` and `skip!` UNCONDITIONALLY, which
/// made a broken snippet green at both ends: the build stage already declines
/// to fail (`cxx-syntax FAILED for <id> (no stamp; consuming test will
/// report)`) on the understanding that the consuming test reports it, and the
/// consuming test then reported nothing. Three snippets failed every scheduled
/// run from at least 2026-09-01 and no lane went red.
///
/// The skip cited issue 0034 as a tracked pre-existing drift. 0034 was resolved
/// and archived on 2026-06-12, and its named cause here — "needs generated
/// config headers" — was issue 1031, fixed. The excuse outlived the defect by
/// three months.
///
/// `?` rather than a bare `assert!`, because the tier policy the old code threw
/// away already lives in `require_compile_check`: hard-fail in the full tier,
/// `[SKIPPED]` under `NROS_FIXTURES_OPTIONAL=1`. This restores that policy
/// instead of writing a second one — the sibling consumer
/// (`platform_header_compile.rs`) has always done it this way.
fn assert_snippet_compiled(id: &str) -> TestResult<()> {
    let stamp = nros_tests::fixtures::require_compile_check(id)?;
    assert!(
        stamp.exists(),
        "compile-ok stamp missing for `{id}`: {}\n\
         The snippet did not compile at the build stage. Run \
         `just build-test-fixtures` and read its log for the compile error — \
         these snippets are the only compile coverage for the public \
         `nros.hpp` surface.",
        stamp.display()
    );
    Ok(())
}

// Phase-257 Stage-3b — the `declared_node_typed_helpers` snippet exercised the
// retired declarative seam (`DeclaredNode`/`DeclaredEntity`/`DeclaredCallback`);
// removed with the seam. The typed surface is guarded by the component examples
// (`configure(Node&)` + `Publisher<M>` + `bind_timer`).

#[test]
fn rclcpp_node_options_and_component_factory_compile() -> TestResult<()> {
    assert_snippet_compiled("rclcpp_node_options")
}

/// phase-277 W5 — the callback-style subscription-with-attachment path
/// (`Node::create_subscription_with_info`) keeps compile coverage via a
/// dedicated snippet; it used to live as an `if (false)` block inside the
/// `examples/native/cpp/listener` example (examples stay demo-only).
#[test]
fn create_subscription_with_info_compiles() -> TestResult<()> {
    assert_snippet_compiled("subscription_with_info")
}

/// issue 1032 — `spin_until_future_complete.cpp` was compiled by the build
/// stage and asserted by NOTHING. Not even the unconditional skip above reached
/// it: a grep for its id across the test tree found only the fixture file.
///
/// It was one of the three snippets failing in every scheduled run, and it was
/// the one no consumer could ever have reported, which is a step worse than a
/// skip that says nothing — a fixture with no consumer is build cost that
/// cannot answer a question.
#[test]
fn spin_until_future_complete_compiles() -> TestResult<()> {
    assert_snippet_compiled("spin_until_future_complete")
}
