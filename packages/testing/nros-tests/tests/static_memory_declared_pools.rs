//! The static-pool inventory's byte figures must equal the bytes the linker
//! actually reserved — phase 392 W1, issues 0815 and 0739.
//!
//! `book/src/reference/static-pool-inventory.md` tells a consumer how much RAM a
//! pool costs so they can rightsize a board. That figure is computed from a
//! `// nros-pool:` comment evaluated at the knobs' defaults, which is a claim
//! about a type the comment cannot see. Issue 0739 declined to annotate
//! `MESSAGE_INFO_TABLE` for exactly this reason: its element gains three fields
//! under `alloc` + `safety-e2e`, so any constant would be right for one build
//! and wrong for the rest.
//!
//! This test closes the loop the other way round. Instead of asking the comment
//! to be trustworthy, it asks the IMAGE: `nm` the built fixture, join each
//! annotated pool to its symbol, and require agreement. A field appended to a
//! pooled struct, or an element size changed, now fails here rather than
//! quietly making a published number wrong.
//!
//! The fixture is a zenoh one because zenoh is where the annotated pools live —
//! an xrce image links none of them, and the checker rejects that as vacuous
//! rather than reporting a green over an empty set.

use nros_tests::{
    TestResult,
    fixtures::{Rmw as FixtureRmw, build_native_rust_example_rmw},
    project_root,
};
use std::process::Command;

#[test]
fn declared_pool_sizes_equal_the_linked_image() -> TestResult<()> {
    let binary = build_native_rust_example_rmw("talker", "talker", FixtureRmw::Zenoh)?;
    let root = project_root();
    let script = root.join("scripts/nros-mem-report.py");
    assert!(
        script.is_file(),
        "missing {} — the pool checker moved or was deleted",
        script.display()
    );

    let out = Command::new("python3")
        .arg(&script)
        .arg(&binary)
        .arg("--check")
        .current_dir(&root)
        .output()?;

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "declared pool arithmetic disagrees with {}\n\
         Regenerate the page after fixing the formula: \
         python3 scripts/gen-pool-inventory.py\n\n{stdout}{stderr}",
        binary.display()
    );
    Ok(())
}
