//! Issue 1581 — the locator a workspace fixture row's image BAKES, read from the
//! same inputs its build reads.
//!
//! A test that runs a baked image starts its router on `alloc::port_of(...)`;
//! the image dials whatever its build compiled in. Those are two sources, and
//! when they part the image fails `Executor::open: ConnectionFailed` before any
//! code under test runs — which reads as a network or timing fault, not as the
//! configuration fault it is. Issue 1581 was four images that lost their
//! locator in a migration and fell back to the board default for a month.
//!
//! [`assert_row_dials_port`] closes the pair: a resolver that knows its cell
//! checks the row's baked locator against the allocator's port and panics with
//! both named. The carriers, in the order the build applies them:
//!
//! 1. the bringup's `system.toml` — `[image.<id>] locator` over
//!    `[image_defaults]` over `[system]` (`nros_orchestration_ir::leaf_system`);
//! 2. the row's `env.NROS_LOCATOR` (boards that `option_env!` it);
//! 3. the row's `cmake_defs.NROS_ENTRY_LOCATOR` (cmake lanes);
//! 4. the row's `west_zenoh_locator` (Zephyr west lanes).
//!
//! Which carrier a BOARD actually reads is `check-image-locator-bake`'s
//! question; this module answers "which port did the row ask for". The two
//! are sound only together: carrier 2 is honoured here as the build would, and
//! the gate is what refuses it on a board that never reads it — exactly the
//! threadx-linux row 1581 found, whose `NROS_LOCATOR` named the right port and
//! reached nothing.

use std::path::Path;

/// Parse a whole TOML DOCUMENT. `str::parse::<toml::Value>` reads a single
/// value, not a document, and rejects `[[table]]` at column 1.
fn read_doc(path: &Path) -> Result<toml::Value, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    toml::from_str::<toml::Table>(&text)
        .map(toml::Value::Table)
        .map_err(|e| format!("parse {}: {e}", path.display()))
}

fn str_at<'a>(t: &'a toml::Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().try_fold(t, |v, k| v.get(k))?.as_str()
}

/// The `examples/fixtures.toml` row with this `id`, from any row table.
fn manifest_row(manifest: &toml::Value, fixture_id: &str) -> Option<toml::Value> {
    manifest.as_table()?.values().find_map(|rows| {
        rows.as_array()?
            .iter()
            .find(|r| r.get("id").and_then(|v| v.as_str()) == Some(fixture_id))
            .cloned()
    })
}

/// The locator `fixture_id`'s image bakes, resolved against the checkout at
/// `root`. `Err` names what was missing.
pub fn baked_locator_in(root: &Path, fixture_id: &str) -> Result<String, String> {
    let manifest_path = root.join("examples/fixtures.toml");
    let manifest = read_doc(&manifest_path)?;
    let row = manifest_row(&manifest, fixture_id)
        .ok_or_else(|| format!("no row `{fixture_id}` in {}", manifest_path.display()))?;

    if let (Some(dir), Some(image)) = (str_at(&row, &["dir"]), str_at(&row, &["image"])) {
        let bringup = str_at(&row, &["bringup"]).unwrap_or("src/demo_bringup");
        let path = root.join(dir).join(bringup).join("system.toml");
        let doc = read_doc(&path)?;
        let id = image.rsplit(':').next().unwrap_or(image);
        let stated = str_at(&doc, &["image", id, "locator"])
            .or_else(|| str_at(&doc, &["image_defaults", "locator"]))
            .or_else(|| str_at(&doc, &["system", "locator"]));
        if let Some(l) = stated {
            return Ok(l.to_owned());
        }
    }
    str_at(&row, &["env", "NROS_LOCATOR"])
        .or_else(|| str_at(&row, &["cmake_defs", "NROS_ENTRY_LOCATOR"]))
        .or_else(|| str_at(&row, &["west_zenoh_locator"]))
        .map(str::to_owned)
        .ok_or_else(|| {
            format!(
                "fixture row `{fixture_id}` bakes NO locator — neither its image's system.toml \
                 nor the row states one, so the image dials its board's compiled-in default \
                 (issue 1581)"
            )
        })
}

/// The TCP port of a `tcp/<host>:<port>` locator.
pub fn locator_port(locator: &str) -> Option<u16> {
    locator.rsplit(':').next()?.parse().ok()
}

/// Panic unless `fixture_id`'s baked locator dials `port` — the router the test
/// is about to start. Called by resolvers that know their matrix cell, so a
/// mismatch is a named configuration failure instead of a `ConnectionFailed`.
pub fn assert_row_dials_port(fixture_id: &str, port: u16) {
    let locator = baked_locator_in(&crate::project_root(), fixture_id)
        .unwrap_or_else(|e| panic!("issue 1581: {e}"));
    assert_eq!(
        locator_port(&locator),
        Some(port),
        "issue 1581: fixture row `{fixture_id}` bakes `{locator}`, but its cell's router is \
         `alloc::port_of(...)` = {port}. The image would dial a port no test listens on and \
         fail `Executor::open: ConnectionFailed` before any code under test runs. Fix the \
         image's `locator` in its bringup system.toml (or the row's carrier)."
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        alloc::port_of,
        matrix::{Lang, PlatformId, Workload},
    };

    /// Every embedded RealtimeTiers row bakes its cell's allocator port. Static:
    /// reads the manifest and the bringups, needs no fixture. The rows are named
    /// here because a manifest row carries no workload.
    #[test]
    fn realtime_tier_rows_dial_their_allocator_port() {
        use Lang::*;
        use PlatformId::*;
        let rows: &[(&str, PlatformId, Lang)] = &[
            ("workspace-rust-freertos-realtime", FreertosMps2, Rust),
            ("workspace-c-freertos-realtime", FreertosMps2, C),
            ("workspace-cpp-freertos-realtime", FreertosMps2, Cpp),
            ("workspace-rust-threadx-linux-realtime", ThreadxLinux, Rust),
            ("workspace-rust-nuttx-realtime", NuttxArm, Rust),
            ("workspace-c-nuttx-realtime", NuttxArm, C),
            ("workspace-cpp-nuttx-realtime", NuttxArm, Cpp),
            ("workspace-rust-nuttx-riscv-realtime", NuttxRiscv, Rust),
            ("workspace-c-nuttx-riscv-realtime", NuttxRiscv, C),
            ("workspace-cpp-nuttx-riscv-realtime", NuttxRiscv, Cpp),
        ];
        let root = crate::project_root();
        let bad: Vec<String> = rows
            .iter()
            .filter_map(|&(id, p, l)| {
                let want = port_of(p, l, Workload::RealtimeTiers);
                match baked_locator_in(&root, id) {
                    Ok(loc) if locator_port(&loc) == Some(want) => None,
                    Ok(loc) => Some(format!("{id}: bakes `{loc}`, cell wants port {want}")),
                    Err(e) => Some(format!("{id}: {e}")),
                }
            })
            .collect();
        assert!(bad.is_empty(), "issue 1581:\n  {}", bad.join("\n  "));
    }

    /// Negative control: a row with no carrier is an error, not a default.
    #[test]
    fn a_row_with_no_carrier_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let bringup = dir.join("ws/src/b");
        std::fs::create_dir_all(&bringup).unwrap();
        std::fs::create_dir_all(dir.join("examples")).unwrap();
        std::fs::write(
            dir.join("examples/fixtures.toml"),
            "[[workspace_fixture]]\nid = \"r\"\ndir = \"ws\"\nbringup = \"src/b\"\nimage = \"x\"\n\
             [[workspace_fixture]]\nid = \"e\"\ndir = \"ws\"\nbringup = \"src/b\"\nimage = \"x\"\n\
             env = { NROS_LOCATOR = \"tcp/h:12\" }\n",
        )
        .unwrap();
        std::fs::write(bringup.join("system.toml"), "[image.x]\nboard = \"b\"\n").unwrap();
        let none = baked_locator_in(dir, "r");
        let env = baked_locator_in(dir, "e");
        std::fs::write(
            bringup.join("system.toml"),
            "[image.x]\nboard = \"b\"\nlocator = \"tcp/h:34\"\n",
        )
        .unwrap();
        let stated = baked_locator_in(dir, "e");
        assert!(none.is_err(), "a row with no carrier resolved to {none:?}");
        assert_eq!(env.as_deref(), Ok("tcp/h:12"));
        assert_eq!(
            stated.as_deref(),
            Ok("tcp/h:34"),
            "the image's own locator comes first"
        );
    }
}
