//! Issue 1285: the board key → board family mapping has ONE table
//! (`nros_entry_lower::BOARD_KEYS`), and every consumer answers from it.
//!
//! The mapping used to be derived four times, and the four disagreed:
//! - `board_family` fell back to `Native`.
//! - `board_to_rtos` was a substring match that fell back to `posix`.
//! - The proc-macro's `known_boards_csv` was hand-written.
//! - The Rust pack's `board_path_for` had its own key set.
//!
//! These tests pin the agreement, so a new key cannot land in one place only.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use nros_cli_core::codegen::entry::{board_to_rtos, pack::entry_pack_for};
use nros_entry_lower::{BOARD_KEYS, BoardFamily, LoweredEntry, board_family, known_board_keys};
use nros_lang::Language;

/// Every consumer, asked about every key in the table, gives the table's answer.
#[test]
fn every_table_key_maps_to_its_family_through_every_consumer() {
    for &(key, family) in BOARD_KEYS {
        assert_eq!(board_family(key), Ok(family), "board_family(`{key}`)");
        assert_eq!(
            board_to_rtos(key),
            Ok(family.tier_rtos_key()),
            "board_to_rtos(`{key}`)"
        );
        let entry = LoweredEntry {
            board: key.into(),
            ..Default::default()
        };
        assert_eq!(entry.family(), Ok(family), "LoweredEntry::family(`{key}`)");

        let routed = entry_pack_for(Language::C, key).expect("known key routes");
        let want = if family.has_c_run_components() {
            "c"
        } else {
            "cpp"
        };
        assert_eq!(routed.pack, want, "entry_pack_for(C, `{key}`)");
    }
}

/// An unknown key is an error at every consumer: never `Native`, never
/// `posix`. The error names the known keys.
#[test]
fn an_unknown_key_errors_at_every_consumer() {
    for key in ["zigos", "esp32-qemu", "mps2-an385", "native_sim/native/64"] {
        let err = board_family(key).expect_err(key).to_string();
        for known in known_board_keys() {
            assert!(
                err.contains(known),
                "`{key}`: message omits `{known}`: {err}"
            );
        }
        assert!(board_to_rtos(key).is_err(), "board_to_rtos(`{key}`)");
        let pack_err = entry_pack_for(Language::C, key).expect_err(key);
        assert!(pack_err.contains(key), "{pack_err}");
    }
}

/// The four keys the old SUBSTRING match sent to `posix`, because none of them
/// contains its RTOS's name.
#[test]
fn substring_victims_get_their_real_rtos() {
    for (key, rtos) in [
        ("s32z270", "freertos"),
        ("an536", "freertos"),
        ("armfvp", "zephyr"),
        ("fvp-aemv8r-smp", "zephyr"),
    ] {
        assert_eq!(board_to_rtos(key), Ok(rtos), "`{key}`");
    }
}

/// The board family each Rust board ZST belongs to. `None` means the board has
/// no RTOS, so no C or C++ entry family: ESP32, RTIC and bare-metal MPS2.
///
/// This oracle is the test's own statement, which is the point. A new ZST in
/// `BOARD_PATHS` fails here until someone says which family it is.
fn rust_zst_family() -> BTreeMap<&'static str, Option<BoardFamily>> {
    BTreeMap::from([
        ("::nros_board_linux::LinuxBoard", Some(BoardFamily::Native)),
        (
            "::nros_board_mps2_an385_freertos::Mps2An385",
            Some(BoardFamily::Freertos),
        ),
        (
            "::nros_board_threadx_linux::ThreadxLinux",
            Some(BoardFamily::Threadx),
        ),
        (
            "::nros_board_threadx_qemu_riscv64::ThreadxQemuRiscv64",
            Some(BoardFamily::Threadx),
        ),
        (
            "::nros_board_nuttx_qemu::NuttxQemu",
            Some(BoardFamily::Nuttx),
        ),
        (
            "::nros_board_zephyr::ZephyrBoard",
            Some(BoardFamily::Zephyr),
        ),
        ("::nros_board_esp32_qemu::Esp32QemuEntry", None),
        ("::nros_board_mps2_an385::RticMps2An385", None),
        ("::nros_board_mps2_an385::Mps2An385", None),
    ])
}

/// The Rust pack's key set and the family table agree. A key the Rust pack
/// resolves to an RTOS board must be in the family table with THAT family. A
/// key it resolves to a no-RTOS board must NOT be there, or a C entry would get
/// a runner for a board that has none.
#[test]
fn the_rust_pack_key_set_agrees_with_the_family_table() {
    let zst_family = rust_zst_family();
    let mut used = BTreeSet::new();
    for &(key, path) in nros_orchestration_ir::BOARD_PATHS {
        let want = *zst_family.get(path).unwrap_or_else(|| {
            panic!("`{key}` names ZST `{path}`, which this test has no family for")
        });
        used.insert(path);
        match want {
            Some(family) => assert_eq!(
                board_family(key),
                Ok(family),
                "Rust key `{key}` names a {} board (`{path}`)",
                family.as_str()
            ),
            None => assert!(
                board_family(key).is_err(),
                "Rust key `{key}` names a no-RTOS board (`{path}`) but the family table \
                 claims it"
            ),
        }
    }
    let stale: Vec<_> = zst_family.keys().filter(|p| !used.contains(*p)).collect();
    assert!(
        stale.is_empty(),
        "oracle names ZSTs no key reaches: {stale:?}"
    );
}

/// Keys the family table has and the Rust pack does not. Each is a C/C++-only
/// spelling. Pinned so that adding a key to one table and not the other is a
/// deliberate edit here, not drift.
#[test]
fn family_keys_without_a_rust_board_are_exactly_these() {
    let rust: BTreeSet<&str> = nros_orchestration_ir::board_path_keys().collect();
    let c_cpp_only: BTreeSet<&str> = known_board_keys().filter(|k| !rust.contains(k)).collect();
    assert_eq!(
        c_cpp_only,
        BTreeSet::from([
            "an536",
            "armfvp",
            "freertos-posix",
            "fvp-aemv8r-smp",
            "mps3-an536-freertos",
            "s32z270",
            "s32z270-freertos",
            "threadx",
        ]),
    );
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// Every runner symbol `c_abi_runners` names is DEFINED in the tree. The table
/// it replaced named `nros_board_threadx_run_tiers`, a symbol that exists
/// nowhere.
///
/// A C definition is a line starting `int32_t <sym>(` in a board crate's `c/`.
/// A Rust one is `extern "C" fn <sym>(` in `nros-cpp`, where the native runners
/// live.
#[test]
fn c_abi_runners_name_only_symbols_that_exist() {
    let root = repo_root();
    let mut sources: Vec<(PathBuf, String)> = Vec::new();
    let boards = root.join("packages/boards");
    for board in std::fs::read_dir(&boards).expect("read packages/boards") {
        let c_dir = board.expect("dir entry").path().join("c");
        let Ok(files) = std::fs::read_dir(&c_dir) else {
            continue;
        };
        for f in files {
            let p = f.expect("dir entry").path();
            if p.extension().is_some_and(|e| e == "c") {
                let text = std::fs::read_to_string(&p).expect("read C source");
                sources.push((p, text));
            }
        }
    }
    let nros_cpp = root.join("packages/api/nros-cpp/src/lib.rs");
    let nros_cpp_text = std::fs::read_to_string(&nros_cpp).expect("read nros-cpp lib.rs");
    assert!(
        sources.len() >= 4,
        "found only {} board C sources under {} — the scan is looking in the wrong place",
        sources.len(),
        boards.display()
    );

    let defined = |sym: &str| -> bool {
        let c_def = format!("int32_t {sym}(");
        let rust_def = format!("extern \"C\" fn {sym}(");
        sources
            .iter()
            .any(|(_, t)| t.lines().any(|l| l.starts_with(&c_def)))
            || nros_cpp_text.contains(&rust_def)
    };

    let mut named = 0;
    for family in BoardFamily::ALL {
        let Some(runners) = family.c_abi_runners() else {
            continue;
        };
        for sym in [runners.run_components, runners.run_tiers]
            .into_iter()
            .flatten()
        {
            named += 1;
            assert!(
                defined(sym),
                "{}: c_abi_runners names `{sym}`, which no board C source or nros-cpp defines",
                family.as_str()
            );
        }
    }
    assert!(named >= 5, "only {named} runner names checked");

    // The negative control: the name the old table invented is absent, so
    // the scan can fail. A scan that finds everything proves nothing.
    assert!(
        !defined("nros_board_threadx_run_tiers"),
        "the scan found a definition of the symbol issue 1285 says does not exist \
         — either ThreadX gained a C runner (update c_abi_runners and this test) or \
         the scan is too loose"
    );
}
