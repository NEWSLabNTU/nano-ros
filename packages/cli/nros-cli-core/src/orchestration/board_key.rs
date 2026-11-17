//! Which family a board key names, for every board the CLI can resolve
//! (RFC-0135 D2, phase-577 W8).
//!
//! `nros_entry_lower::BOARD_KEYS` is the one table of nano-ros's OWN board keys
//! (issue 1285), and `tests/board_key_table.rs` holds it to the in-tree board
//! definitions both ways. A board package outside nano-ros (one reached through
//! `$NROS_EXTRA_BOARD_PATH`) can never be a row in a table compiled into
//! nano-ros, so every CLI site that asked that table alone refused an outside
//! board's image: `codegen entry-pack` first, with "unknown board key".
//!
//! [`board_family`] asks the table, then the board catalog: an outside board's
//! family is its board definition's `platform`'s, the same rule
//! [`PlatformKind::board_family`](super::board_descriptor::PlatformKind::board_family)
//! states for the catalog namespace. A key neither knows is still the table's
//! error, which names every key nano-ros ships.
//!
//! Every `nros-cli-core` site that maps a board key to a family goes through
//! here; `check-board-key-family-sites` refuses a direct call to the table.

use nros_entry_lower::{BoardFamily, UnknownBoard};

/// The family `key` names: nano-ros's table, else the board catalog.
pub fn board_family(key: &str) -> Result<BoardFamily, UnknownBoard> {
    match nros_entry_lower::board_family(key) {
        Ok(f) => Ok(f),
        Err(e) => catalog_family(key).ok_or(e),
    }
}

/// [`BoardFamily::sched_task_rtos_key`] for `key`, or the no-RTOS key.
pub fn sched_task_rtos_key(key: &str) -> &'static str {
    board_family(key).map_or(
        nros_entry_lower::NO_RTOS_SCHED_TASK_KEY,
        BoardFamily::sched_task_rtos_key,
    )
}

/// [`BoardFamily::entry_shape`] for `key`; `None` for a key no catalog knows.
pub fn entry_shape(key: &str) -> Option<&'static str> {
    board_family(key).ok().and_then(BoardFamily::entry_shape)
}

/// The family of a board the catalog resolves: the in-tree definitions plus
/// every `$NROS_EXTRA_BOARD_PATH` root, found from the working directory.
fn catalog_family(key: &str) -> Option<BoardFamily> {
    let cwd = std::env::current_dir().ok()?;
    let root = super::nano_ros_root::resolve(None, &cwd)?;
    catalog_family_in(&root, &super::board_descriptor::extra_board_roots(), key)
}

/// [`catalog_family`] over an explicit root and extra roots (tests).
pub(crate) fn catalog_family_in(
    root: &std::path::Path,
    extra_roots: &[std::path::PathBuf],
    key: &str,
) -> Option<BoardFamily> {
    let catalog = super::board_descriptor::BoardCatalog::load_with_extra(root, extra_roots).ok()?;
    let mut families = catalog
        .descriptors()
        .iter()
        // A board KEY is a definition's `names` entry and nothing else: not
        // the framework's own id (`native_sim/native/64`) or a directory
        // alias, which `answers_to` also accepts but an image never authors
        // (issue 1519), and which the table's error must keep refusing.
        .filter(|d| d.names.iter().any(|n| n == key))
        .filter_map(|d| d.platform.board_family());
    let first = families.next()?;
    // Two definitions answering one key with different families is no answer.
    families.all(|f| f == first).then_some(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board_package(root: &std::path::Path, key: &str, platform: &str) {
        let dir = root.join(format!("nros-board-{key}"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("nros-board.toml"),
            format!(
                "[[board]]\nnames = [\"{key}\"]\nplatform = \"{platform}\"\n\
                 toolchain = \"stable\"\nentry_kind = \"board-run\"\nsupported_netstacks = []\n"
            ),
        )
        .unwrap();
        std::fs::write(
            dir.join("package.xml"),
            format!(
                "<package format=\"3\"><name>nros_board_x</name><export>\
                 <nano_ros_provides kind=\"board\" name=\"{key}\"/></export></package>"
            ),
        )
        .unwrap();
    }

    fn nros_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
    }

    /// An outside board's family is its definition's platform's; a key no
    /// catalog knows stays unknown.
    #[test]
    fn an_outside_board_package_has_its_platforms_family() {
        let t = tempfile::tempdir().unwrap();
        board_package(t.path(), "outside-r5f-freertos", "freertos");
        let extra = [t.path().to_path_buf()];
        assert_eq!(
            catalog_family_in(&nros_root(), &extra, "outside-r5f-freertos"),
            Some(BoardFamily::Freertos)
        );
        assert_eq!(
            catalog_family_in(&nros_root(), &extra, "no-such-board"),
            None
        );
        assert_eq!(
            catalog_family_in(&nros_root(), &[], "outside-r5f-freertos"),
            None,
            "without its root the outside board is unknown"
        );
    }

    /// The table still answers first, so an in-tree key never reaches the
    /// catalog, and the table's error survives a key nobody knows.
    #[test]
    fn the_table_answers_first_and_its_error_survives() {
        assert_eq!(
            board_family("mps2-an385-freertos"),
            Ok(BoardFamily::Freertos)
        );
        let err = board_family("no-such-board-anywhere").unwrap_err();
        assert!(err.to_string().contains("mps2-an385-freertos"), "{err}");
    }
}
