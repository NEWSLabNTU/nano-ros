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
///
/// `native_sim/native/64` was in this list until phase-445 W5, left it while
/// five bringups authored it as an image board (it was the zephyr
/// descriptor's second NAME then), and is back since issue 1519: it is
/// Zephyr's id for the board, `[board.zephyr] west_board`, which no image may
/// author — `nros build` refuses it naming `zephyr` — so no consumer may
/// accept it as a key either. `rtic-mps2-an385` is here as a real Rust key
/// whose board has no RTOS, so asking its family is an error by design rather
/// than by omission.
#[test]
fn an_unknown_key_errors_at_every_consumer() {
    for key in [
        "zigos",
        "esp32-qemu",
        "mps2-an385",
        "rtic-mps2-an385",
        "native_sim/native/64",
    ] {
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

/// Issue 1285 follow-up. There are TWO board namespaces with two authorities:
/// entry board keys (`BOARD_KEYS`) and image board ids (the board catalog,
/// `packages/boards/**/nros-board.toml`). `nros::main!` and `plan_from_model`
/// read the first, and `codegen-system` reads the second. Many spellings are
/// in both. Where one is, both authorities must name the same RTOS, or an
/// image's tiers would depend on which verb baked it.
///
/// A Rust-pack key the family table leaves out, because the board has no RTOS,
/// must resolve in the catalog to a platform with no RTOS family too.
#[test]
fn a_spelling_in_both_namespaces_names_the_same_rtos() {
    use nros_cli_core::orchestration::board_descriptor::{BoardCatalog, DeployResolution};

    let catalog = BoardCatalog::load_with_extra(&repo_root(), &[]).expect("in-tree board catalog");
    let resolve = |key: &str| match catalog.resolve_deploy(key) {
        DeployResolution::Board(d) => Some(d.platform),
        _ => None,
    };

    let mut shared = 0;
    for &(key, family) in BOARD_KEYS {
        let Some(platform) = resolve(key) else {
            continue;
        };
        shared += 1;
        assert_eq!(
            platform.board_family(),
            Some(family),
            "`{key}`: BOARD_KEYS says {}, the catalog says platform `{}`",
            family.as_str(),
            platform.kebab()
        );
        assert_eq!(platform.tier_rtos_key(), family.tier_rtos_key(), "`{key}`");
    }
    // Measured 2026-09-11: 18 of the 21 keys resolve in the catalog. The
    // exceptions are `armfvp` and `freertos-qemu-mps2-an385`, which it does not
    // know, and `threadx`, which two descriptors claim. A floor, so the
    // comparison cannot quietly become vacuous.
    assert!(shared >= 15, "only {shared} keys resolve in the catalog");

    let mut no_rtos = 0;
    for key in nros_orchestration_ir::board_path_keys().filter(|k| board_family(k).is_err()) {
        let Some(platform) = resolve(key) else {
            continue;
        };
        no_rtos += 1;
        assert_eq!(
            platform.board_family(),
            None,
            "`{key}` has no row in BOARD_KEYS, but the catalog gives it an RTOS platform `{}`",
            platform.kebab()
        );
        assert_eq!(
            platform.tier_rtos_key(),
            nros_entry_lower::tier_rtos_key_for(key),
            "`{key}`"
        );
    }
    assert!(no_rtos >= 4, "only {no_rtos} no-RTOS Rust keys resolve");
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
    for &(key, path, _links_std) in nros_orchestration_ir::BOARD_PATHS {
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

/// Issue 1381 — `BOARD_PATHS`'s `links_std` column agrees with the board
/// descriptors' `entry_kind`, for every key a descriptor knows.
///
/// The column exists because the two Rust EMITTERS need the answer where no
/// descriptor is reachable: `nros::main!` runs as a proc macro with a board KEY
/// and no catalog. So it is a restatement, and a restatement that nothing
/// checks is a mirror waiting to drift — the class this file was opened for.
///
/// `entry_kind` is the authority because it is also what WRITES the attribute:
/// `builder::entry` puts `#![no_std]` at the top of a `board-run` and a
/// `zephyr-staticlib` entry TU and nothing at the top of a `hosted-main` one.
///
/// Keys a descriptor does not name (`mps2-an385`, `freertos`, `nuttx-riscv`, …
/// are Rust-pack spellings) are skipped rather than failed: this asserts
/// agreement where both sides speak, not that the two key sets are equal —
/// `an_unknown_key_errors_at_every_consumer` above records that they are not.
#[test]
fn the_links_std_column_agrees_with_the_descriptors_entry_kind() {
    use nros_cli_core::orchestration::board_descriptor::{BoardCatalog, EntryKind};

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        // packages/cli/nros-cli-core -> cli -> packages -> repo root
        .ancestors()
        .nth(3)
        .expect("repo root")
        .to_path_buf();
    // No `Err(_) => return`: this asserts about the SHIPPED descriptors, so a
    // catalog that will not load is a failure, not a green having checked
    // nothing (issue 0571's shape).
    let catalog = BoardCatalog::load(&root)
        .unwrap_or_else(|e| panic!("shipped board catalog under {}: {e}", root.display()));

    let mut checked = 0;
    for &(key, _, links_std) in nros_orchestration_ir::BOARD_PATHS {
        let Some(d) = catalog
            .descriptors()
            .iter()
            .find(|d| d.names.iter().any(|n| n == key))
        else {
            continue;
        };
        let want = match d.entry_kind {
            EntryKind::HostedMain => true,
            EntryKind::BoardRun | EntryKind::ZephyrStaticlib => false,
        };
        assert_eq!(
            links_std, want,
            "`{key}`: BOARD_PATHS says links_std = {links_std}, but its descriptor \
             declares entry_kind = {:?}. An emitter reads the column and would write a \
             `std::` path into a `#![no_std]` entry (issue 1381) — or drop the hosted \
             `fn main()` from one that needs it.",
            d.entry_kind
        );
        checked += 1;
    }
    // A scan that matched nothing would pass silently. Eleven of the 22 keys
    // are named by a descriptor as of 2026-09-21.
    assert!(
        checked >= 8,
        "only {checked} BOARD_PATHS keys were matched to a descriptor — the name \
         matching has probably broken, so this test is checking nothing"
    );
}

/// The FRAMEWORK a key resolves to agrees with the descriptors' `entry_kind`,
/// for every key a descriptor knows.
///
/// This is the sibling of [`the_links_std_column_agrees_with_the_descriptors_entry_kind`]
/// over the same catalog and the same key set, and it exists because the gate
/// issue 1435 shipped has a reach narrower than the rule it enforces — issue
/// 0196's shape.
///
/// That gate is `nros_orchestration_ir`'s
/// `every_key_of_one_board_zst_wants_one_framework`, and it compares keys that
/// SHARE a board ZST. It therefore checks AGREEMENT, not correctness, and is
/// blind to the two ways a framework row can be wrong without two keys
/// disagreeing:
///
/// - a ZST named by exactly ONE key (`threadx-linux` today) — nothing to
///   compare it against, so any value passes;
/// - BOTH keys of a shared ZST wrong the SAME way. Had phase-445 W5 RENAMED
///   the zephyr key instead of adding a second spelling, `framework_for_board_key`
///   would have answered `None` for the only key there was, every consumer
///   would have read `owned-spin`, and the agreement gate would have been
///   green on a table that emits `<ZephyrBoard as BoardEntry>::run` — the
///   `E0277` of issue 1435, unremarked.
///
/// `entry_kind` is an INDEPENDENT authority on the entry shape, written per
/// board in `nros-board.toml` rather than derived from the key, so it can
/// answer both cases. What it can answer is the `zephyr-staticlib` shape
/// exactly: Zephyr owns `main`, so the macro's `Framework::Zephyr` arm emits a
/// `rust_main` staticlib export and no `BoardEntry::run` call. The assertion is
/// therefore a BICONDITIONAL on `zephyr` alone, and deliberately says nothing
/// about the rest: `board-run` covers `owned-spin`, `rtic`, `embassy` and
/// `esp32` at once, so it does not name a framework and a test pretending it
/// did would be asserting something the data does not hold.
#[test]
fn the_framework_agrees_with_the_descriptors_entry_kind() {
    use nros_cli_core::orchestration::board_descriptor::{BoardCatalog, EntryKind};

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root")
        .to_path_buf();
    // Same reason as the sibling test: a catalog that will not load is a
    // failure, not a green having checked nothing (issue 0571's shape).
    let catalog = BoardCatalog::load(&root)
        .unwrap_or_else(|e| panic!("shipped board catalog under {}: {e}", root.display()));

    let mut checked = 0;
    let mut zephyr_keys = 0;
    for &(key, zst, _links_std) in nros_orchestration_ir::BOARD_PATHS {
        let Some(d) = catalog
            .descriptors()
            .iter()
            .find(|d| d.names.iter().any(|n| n == key))
        else {
            continue;
        };
        // `None` is what every consumer reads as `owned-spin`, so resolve it
        // the way they do rather than skipping it — a MISSING row is exactly
        // the defect this test is here for.
        let framework = nros_orchestration_ir::framework_for_board_key(key).unwrap_or("owned-spin");
        let wants_zephyr = matches!(d.entry_kind, EntryKind::ZephyrStaticlib);
        assert_eq!(
            framework == "zephyr",
            wants_zephyr,
            "`{key}` ({zst}): `framework_for_board_key` says `{framework}`, but its \
             descriptor declares entry_kind = {:?}. A `zephyr-staticlib` board wants the \
             `zephyr` entry shape (a `rust_main` staticlib export) and nothing else wants \
             it; any other answer makes both Rust emitters render \
             `<Board as BoardEntry>::run` for a ZST that does not implement it, which is \
             the `E0277` of issue 1435. Add `{key}` to `framework_for_board_key`.",
            d.entry_kind
        );
        checked += 1;
        if wants_zephyr {
            zephyr_keys += 1;
        }
    }
    // A scan that matched nothing would pass silently, and a scan that matched
    // no ZEPHYR key would pass while checking only the uninteresting half.
    assert!(
        checked >= 8,
        "only {checked} BOARD_PATHS keys were matched to a descriptor — the name \
         matching has probably broken, so this test is checking nothing"
    );
    // ONE since issue 1519: the floor was 2 while the zephyr board had two
    // keys (`zephyr` and its Zephyr id `native_sim/native/64`), and that second
    // key was retired with the framework spelling. The point of the floor —
    // the zephyr shape is represented at all — needs only one.
    assert!(
        zephyr_keys >= 1,
        "only {zephyr_keys} matched key(s) belong to a `zephyr-staticlib` board — the \
         one shape this test can actually name is unrepresented, so it is checking \
         nothing that matters"
    );
}

/// Issue 1517 — the string `west build -b` receives comes from the descriptor,
/// not from whatever the image happened to author.
///
/// `[image.*] board` is a nano-ros board id (`ImageBlock::board`: "NEVER a
/// framework's own board string"), and `nros build` used to hand it to west
/// verbatim unless the board declared the OUTER `west_board` — which nothing
/// in-tree does. Every board that states `[board.zephyr] west_board` was
/// therefore mis-projected: `[image.fvp] board = "fvp-aemv8r-smp"` reached
/// `west build -b fvp-aemv8r-smp`, a board west has never heard of. The row
/// only looked fine because it had misread as `native_sim/native/64`, where
/// the authored string IS the Zephyr id — which is the whole reason the
/// fall-through went unnoticed.
///
/// Asserted over every zephyr descriptor in the real catalog, in both
/// directions: a stated `west_board` is USED, and a board that states none
/// falls back to the authored id.
#[test]
fn a_zephyr_boards_west_b_comes_from_its_descriptor() {
    use nros_cli_core::orchestration::board_descriptor::BoardCatalog;

    let catalog = BoardCatalog::load_with_extra(&repo_root(), &[]).expect("in-tree board catalog");

    let mut stated = 0;
    let mut fell_back = 0;
    for d in catalog.descriptors() {
        if d.platform.kebab() != "zephyr" {
            continue;
        }
        // Every name a bringup could author for this board must project to the
        // same `-b`: the projection is a property of the BOARD, not of which
        // of its names someone wrote down.
        for authored in &d.names {
            let got = d.west_build_board(authored);
            match d.zephyr.as_ref().map(|z| z.west_board.as_str()) {
                Some(want) => {
                    assert_eq!(
                        got, want,
                        "`{authored}`: the descriptor states west_board = `{want}`, but the \
                         projection gives `{got}` — an image authoring the nano-ros board id \
                         would reach `west build -b {got}`"
                    );
                    stated += 1;
                }
                None => {
                    assert_eq!(
                        got, *authored,
                        "`{authored}`: no `[board.zephyr] west_board`, so the authored id is \
                         all there is to pass"
                    );
                    fell_back += 1;
                }
            }
        }
    }
    // A floor, so the stated arm cannot go vacuous. Measured 2026-09-28: the
    // `fvp-aemv8r-smp`, `mps2-an385-zephyr`, `qemu-cortex-a53` AND `zephyr`
    // descriptors state a `west_board`.
    assert!(stated >= 4, "only {stated} stated west_board(s) checked");

    // The fall-back arm has NO in-tree subject any more, and that is a result
    // rather than a gap — phase-470 W5.a / issue 1288. The `zephyr` descriptor
    // was the only one falling back, and it was the WRONG answer: 21 in-tree
    // images author `board = "zephyr"`, so all 21 emitted `west build -b
    // zephyr`, a board Zephyr does not have. Giving it a `west_board` fixed
    // that and emptied this arm.
    //
    // So the arm moves to a SYNTHETIC descriptor rather than being deleted or
    // floored at zero. The fall-back is still live code
    // (`west_build_board`'s `unwrap_or_else`) and still the right behaviour for
    // an out-of-tree board that states no `[board.zephyr]`; an arm asserted over
    // a catalog that no longer contains a subject is a test that silently checks
    // nothing, which is the shape the floors above exist to refuse.
    assert_eq!(
        fell_back, 0,
        "an in-tree zephyr descriptor states no `[board.zephyr] west_board`. \
         Either add it to that descriptor, or raise the `stated` floor and \
         delete this assertion — but do not leave the fall-back live in-tree: \
         issue 1288 measured what that costs"
    );
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path().join("nros-board-out-of-tree");
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(
        dir.join("nros-board.toml"),
        "[[board]]\n\
         names = [\"acme-zephyr-board\"]\n\
         platform = \"zephyr\"\n\
         toolchain = \"stable\"\n\
         entry_kind = \"zephyr-staticlib\"\n\
         supported_netstacks = []\n",
    )
    .expect("write descriptor");
    // A descriptor is only loaded when its package ANNOUNCES the board
    // (`check-provider-announcements.py`'s other half): the loader refuses an
    // unannounced one by design, so the synthetic package needs both files.
    std::fs::write(
        dir.join("package.xml"),
        "<?xml version=\"1.0\"?>\n\
         <package format=\"3\">\n\
         <name>nros_board_acme</name>\n\
         <version>0.0.0</version>\n\
         <description>synthetic out-of-tree zephyr board</description>\n\
         <maintainer email=\"dev@example.com\">Developer</maintainer>\n\
         <license>Apache-2.0</license>\n\
         <export>\n\
         <build_type>nros_cmake</build_type>\n\
         <nano_ros_provides kind=\"board\" name=\"acme-zephyr-board\"/>\n\
         </export>\n\
         </package>\n",
    )
    .expect("write package.xml");
    let extra = BoardCatalog::load_with_extra(&repo_root(), &[tmp.path().to_path_buf()])
        .expect("catalog with an out-of-tree board");
    let acme = extra
        .descriptors()
        .iter()
        .find(|d| d.names.iter().any(|n| n == "acme-zephyr-board"))
        .expect("the synthetic descriptor loaded");
    assert!(
        acme.zephyr.is_none(),
        "the synthetic board states no [board.zephyr]"
    );
    assert_eq!(
        acme.west_build_board("acme-zephyr-board"),
        "acme-zephyr-board",
        "with no stated west_board the authored id is all there is to pass"
    );
}
