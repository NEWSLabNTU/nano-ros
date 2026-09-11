//! Stage 2 of the ENTRY pipeline: the facts every language pack renders from.
//!
//! RFC-0091 §4 / phase-432 W2.2. The entry emitters each derived these
//! independently — and the `nros::main!()` proc-macro derived them a third
//! time, because it cannot depend on `nros-cli-core` (issue 0083: that pulled
//! the whole planner into every USER's entry build).
//!
//! ## The dependency budget is the design constraint
//!
//! This crate exists to be adoptable by that proc-macro. Its dependency list
//! is `serde` today and may grow only to what the macro already accepts
//! (`nros-pkg-index`, `nros-launch-parser`, `nros-orchestration-ir`). A heavy
//! dependency here lands in every downstream user's build, which is the force
//! that produced the duplication this crate removes. `eyre` in particular
//! stays out: a leaf carries a plain error type, as `nros-lang` does.
//!
//! ## What belongs here, and what does not
//!
//! COMPUTATION belongs here: which family a board key names, which boot shape
//! that family has, the encodings the ABI fixes. SPELLING does not — RFC-0091
//! §8b found the first draft leaking C++ into this stage as a `board_path`
//! like `::nros::board::LinuxBoard`, which a pure-C or Zig pack cannot use.
//! The neutral fact is the board's IDENTITY; how that becomes a call is the
//! pack's business.

#![forbid(unsafe_code)]
#![no_std]

extern crate alloc;

mod node;

pub use node::{LoweredEntry, LoweredNode, NodeIdentity, QosOverride, sanitize_pkg};

/// The board families the entry pipeline distinguishes.
///
/// Every key in [`BOARD_KEYS`] collapses onto one of five families. The KEY is
/// what a user writes and what cmake passes; the FAMILY is what the lowering
/// reasons about, and what a pack turns into a call.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum BoardFamily {
    /// The host build (`native`, `posix`).
    Native,
    Zephyr,
    Nuttx,
    Freertos,
    Threadx,
}

/// The boot wrapper a generated entry gets.
///
/// Derived from the family, once. It used to be spelled separately in the
/// per-tier and single-executor paths of `emit_cpp`, and the two spellings
/// tested DIFFERENT predicates — they agreed only because a condition seventy
/// lines away excluded the one board they disagree about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BootShape {
    /// The kernel calls `main(void)` directly (Zephyr).
    Kernel,
    /// The board's `startup.c` owns `main` and dispatches to `nros_app_main`.
    App,
    /// A host process keeping the POSIX `int main(argc, argv)`.
    Host,
}

impl BoardFamily {
    /// Every family, in a stable order — so a consumer that must handle all of
    /// them iterates rather than lists, and cannot go stale.
    pub const ALL: [BoardFamily; 5] = [
        BoardFamily::Native,
        BoardFamily::Zephyr,
        BoardFamily::Nuttx,
        BoardFamily::Freertos,
        BoardFamily::Threadx,
    ];

    /// Does this family boot through the board's own `startup.c`?
    ///
    /// Everything except the host: the board owns `main` and dispatches to the
    /// entry's `app_main`, so a generated `int main` would be a second `main`
    /// in an image whose board already defines one. Zephyr is embedded but the
    /// KERNEL calls `main(void)`, which is why boot shape is three-valued and
    /// not this predicate.
    pub fn is_embedded(self) -> bool {
        self != BoardFamily::Native
    }

    /// The C-ABI runners this family exports, or `None` when it has no C
    /// board surface at all.
    ///
    /// Issue 1285. This is the ONE record of which symbol a C entry calls.
    /// The runner names used to live in `emit_c`'s own table, while this
    /// predicate lived here, and nothing tied the two. The table's ThreadX arm
    /// named `nros_board_threadx_run_tiers`, a symbol that exists nowhere. Now
    /// the predicate is derived from the names, so the two cannot disagree.
    ///
    /// The names are carried, not assembled from a prefix. The C ABI does not
    /// name them uniformly: `native` keeps the `_named` suffix its
    /// two-overload history left behind, and the RTOS runners have no such
    /// pair. Where each is DEFINED:
    ///
    /// - `Native` — `nros_board_native_run_components_named` and
    ///   `nros_board_native_run_tiers`, both `extern "C"` in Rust
    ///   (`packages/api/nros-cpp/src/lib.rs`).
    /// - `Freertos`, `Zephyr`, `Nuttx` — ONE `run_components`,
    ///   `nros_board_rtos_run_components`
    ///   (`packages/boards/nros-board-common/c/nros_rtos_run_components.c`),
    ///   because the single-executor path differs only in a per-tick yield.
    ///   `run_tiers` is per-board: a FreeRTOS task, a Zephyr `k_thread` and a
    ///   NuttX pthread are three different things. The three are
    ///   `nros_board_{freertos,zephyr,nuttx}_run_tiers`, in each board crate's
    ///   `c/<rtos>_run_tiers.c`.
    /// - `Threadx` — the same shared `nros_board_rtos_run_components`, and NO
    ///   `run_tiers` (issue 1286). ThreadX's C++ `run_components` is the
    ///   FreeRTOS one line for line, and the shared runner already has no
    ///   per-tick yield off Zephyr, so there was nothing ThreadX-specific to
    ///   write. The CMake lane compiles the TU into the app target (both
    ///   `cmake/board/nano-ros-board-*threadx*.cmake`). The cargo lane does
    ///   not: `nros-board-threadx-linux`'s glue is `+whole-archive`
    ///   (issue 0582), so the runner there would pull `nros_cpp_*` into every
    ///   Rust ThreadX image.
    ///
    /// Each half is its own `Option` because the two land independently.
    /// ThreadX is the family that uses that: a `Some` with one `None` in it.
    /// A multi-tier ThreadX plan therefore takes the single-executor
    /// sched-context path in the C pack, exactly as it does in the C++ one
    /// (`Plan::executor_shape` reads the `run_tiers` half).
    ///
    /// `nros-cli-core/tests/board_key_table.rs` checks that every name here is
    /// defined in the tree.
    pub fn c_abi_runners(self) -> Option<CAbiRunners> {
        match self {
            BoardFamily::Native => Some(CAbiRunners {
                run_components: Some("nros_board_native_run_components_named"),
                run_tiers: Some("nros_board_native_run_tiers"),
            }),
            BoardFamily::Freertos => Some(CAbiRunners {
                run_components: Some("nros_board_rtos_run_components"),
                run_tiers: Some("nros_board_freertos_run_tiers"),
            }),
            BoardFamily::Zephyr => Some(CAbiRunners {
                run_components: Some("nros_board_rtos_run_components"),
                run_tiers: Some("nros_board_zephyr_run_tiers"),
            }),
            BoardFamily::Nuttx => Some(CAbiRunners {
                run_components: Some("nros_board_rtos_run_components"),
                run_tiers: Some("nros_board_nuttx_run_tiers"),
            }),
            BoardFamily::Threadx => Some(CAbiRunners {
                run_components: Some("nros_board_rtos_run_components"),
                run_tiers: None,
            }),
        }
    }

    /// Whether this family ships a C-ABI `run_components`, i.e. whether a
    /// `--lang c` entry can be rendered by the C pack instead of being routed
    /// to the C++ one.
    ///
    /// phase-432 W3.1. This is the predicate the routing rule needs, and it is
    /// NOT `is_embedded()`, which is what it used to ask. The two agreed only
    /// while `native` was the sole family with a C runner. They are different
    /// questions, and conflating them made a family-wide assumption out of a
    /// per-board fact.
    ///
    /// One home, because four sites consume it: the pack router
    /// (`entry_pack_for`), the emitter's own refusal, the CLI's emit dispatch,
    /// and the CMake extension/`LANGUAGES` decision. It is DERIVED from
    /// [`c_abi_runners`](Self::c_abi_runners) (issue 1285), so the routing
    /// and the symbol the C pack names come from one record.
    ///
    /// The benefit is RMW-CONDITIONAL and this predicate does not encode that.
    /// A C runner drops the C++ toolchain requirement for zenoh and XRCE, but
    /// not for cyclonedds or uORB, whose RMW libraries are themselves C++. The
    /// entry language and the RMW's own language are separate facts, so the
    /// routing answers only the first.
    pub fn has_c_run_components(self) -> bool {
        self.c_abi_runners()
            .is_some_and(|r| r.run_components.is_some())
    }

    /// The key a tier's per-RTOS sub-table is read under:
    /// `[tiers.<name>.<rtos>]`, rlm's `TierDef::platform`, and
    /// `nros-orchestration-ir`'s `rtos_spec`.
    ///
    /// This is `as_str()` for every family but the host, which is spelled
    /// `posix` there. That is the platform the host build runs on, not the
    /// board it names.
    pub fn tier_rtos_key(self) -> &'static str {
        match self {
            BoardFamily::Native => "posix",
            other => other.as_str(),
        }
    }

    /// The boot wrapper this family's entry needs.
    pub fn boot_shape(self) -> BootShape {
        match self {
            BoardFamily::Zephyr => BootShape::Kernel,
            BoardFamily::Native => BootShape::Host,
            BoardFamily::Nuttx | BoardFamily::Freertos | BoardFamily::Threadx => BootShape::App,
        }
    }

    /// The family's canonical name — the same string the serde repr uses.
    pub fn as_str(self) -> &'static str {
        match self {
            BoardFamily::Native => "native",
            BoardFamily::Zephyr => "zephyr",
            BoardFamily::Nuttx => "nuttx",
            BoardFamily::Freertos => "freertos",
            BoardFamily::Threadx => "threadx",
        }
    }
}

/// The C-ABI runners a board family exports. See
/// [`BoardFamily::c_abi_runners`].
///
/// Each half is independently optional. A family may ship `run_components`
/// without `run_tiers`: ThreadX does (issue 1286).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CAbiRunners {
    /// The single-executor runner: `(…, setup) -> int32_t`.
    pub run_components: Option<&'static str>,
    /// The one-task-per-tier runner: `(…, tiers, n_tiers) -> int32_t`.
    pub run_tiers: Option<&'static str>,
}

/// Every board key the entry pipeline knows, and the family it names. This is
/// the ONE table (issue 1285).
///
/// It used to be derived four times, and the four disagreed:
/// - `board_family` here was a `match` whose `_ => Native` turned any unknown
///   key into a host build.
/// - `nros-cli-core`'s `board_to_rtos` was a SUBSTRING match, so `s32z270`,
///   `an536`, `armfvp` and `fvp-aemv8r-smp` read as `posix`.
/// - The proc-macro's `known_boards_csv` was a hand-written list.
/// - The Rust pack's `board_path_for` had its own key set.
///
/// [`board_family`] and `board_to_rtos` now read this table. The Rust pack's
/// key set, `nros_orchestration_ir::BOARD_PATHS`, is Rust-pack-local spelling
/// and stays in its own crate. `nros-cli-core/tests/board_key_table.rs`
/// checks it against this table.
///
/// Rows, by family:
/// - Zephyr: every Phase 215 board (FVP, qemu-zephyr, …) compiles with
///   `__ZEPHYR__` and shares the one metadata-driven adapter.
/// - NuttX (phase 238): the network is up at kernel boot, so these share the
///   lifecycle adapter. `nuttx-riscv` is the Rust spelling of `rv-virt-nuttx`
///   (phase-337 W3).
/// - FreeRTOS: phase 240.6 / phase-263 C2b, plus phase-370's
///   `freertos-posix` and phase-372/385's S32Z270 and MPS3-AN536.
///   `freertos-posix` is a HOST process whose nodes still run as FreeRTOS
///   TASKS, so it belongs here and not with `native`. It once fell through to
///   the host default, a silent wrong answer that surfaced only at link, when
///   `app_main` came up undefined.
/// - ThreadX (phase 246): the host sim and bare-metal qemu-riscv64.
///
/// Board keys with no RTOS are NOT here: `esp32-qemu`, `rtic-mps2-an385` and
/// the bare-metal `mps2-an385`. A C or C++ entry has no runner for them, so
/// asking for their family is an error, not a guess.
pub const BOARD_KEYS: &[(&str, BoardFamily)] = &[
    ("native", BoardFamily::Native),
    ("posix", BoardFamily::Native),
    ("zephyr", BoardFamily::Zephyr),
    // The zephyr descriptor's second name, and a key since phase-445 W5:
    // a Zephyr entry's board is now READ from the image that builds it
    // rather than written as a `deploy = "zephyr"` token, and
    // `examples/workspaces/rust` spells that image's board this way.
    ("native_sim/native/64", BoardFamily::Zephyr),
    ("fvp-aemv8r-smp", BoardFamily::Zephyr),
    ("armfvp", BoardFamily::Zephyr),
    ("nuttx", BoardFamily::Nuttx),
    ("qemu-armv7a-nuttx", BoardFamily::Nuttx),
    ("rv-virt-nuttx", BoardFamily::Nuttx),
    ("nuttx-riscv", BoardFamily::Nuttx),
    ("freertos", BoardFamily::Freertos),
    ("mps2-an385-freertos", BoardFamily::Freertos),
    ("freertos-qemu-mps2-an385", BoardFamily::Freertos),
    ("freertos-posix", BoardFamily::Freertos),
    ("s32z270-freertos", BoardFamily::Freertos),
    ("s32z270", BoardFamily::Freertos),
    ("mps3-an536-freertos", BoardFamily::Freertos),
    ("an536", BoardFamily::Freertos),
    ("threadx", BoardFamily::Threadx),
    ("threadx-linux", BoardFamily::Threadx),
    ("threadx-qemu-riscv64", BoardFamily::Threadx),
    ("rv-virt-threadx", BoardFamily::Threadx),
];

/// A board key that is not in [`BOARD_KEYS`].
///
/// Its `Display` names every known key, so the person who wrote the key can
/// fix it without reading this crate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownBoard {
    pub key: alloc::string::String,
}

impl core::fmt::Display for UnknownBoard {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "unknown board key `{}` — the entry pipeline knows: ",
            self.key
        )?;
        for (i, key) in known_board_keys().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            f.write_str(key)?;
        }
        Ok(())
    }
}

impl core::error::Error for UnknownBoard {}

/// The keys of [`BOARD_KEYS`], in table order.
pub fn known_board_keys() -> impl Iterator<Item = &'static str> {
    BOARD_KEYS.iter().map(|(key, _)| *key)
}

/// Which family a board key names.
///
/// An unknown key is an ERROR that names the known keys (issue 1285). This
/// used to fall back to [`BoardFamily::Native`], which made an embedded key
/// the table had not learned look exactly like a host build. The failure then
/// arrived minutes later, at link, as an undefined `app_main`.
pub fn board_family(board: &str) -> Result<BoardFamily, UnknownBoard> {
    BOARD_KEYS
        .iter()
        .find(|(key, _)| *key == board)
        .map(|(_, family)| *family)
        .ok_or_else(|| UnknownBoard { key: board.into() })
}

/// The tier key a board with NO RTOS family resolves to: the empty string,
/// which names no `[tiers.<name>.<rtos>]` sub-table.
///
/// Issue 1285 follow-up. Each lenient caller used to spell its own fallback.
/// `plan_from_model` wrote `""`, while the proc-macro and the CLI's
/// `codegen-system` wrote `"posix"` from a SUBSTRING match. So an RTIC entry
/// read its tiers from the host's sub-table in one Rust producer and from none
/// in the other. What this key means downstream:
/// - `resolve_tiers` refuses an AUTHORED tier (`TierResolveError::NoRtosFamily`):
///   there is no sub-table to read and no task to run it. The degenerate
///   default tier is unaffected.
/// - `sched_caps_for` answers the bare-metal caps.
/// - `derive_tiers_from_contracts` derives no tier, and records a degradation
///   for each node it would have placed.
pub const NO_RTOS_TIER_KEY: &str = "";

/// The `[tiers.<name>.<rtos>]` key for an entry board key, for a caller where
/// an UNKNOWN key is legal.
///
/// Read from [`BOARD_KEYS`]. A key the table does not know gets
/// [`NO_RTOS_TIER_KEY`], never a guess from how the key is spelled. That covers
/// a no-RTOS Rust board (`esp32-qemu`, `rtic-mps2-an385`, bare `mps2-an385`)
/// and an out-of-tree board. The old substring match read `my-freertos-board`
/// as FreeRTOS and `s32z270` as the host.
///
/// Two callers are lenient because the issue documents that the key there is
/// any Rust or out-of-tree board: `plan_from_model` (shared with `nros build`'s
/// Rust entry generation) and the `nros::main!` proc-macro. A caller that
/// NEEDS the family asks [`board_family`] and refuses the key.
pub fn tier_rtos_key_for(board: &str) -> &'static str {
    board_family(board).map_or(NO_RTOS_TIER_KEY, BoardFamily::tier_rtos_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue 1285 follow-up: the lenient lookup is the table read one way. It
    /// is never the spelling of the key.
    #[test]
    fn tier_rtos_key_for_reads_the_table() {
        for (key, family) in BOARD_KEYS {
            assert_eq!(tier_rtos_key_for(key), family.tier_rtos_key(), "`{key}`");
        }
        // The old substring match's victims: none contains its RTOS's name.
        for (key, rtos) in [
            ("s32z270", "freertos"),
            ("an536", "freertos"),
            ("armfvp", "zephyr"),
            ("fvp-aemv8r-smp", "zephyr"),
        ] {
            assert_eq!(tier_rtos_key_for(key), rtos, "`{key}`");
        }
    }

    /// An unknown key names no RTOS, including one whose NAME contains an RTOS.
    /// The substring match read those as that RTOS, and every other key as the
    /// host.
    #[test]
    fn an_unknown_key_names_no_rtos() {
        for key in [
            "my-freertos-board",
            "zephyr-custom",
            "nuttx-fork",
            "threadx-port",
            "esp32-qemu",
            "rtic-mps2-an385",
            "mps2-an385",
            "stm32f4",
        ] {
            assert_eq!(tier_rtos_key_for(key), NO_RTOS_TIER_KEY, "`{key}`");
        }
    }

    #[test]
    fn every_known_board_key_lands_in_its_family() {
        for (key, want) in BOARD_KEYS {
            assert_eq!(board_family(key), Ok(*want), "board key `{key}`");
        }
        // 19 keys the C++ emitter knew, plus the two RTOS keys only the Rust
        // pack knew (`nuttx-riscv`, `freertos-qemu-mps2-an385`, issue 1285),
        // plus `native_sim/native/64` (phase-445 W5).
        assert_eq!(
            BOARD_KEYS.len(),
            22,
            "a key was added or removed without a row"
        );
    }

    #[test]
    fn no_board_key_is_listed_twice() {
        for (i, (key, _)) in BOARD_KEYS.iter().enumerate() {
            assert!(
                !BOARD_KEYS[..i].iter().any(|(k, _)| k == key),
                "board key `{key}` has two rows — the first would shadow the second"
            );
        }
    }

    /// Each tier-key spelling is what rlm's `TierDef::platform` matches. The
    /// host is `posix` there, not `native`.
    #[test]
    fn tier_rtos_key_is_the_spelling_the_tier_tables_use() {
        assert_eq!(BoardFamily::Native.tier_rtos_key(), "posix");
        for f in BoardFamily::ALL {
            if f != BoardFamily::Native {
                assert_eq!(f.tier_rtos_key(), f.as_str());
            }
        }
    }

    /// The routing predicate is DERIVED from the runner names. The two used to
    /// be separate tables that agreed by accident.
    #[test]
    fn has_c_run_components_is_the_runner_table_read_one_way() {
        for f in BoardFamily::ALL {
            assert_eq!(
                f.has_c_run_components(),
                f.c_abi_runners().and_then(|r| r.run_components).is_some(),
                "{}",
                f.as_str()
            );
        }
    }

    /// Issue 1286. ThreadX has the shared `run_components` and NO `run_tiers`.
    /// So a C entry renders as C, and a multi-tier plan takes the
    /// sched-context path, not `run_tiers`. Both halves are pinned: a
    /// `run_tiers` named here would be a symbol defined nowhere, which is the
    /// defect issue 1285 removed.
    #[test]
    fn threadx_has_run_components_and_no_run_tiers() {
        assert_eq!(
            BoardFamily::Threadx.c_abi_runners(),
            Some(CAbiRunners {
                run_components: Some("nros_board_rtos_run_components"),
                run_tiers: None,
            })
        );
        assert!(BoardFamily::Threadx.has_c_run_components());
    }

    /// Every family now has a C-ABI `run_components` (issue 1286 closed the
    /// last one), so a `--lang c` entry renders as C on every board. A new
    /// family without one fails here and must decide, rather than silently
    /// routing to the C++ pack.
    #[test]
    fn every_family_has_a_c_run_components() {
        for f in BoardFamily::ALL {
            assert!(f.has_c_run_components(), "{}", f.as_str());
        }
    }

    /// `freertos-posix` is the trap this table exists to hold. It is a host
    /// PROCESS, so it reads like `native` — but its nodes run as FreeRTOS
    /// tasks and its `startup.c` owns `main`, so a host `int main` compiles
    /// and then fails at link on an undefined `app_main`.
    #[test]
    fn freertos_posix_is_freertos_not_native() {
        assert_eq!(board_family("freertos-posix"), Ok(BoardFamily::Freertos));
        assert_eq!(
            board_family("freertos-posix").unwrap().boot_shape(),
            BootShape::App,
            "a host `int main` here is a second main"
        );
    }

    /// The boot shape is three-valued for a reason: Zephyr is embedded AND
    /// takes the host-looking `main(void)`, so `is_embedded` cannot decide it.
    #[test]
    fn zephyr_is_embedded_but_boots_through_main() {
        assert!(BoardFamily::Zephyr.is_embedded());
        assert_eq!(BoardFamily::Zephyr.boot_shape(), BootShape::Kernel);
        assert_ne!(BoardFamily::Zephyr.boot_shape(), BootShape::Host);
    }

    /// ThreadX is the board the two former spellings in `emit_cpp` disagreed
    /// about — one tested `freertos || nuttx`, the other `is_embedded`. Pinned
    /// here so the one derivation stays right about it on its own terms.
    #[test]
    fn threadx_boots_through_the_board_not_a_host_main() {
        assert_eq!(BoardFamily::Threadx.boot_shape(), BootShape::App);
        assert_eq!(
            BoardFamily::Threadx.boot_shape(),
            BoardFamily::Nuttx.boot_shape()
        );
    }

    /// Issue 1285 — an unknown key is an error, never the host. The message
    /// names every known key, so the fix is visible from the failure.
    #[test]
    fn an_unknown_key_is_an_error_naming_the_known_keys() {
        use alloc::string::ToString;
        for key in ["zigos", "", "esp32-qemu", "mps2-an385", "NATIVE"] {
            let err = board_family(key).expect_err(key);
            assert_eq!(err.key, key);
            let msg = err.to_string();
            assert!(msg.contains(&alloc::format!("`{key}`")), "{msg}");
            for known in known_board_keys() {
                assert!(msg.contains(known), "message omits `{known}`: {msg}");
            }
        }
    }

    #[test]
    fn only_native_is_not_embedded() {
        for f in BoardFamily::ALL {
            assert_eq!(
                f.is_embedded(),
                f != BoardFamily::Native,
                "{} embeddedness",
                f.as_str()
            );
        }
    }

    #[test]
    fn all_is_exhaustive() {
        for f in BoardFamily::ALL {
            // Exhaustive match: a new family fails to compile here first.
            let named = match f {
                BoardFamily::Native => "native",
                BoardFamily::Zephyr => "zephyr",
                BoardFamily::Nuttx => "nuttx",
                BoardFamily::Freertos => "freertos",
                BoardFamily::Threadx => "threadx",
            };
            assert_eq!(named, f.as_str());
        }
        assert_eq!(BoardFamily::ALL.len(), 5);
    }
}
