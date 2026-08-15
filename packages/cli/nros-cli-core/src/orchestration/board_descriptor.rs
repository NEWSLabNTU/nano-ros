//! Data-driven board profiles (Phase 195.C).
//!
//! The `nros` CLI is shipped from a *separate* repo, so it must carry **no**
//! baked-in knowledge of the nano-ros workspace layout. Every per-board fact
//! — which board crate to depend on, the rustc target, the `.cargo/config.toml`
//! body, the kernel-port / libc paths, the generated entry-point shape — lives
//! in a `nros-board.toml` descriptor *in the workspace* and is read at runtime.
//!
//! Discovery is uniform: every `packages/boards/*/nros-board.toml` is loaded
//! (crate-backed boards put the file in their crate dir; the crate-less host
//! boards — `posix`, `zephyr` — get a descriptor-only dir under
//! `packages/boards/`). A file holds a `[[board]]` array so one crate can back
//! several boards (e.g. `nros-board-nuttx-qemu` → arm virt + rv-virt,
//! differing only by `chip`).
//!
//! Layout paths in `cargo_config` are stored **relative** and written with the
//! `${workspace}` placeholder; the CLI substitutes the workspace root it
//! discovered at render time, so the binary stays workspace-agnostic.

use std::path::Path;

use serde::Deserialize;

/// Resolved platform identity for a `(board, target)` pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlatformKind {
    Posix,
    Freertos,
    BareMetal,
    Nuttx,
    Zephyr,
    ThreadxLinux,
    ThreadxRiscv64,
    Esp32,
    Stm32,
    OrinSpe,
}

impl PlatformKind {
    /// The kebab-case token this variant deserializes from — the spelling that
    /// appears as `platform = "…"` in `nros-board.toml`.
    ///
    /// phase-341 W2 needs it because a leaf's `[package.metadata.nros.entry]
    /// deploy` is matched against the descriptor's `platform` (the mapping
    /// `scripts/check-board-cargo-config-applied.sh` already uses). Written as
    /// an exhaustive match rather than a serde round-trip so adding a variant
    /// is a compile error here instead of a silently unmatched board;
    /// `platform_kebab_round_trips` proves the two spellings agree.
    pub fn kebab(self) -> &'static str {
        match self {
            PlatformKind::Posix => "posix",
            PlatformKind::Freertos => "freertos",
            PlatformKind::BareMetal => "bare-metal",
            PlatformKind::Nuttx => "nuttx",
            PlatformKind::Zephyr => "zephyr",
            PlatformKind::ThreadxLinux => "threadx-linux",
            PlatformKind::ThreadxRiscv64 => "threadx-riscv64",
            PlatformKind::Esp32 => "esp32",
            PlatformKind::Stm32 => "stm32",
            PlatformKind::OrinSpe => "orin-spe",
        }
    }
}

/// Rust toolchain a generated package pins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Toolchain {
    /// Stable rustc with a prebuilt target — no `rust-toolchain.toml`.
    Stable,
    /// Pinned nightly + `rust-src` for `-Z build-std`.
    Nightly,
    /// Xtensa `+esp` espup toolchain (ESP32-S3).
    Esp,
}

/// External libraries the generated `build.rs` must link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LinkKind {
    /// Board crate / cargo handles all linking.
    None,
    /// NuttX staging-archive group-link + dramboot linker script.
    NuttxStaging,
}

/// Shape of the generated package's entry point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EntryKind {
    /// Hosted Rust `fn main` (posix / threadx-linux host).
    HostedMain,
    /// `<board>::run(cfg, closure)` on a bare-metal / RTOS target.
    BoardRun,
    /// Rust staticlib consumed by zephyr-lang-rust `rust_cargo_application()`.
    ZephyrStaticlib,
}

// phase-351 W6 — `NetStack` (`rtos-owned` / `nanoros-owned`) is GONE. It was
// parsed and never read from the day it was added, and it answered the wrong
// question: "who brings up NIC+IP", not "which stack", so nothing could act on
// it. `supported_netstacks` (W4) answers the question consumers actually ask
// and IS read — by `resolve_netstack`, by `nros ws board-facts`, and by
// `check-site-config`.

/// The per-board pieces the entry-point renderer interpolates into the shared
/// board-run entry shape. `None` path interpolation here — these reference only
/// the board crate name.
#[derive(Debug, Clone, Deserialize)]
pub struct BoardEntry {
    /// Board rlib invoked as `<crate>::run(<crate>::Config::default(), ..)`.
    pub crate_name: String,
    /// Doc comment emitted directly above the entry fn.
    #[serde(default)]
    pub comment: String,
    /// Attribute(s) + `fn` signature line(s) preceding the fn body.
    pub signature: String,
    /// Crate-root `use`s / items pinned above the entry (panic handler, etc.).
    #[serde(default)]
    pub crate_root_extra: String,
    /// Builder-chain suffix appended inside the closure; empty for most boards.
    #[serde(default)]
    pub closure_extra: String,
}

/// Declared board capabilities (RFC-0042 D2 / phase-241 wave C). The single
/// source of truth for what a board provides; the generator (241.C.2) lowers
/// each to the right per-platform mechanism — `-D NROS_PLATFORM_HAS_*` for
/// baremetal/threadx, Kconfig (`prj.conf`) for zephyr, etc. — instead of the
/// per-RTOS-header self-`#define`s + the one hand-set cmake `-D` they replace.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct BoardCapabilities {
    /// Board has a usable heap allocator. Drives the canonical malloc/free +
    /// `NROS_PLATFORM_HAS_MALLOC` (and `!NROS_NO_DYNAMIC_MEMORY` on bare-metal).
    #[serde(default)]
    pub heap: bool,
    /// Board provides atomic load/store. Drives `NROS_PLATFORM_HAS_ATOMICS`.
    #[serde(default)]
    pub atomics: bool,
    /// Board has threads + a mutex. Drives `NROS_FEATURE_THREADS` /
    /// `NROS_PLATFORM_HAS_MUTEX`.
    #[serde(default)]
    pub threads: bool,
}

impl BoardCapabilities {
    /// Conservative defaults inferred from the platform when a board omits the
    /// `[board.capabilities]` block (migration path; a lint flags reliance on
    /// inference). RTOS + hosted platforms have a heap/threads; generic
    /// bare-metal does not (it must opt in — the #38 lesson). Atomics are
    /// assumed everywhere (every supported target provides them today).
    fn inferred(platform: PlatformKind) -> Self {
        use PlatformKind::*;
        match platform {
            Posix | Freertos | Nuttx | Zephyr | ThreadxLinux | ThreadxRiscv64 | Esp32 => {
                BoardCapabilities {
                    heap: true,
                    atomics: true,
                    threads: true,
                }
            }
            // Generic bare-metal / SPE: no heap by default (opt in via board.toml).
            BareMetal | Stm32 | OrinSpe => BoardCapabilities {
                heap: false,
                atomics: true,
                threads: false,
            },
        }
    }
}

/// One board profile. Mirrors the old hardcoded `PlatformProfile` +
/// `BoardEntry`, but every field is owned data read from `nros-board.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct BoardDescriptor {
    /// Board name + accepted aliases (the values a user passes as `board`).
    pub names: Vec<String>,
    pub platform: PlatformKind,
    /// rustc target triple this board pins, if any (`None` → take from plan).
    #[serde(default)]
    pub target: Option<String>,
    pub toolchain: Toolchain,
    /// The `nros/<feature>` selected (e.g. `platform-posix`).
    pub platform_feature: String,
    /// Extra local default-feature aliases beyond `nros/<feature>`.
    #[serde(default)]
    pub local_aliases: Vec<String>,
    pub link_kind: LinkKind,
    pub entry_kind: EntryKind,
    /// phase-351 W4 — the network stacks this board can actually be built with,
    /// in preference order; the first is the default when a deploy names none.
    ///
    /// A FACT of the board, not a menu: every vendor has already welded its
    /// choice, and the pairing has a validity domain (NetX Duo ships a port
    /// table of 24 arches against ThreadX's 47, so a ThreadX arch with no NetX
    /// counterpart cannot be paired at all). Empty means "this board makes no
    /// stack choice" — the RTOS or the host owns it and a deploy must not try
    /// to select one.
    #[serde(default)]
    pub supported_netstacks: Vec<String>,
    /// esp-hal / stm32 chip feature; `None` for non-chip platforms.
    #[serde(default)]
    pub chip: Option<String>,
    /// Board crate to depend on; `None` for crate-less host boards
    /// (posix / zephyr) that pull static or `nros-platform-cffi` deps.
    #[serde(default)]
    pub board_crate: Option<String>,
    /// Board-crate path relative to the workspace root; defaults to
    /// `packages/boards/<board_crate>` when omitted.
    #[serde(default)]
    pub crate_path: Option<String>,
    /// Extra features to enable on the board crate dependency.
    #[serde(default)]
    pub board_features: Vec<String>,
    /// Phase 252 — the capability-axis features this board crate forwards to its
    /// backend (e.g. `["safety-e2e"]` → the board's `safety-e2e = ["nros-rmw-zenoh?/safety-e2e"]`).
    /// A declared `[safety]` axis lowers to the board feature only when the board
    /// advertises it here; otherwise codegen skips it + warns (so a board without
    /// the feature is never a Cargo error). Empty ⇒ the board carries no capability
    /// forwarding yet. → RFC-0031 § "Generalization", issue 0072.
    #[serde(default)]
    pub capability_features: Vec<String>,
    /// Verbatim `.cargo/config.toml` body, with `${workspace}` placeholders for
    /// any layout path. `None` for boards that need no config (posix/zephyr/…).
    #[serde(default)]
    pub cargo_config: Option<String>,
    /// Generated entry-point pieces; `None` for hosted boards that emit the
    /// default `fn main` shape.
    #[serde(default)]
    pub entry: Option<BoardEntry>,
    /// Disambiguate two descriptors sharing a `names` entry by requiring this
    /// substring in the requested target (e.g. `"riscv64"` for threadx-riscv64,
    /// so `board = "threadx"` picks riscv64 vs linux by target).
    #[serde(default)]
    pub target_contains: Option<String>,
    /// Declared board capabilities (heap/atomics/threads). `None` → inferred from
    /// `platform` via `capabilities()` during the 241.C migration.
    #[serde(default)]
    pub capabilities: Option<BoardCapabilities>,
    /// CMake cross-compile facts for `nros setup`'s CMakePreset emission
    /// (RFC-0048 §6 / phase-287 W5). `None` for host boards (posix) that need no
    /// toolchain file — their preset carries only `nano_ros_ROOT`.
    #[serde(default)]
    pub cmake: Option<BoardCmake>,
    /// phase-341 W2 — the `nros-board.toml` this descriptor was read from,
    /// workspace-relative. Set by [`BoardCatalog::load`], `None` for
    /// in-memory descriptors. Recorded rather than derived from
    /// `crate_path_rel()`: a generated projection of `cargo_config` names its
    /// SSoT in the DO-NOT-EDIT header, and a header that names the wrong file
    /// sends the next reader to edit the wrong descriptor.
    #[serde(skip)]
    pub source: Option<String>,
}

/// `[board.cmake]` — CMake toolchain facts for the ament-shape preset flow.
/// Deliberately minimal: only the board-intrinsic toolchain file. The SDK
/// directory cache-vars (`NUTTX_DIR`, `THREADX_DIR`, …) are NOT restated here —
/// the platform CMake modules default them from their own on-disk location, and
/// the store compiler bin flows onto the preset's `environment.PATH` from the
/// provision result. No `${…}` templating (RFC-0048 §6, shape C′).
#[derive(Debug, Clone, Deserialize)]
pub struct BoardCmake {
    /// Repo-relative path to the CMake toolchain file, e.g.
    /// `cmake/toolchain/armv7a-nuttx-eabi.cmake`. `nros setup` resolves it against
    /// the repo root and emits it as the preset's `toolchainFile`.
    pub toolchain_file: String,
}

/// phase-351 W4 — why a `[deploy.<name>.nros].netstack` was refused.
///
/// Both arms name what IS available, because the whole point of declaring the
/// domain is that a user who picked outside it can see the edge.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NetstackError {
    #[error(
        "board `{board}` does not support netstack `{requested}` — it supports: {}",
        supported.join(", ")
    )]
    Unsupported {
        board: String,
        requested: String,
        supported: Vec<String>,
    },
    #[error(
        "board `{board}` declares no `supported_netstacks`, so `netstack = \"{requested}\"` \
         selects nothing: this board's RTOS (or its host) owns the stack. Drop the key."
    )]
    BoardSelectsNone { board: String, requested: String },
}

impl BoardDescriptor {
    /// Board-crate path relative to the workspace root, applying the
    /// `packages/boards/<board_crate>` default.
    pub fn crate_path_rel(&self) -> Option<String> {
        self.crate_path.clone().or_else(|| {
            self.board_crate
                .as_ref()
                .map(|c| format!("packages/boards/{c}"))
        })
    }

    /// Resolved board capabilities — the declared `[board.capabilities]` block,
    /// or platform-inferred conservative defaults when omitted (241.C migration).
    pub fn capabilities(&self) -> BoardCapabilities {
        self.capabilities
            .unwrap_or_else(|| BoardCapabilities::inferred(self.platform))
    }

    /// Whether the board declared its capabilities explicitly (vs relying on the
    /// platform-inferred defaults). Used by the migration lint.
    pub fn has_declared_capabilities(&self) -> bool {
        self.capabilities.is_some()
    }

    /// phase-351 W4 — the netstack this deploy will build with, or an error
    /// naming what the board actually supports.
    ///
    /// `requested` is `[deploy.<name>.nros].netstack`. `None` takes the board's
    /// first declared stack, which is why the list is ordered. A board that
    /// declares NO stacks makes no choice: naming one there is an error too,
    /// because silently ignoring it is how a deploy ends up believing it
    /// selected something.
    pub fn resolve_netstack<'a>(
        &'a self,
        requested: Option<&'a str>,
    ) -> Result<Option<&'a str>, NetstackError> {
        match (requested, self.supported_netstacks.first()) {
            (None, default) => Ok(default.map(String::as_str)),
            (Some(want), None) => Err(NetstackError::BoardSelectsNone {
                board: self.names.first().cloned().unwrap_or_default(),
                requested: want.to_string(),
            }),
            (Some(want), Some(_)) => {
                if self.supported_netstacks.iter().any(|s| s == want) {
                    Ok(Some(want))
                } else {
                    Err(NetstackError::Unsupported {
                        board: self.names.first().cloned().unwrap_or_default(),
                        requested: want.to_string(),
                        supported: self.supported_netstacks.clone(),
                    })
                }
            }
        }
    }

    /// Render `cargo_config` with `${workspace}` resolved to `workspace`.
    pub fn cargo_config_rendered(&self, workspace: &Path) -> Option<String> {
        let ws = path_for_template(workspace);
        self.cargo_config
            .as_ref()
            .map(|body| body.replace("${workspace}", &ws))
    }
}

/// Escape a path for embedding inside a double-quoted TOML string.
fn path_for_template(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

#[derive(Debug, Deserialize)]
struct BoardFile {
    #[serde(default, rename = "board")]
    boards: Vec<BoardDescriptor>,
}

/// Every board descriptor discovered under `<workspace>/packages/boards`.
#[derive(Debug, Default)]
pub struct BoardCatalog {
    descriptors: Vec<BoardDescriptor>,
}

impl BoardCatalog {
    /// Load every `packages/boards/*/nros-board.toml` under `workspace`.
    pub fn load(workspace: &Path) -> Result<Self, BoardLoadError> {
        let boards_dir = workspace.join("packages/boards");
        let mut descriptors = Vec::new();
        let entries = std::fs::read_dir(&boards_dir)
            .map_err(|e| BoardLoadError::Io(boards_dir.clone(), e))?;
        for entry in entries {
            let entry = entry.map_err(|e| BoardLoadError::Io(boards_dir.clone(), e))?;
            let descriptor_path = entry.path().join("nros-board.toml");
            if !descriptor_path.is_file() {
                continue;
            }
            let text = std::fs::read_to_string(&descriptor_path)
                .map_err(|e| BoardLoadError::Io(descriptor_path.clone(), e))?;
            let file: BoardFile = toml::from_str(&text)
                .map_err(|e| BoardLoadError::Parse(descriptor_path.clone(), e))?;
            // Workspace-relative, forward slashes — it goes into a COMMITTED
            // generated header, so an absolute host path would be drift the
            // moment anyone else regenerates it.
            let rel = descriptor_path
                .strip_prefix(workspace)
                .unwrap_or(&descriptor_path)
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            descriptors.extend(file.boards.into_iter().map(|mut b| {
                b.source = Some(rel.clone());
                b
            }));
        }
        Ok(Self { descriptors })
    }

    /// Build a catalog from already-parsed descriptors (tests / in-memory).
    pub fn from_descriptors(descriptors: Vec<BoardDescriptor>) -> Self {
        Self { descriptors }
    }

    pub fn descriptors(&self) -> &[BoardDescriptor] {
        &self.descriptors
    }

    /// Resolve a `(board, target)` pair to its descriptor.
    ///
    /// A board name may be claimed by two descriptors (e.g. `threadx` →
    /// `threadx-riscv64` vs `threadx-linux`); the one whose `target_contains`
    /// matches the requested target wins, else the unconstrained one. As a last
    /// resort an unknown board on a `*-linux*` target resolves to `posix`
    /// (mirrors the old `target.contains("linux")` fallback).
    pub fn resolve(&self, board: &str, target: &str) -> Option<&BoardDescriptor> {
        let named: Vec<&BoardDescriptor> = self
            .descriptors
            .iter()
            .filter(|d| d.names.iter().any(|n| n == board))
            .collect();
        if !named.is_empty() {
            // Prefer a target-qualified match, then the unconstrained one.
            return named
                .iter()
                .find(|d| {
                    d.target_contains
                        .as_ref()
                        .is_some_and(|sub| target.contains(sub.as_str()))
                })
                .or_else(|| named.iter().find(|d| d.target_contains.is_none()))
                .copied();
        }
        if target.contains("linux") {
            return self
                .descriptors
                .iter()
                .find(|d| d.platform == PlatformKind::Posix);
        }
        None
    }

    /// phase-341 W2 — resolve a leaf's `[package.metadata.nros.entry] deploy`
    /// token to the descriptor whose `cargo_config` governs that leaf's link.
    ///
    /// [`resolve`] cannot serve here: it takes a `(board, target)` pair and the
    /// target is exactly what the projection is trying to *derive*. Two rules,
    /// in order, each requiring a UNIQUE hit:
    ///
    /// 1. **`names`** — the board's own name/alias list. This is what separates
    ///    the two descriptors that share a triple *and* a platform:
    ///    `nuttx` (armv7a) vs `nuttx-riscv` (riscv32imac) are one `names` entry
    ///    apart, and `target_contains` — [`resolve`]'s discriminator — is
    ///    useless without a target to test it against.
    /// 2. **`platform`** — the mapping
    ///    `scripts/check-board-cargo-config-applied.sh` uses (it matches a
    ///    leaf's `deploy` against each `platform = "…"`). Reached only when no
    ///    board CLAIMS the name, and only when exactly one board declares that
    ///    platform — `nuttx` is declared by two, so a deploy token that reached
    ///    this rule for it is ambiguous rather than "the first one".
    ///
    /// Anything else is [`DeployResolution::Unknown`] / `Ambiguous`, and the
    /// caller must write NOTHING. A projection carrying the wrong board's link
    /// args is worse than no projection: the hand-mirrored block it would
    /// shadow is at least the block that links today (issue 0440).
    pub fn resolve_deploy(&self, deploy: &str) -> DeployResolution<'_> {
        let by_name: Vec<&BoardDescriptor> = self
            .descriptors
            .iter()
            .filter(|d| d.names.iter().any(|n| n == deploy))
            .collect();
        match by_name.len() {
            1 => return DeployResolution::Board(by_name[0]),
            0 => {}
            _ => return DeployResolution::Ambiguous(descriptor_labels(&by_name)),
        }
        let by_platform: Vec<&BoardDescriptor> = self
            .descriptors
            .iter()
            .filter(|d| d.platform.kebab() == deploy)
            .collect();
        match by_platform.len() {
            1 => DeployResolution::Board(by_platform[0]),
            0 => DeployResolution::Unknown,
            _ => DeployResolution::Ambiguous(descriptor_labels(&by_platform)),
        }
    }
}

/// Human-readable identity of each candidate, for an ambiguity diagnostic.
fn descriptor_labels(candidates: &[&BoardDescriptor]) -> Vec<String> {
    candidates
        .iter()
        .map(|d| match &d.source {
            Some(src) => format!("{} ({src})", d.names.join("/")),
            None => d.names.join("/"),
        })
        .collect()
}

/// Outcome of [`BoardCatalog::resolve_deploy`].
#[derive(Debug)]
pub enum DeployResolution<'a> {
    /// Exactly one descriptor claims this deploy token.
    Board(&'a BoardDescriptor),
    /// No descriptor claims it (e.g. a deploy key known only to
    /// `nros_orchestration_ir::board_path_for`, or an out-of-tree board).
    Unknown,
    /// Several descriptors claim it and nothing here can choose between them.
    Ambiguous(Vec<String>),
}

/// Error loading or parsing board descriptors.
#[derive(Debug)]
pub enum BoardLoadError {
    Io(std::path::PathBuf, std::io::Error),
    Parse(std::path::PathBuf, toml::de::Error),
}

impl std::fmt::Display for BoardLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BoardLoadError::Io(path, e) => write!(f, "reading {}: {e}", path.display()),
            BoardLoadError::Parse(path, e) => write!(f, "parsing {}: {e}", path.display()),
        }
    }
}

impl std::error::Error for BoardLoadError {}

#[cfg(test)]
mod tests {
    use super::*;

    // ── phase-351 W4 — supported_netstacks ────────────────────────────────

    fn board_with(stacks: &[&str]) -> BoardDescriptor {
        let mut d: BoardDescriptor = toml::from_str::<BoardFile>(STM32_TOML)
            .expect("fixture parses")
            .boards
            .remove(0);
        d.supported_netstacks = stacks.iter().map(|s| s.to_string()).collect();
        d
    }

    /// No request takes the board's FIRST declared stack — which is why the
    /// list is ordered rather than a set.
    #[test]
    fn unrequested_netstack_takes_the_boards_default() {
        assert_eq!(
            board_with(&["lwip", "freertos_plus_tcp"])
                .resolve_netstack(None)
                .unwrap(),
            Some("lwip")
        );
    }

    #[test]
    fn a_supported_netstack_resolves_to_itself() {
        assert_eq!(
            board_with(&["lwip", "freertos_plus_tcp"])
                .resolve_netstack(Some("freertos_plus_tcp"))
                .unwrap(),
            Some("freertos_plus_tcp")
        );
    }

    /// The error must NAME the domain — a refusal that does not say what is
    /// available just moves the guessing.
    #[test]
    fn an_unsupported_netstack_lists_what_is_supported() {
        let err = board_with(&["netxduo"])
            .resolve_netstack(Some("lwip"))
            .expect_err("lwip is not in the board's table");
        let msg = err.to_string();
        assert!(msg.contains("netxduo"), "{msg}");
        assert!(msg.contains("lwip"), "{msg}");
    }

    /// A board that declares none makes no choice, so naming one is an error
    /// rather than a silent no-op: the deploy would otherwise believe it had
    /// selected something.
    #[test]
    fn naming_a_netstack_on_a_board_that_has_none_is_refused() {
        let err = board_with(&[])
            .resolve_netstack(Some("lwip"))
            .expect_err("a board with no table cannot honour a request");
        assert!(err.to_string().contains("owns the stack"), "{err}");
        assert_eq!(board_with(&[]).resolve_netstack(None).unwrap(), None);
    }

    /// The SHIPPED descriptors, so the declarations cannot rot: each board that
    /// claims a stack must resolve it, and the empties must refuse.
    #[test]
    fn shipped_boards_declare_a_resolvable_domain() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            // packages/cli/nros-cli-core -> cli -> packages -> repo root
            .ancestors()
            .nth(3)
            .expect("repo root")
            .to_path_buf();
        // No `Err(_) => return` escape: this test asserts about the SHIPPED
        // descriptors, so a catalog it cannot load is a failure, not a reason
        // to report green having checked nothing (issue 0571's shape).
        let catalog = BoardCatalog::load(&root)
            .unwrap_or_else(|e| panic!("shipped board catalog under {}: {e}", root.display()));
        let mut checked = 0;
        for d in catalog.descriptors() {
            for want in &d.supported_netstacks {
                assert_eq!(
                    d.resolve_netstack(Some(want)).unwrap(),
                    Some(want.as_str()),
                    "board {:?} does not resolve its own declared stack",
                    d.names
                );
                checked += 1;
            }
            assert!(
                d.resolve_netstack(None).is_ok(),
                "board {:?} cannot resolve its default",
                d.names
            );
        }
        assert!(checked > 0, "no shipped board declares a netstack");
    }

    const STM32_TOML: &str = r##"
[[board]]
names = ["stm32f4", "stm32f429"]
platform = "stm32"
target = "thumbv7em-none-eabihf"
toolchain = "stable"
platform_feature = "platform-bare-metal"
local_aliases = ["platform-stm32"]
link_kind = "none"
entry_kind = "board-run"
chip = "stm32f429"
board_crate = "nros-board-stm32f4"
cargo_config = """
[build]
target = "thumbv7em-none-eabihf"
"""

[board.entry]
crate_name = "nros_board_stm32f4"
signature = "#[nros_board_stm32f4::entry]\nfn main() -> !"
crate_root_extra = "use panic_probe as _;"

[[board]]
names = ["stm32f407"]
platform = "stm32"
target = "thumbv7em-none-eabihf"
toolchain = "stable"
platform_feature = "platform-bare-metal"
link_kind = "none"
entry_kind = "board-run"
chip = "stm32f407"
board_crate = "nros-board-stm32f4"

[board.entry]
crate_name = "nros_board_stm32f4"
signature = "#[nros_board_stm32f4::entry]\nfn main() -> !"
"##;

    fn catalog() -> BoardCatalog {
        let file: BoardFile = toml::from_str(STM32_TOML).expect("parse stm32 descriptor");
        BoardCatalog::from_descriptors(file.boards)
    }

    /// phase-241 C.4 — migration lint (merge gate): every in-tree board must
    /// declare `[board.capabilities]` rather than rely on the platform-inferred
    /// defaults. All boards declare today; this catches a future board that
    /// omits the block (which would silently inherit a possibly-wrong heap/
    /// threads default — the issue-0038 footgun).
    /// Phase 252 (issue 0072) — a board descriptor advertises the `safety-e2e`
    /// capability feature, so codegen lowers `[safety]` to that board's
    /// `safety-e2e = ["nros-rmw-zenoh?/safety-e2e"]` forwarding.
    ///
    /// phase-337 W7.a rehomed this from `stm32f4` (deleted with its board) to
    /// `bare-metal` (the mps2-an385 descriptor). The assertion is over the SET rather than one name: the
    /// point is that the forwarding chain has at least one in-tree witness, and
    /// keying it to a single board is what made a board deletion look like a
    /// capability regression.
    #[test]
    fn a_board_advertises_the_safety_capability_feature() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("repo root")
            .to_path_buf();
        let cat = BoardCatalog::load(&root).expect("load real board catalog");
        let advertising: Vec<&str> = cat
            .descriptors()
            .iter()
            .filter(|d| d.capability_features.iter().any(|f| f == "safety-e2e"))
            .flat_map(|d| d.names.iter().map(|n| n.as_str()))
            .collect();
        assert!(
            advertising.contains(&"bare-metal"),
            "bare-metal (mps2-an385) must advertise safety-e2e; \
             advertising boards: {advertising:?}"
        );
    }

    #[test]
    fn every_in_tree_board_declares_capabilities() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("repo root from packages/cli/nros-cli-core")
            .to_path_buf();
        let cat = BoardCatalog::load(&root).expect("load real board catalog");
        assert!(
            !cat.descriptors().is_empty(),
            "no boards loaded from {}/packages/boards",
            root.display()
        );
        let undeclared: Vec<String> = cat
            .descriptors()
            .iter()
            .filter(|d| !d.has_declared_capabilities())
            .map(|d| d.names.join("/"))
            .collect();
        assert!(
            undeclared.is_empty(),
            "boards relying on inferred capabilities — add [board.capabilities] \
             to their nros-board.toml: {undeclared:?}"
        );
    }

    /// Read a FreeRTOS config header and INLINE its relative `#include "..."`
    /// siblings, so the caller sees the same text the compiler does.
    ///
    /// phase-337 W5.a hoisted the shared body of `FreeRTOSConfig.h` into
    /// `nros-board-freertos/config/`, leaving each board's copy as two
    /// `#define`s plus a relative include. The agreement gate below reads that
    /// file directly, so from W5.a until phase-337 W7.b it saw NO
    /// `configSUPPORT_DYNAMIC_ALLOCATION` at all and read it as `0` — the gate
    /// went red while the thing it guards was fine. A gate that stops at the
    /// first file is narrower than the rule it enforces (the issue-0196 class),
    /// so it follows the include instead.
    ///
    /// Deliberately NOT a C preprocessor: it resolves `#include "relative/path"`
    /// against the including file's directory, depth-first, and ignores
    /// `#include <system>` and any conditional compilation. That is exactly the
    /// shape these config headers use, and anything richer would be a second
    /// implementation of cpp living in a test.
    fn read_freertos_config(path: &std::path::Path) -> Option<String> {
        fn walk(path: &std::path::Path, depth: usize, out: &mut String) {
            // A cycle or a pathological chain must not hang the test suite.
            if depth > 8 {
                return;
            }
            let Ok(src) = std::fs::read_to_string(path) else {
                return;
            };
            let dir = path.parent().unwrap_or(std::path::Path::new("."));
            for line in src.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("#include")
                    && let Some(open) = rest.find('"')
                    && let Some(close) = rest[open + 1..].find('"')
                {
                    let rel = &rest[open + 1..open + 1 + close];
                    walk(&dir.join(rel), depth + 1, out);
                    continue;
                }
                out.push_str(line);
                out.push('\n');
            }
        }
        if !path.exists() {
            return None;
        }
        let mut out = String::new();
        walk(path, 0, &mut out);
        Some(out)
    }

    /// `#define <name> <val>` is present with a non-zero `<val>` (the FreeRTOS
    /// idiom for an enabled feature). Absent or `0` → false.
    fn freertos_define_is_one(src: &str, name: &str) -> bool {
        src.lines().any(|line| {
            let line = line.trim();
            let Some(rest) = line.strip_prefix("#define") else {
                return false;
            };
            let mut it = rest.split_whitespace();
            it.next() == Some(name) && it.next().and_then(|v| v.parse::<i64>().ok()) == Some(1)
        })
    }

    /// Phase 241.C.2b — for a FreeRTOS board that co-locates its
    /// `config/FreeRTOSConfig.h`, the declared `[board.capabilities]` must AGREE
    /// with the RTOS config it claims to mirror, not silently override it:
    /// `configSUPPORT_DYNAMIC_ALLOCATION` ↔ `heap`, `configUSE_MUTEXES` ↔
    /// `threads`. Catches the #38-class drift (board.toml says heap-capable but
    /// the FreeRTOS config disabled dynamic allocation) at merge time rather than
    /// in an e2e dispatch. (Zephyr's heap/mutex live in per-app Kconfig, not a
    /// board-local file, so they stay config-derived — see 241.C.2b note.)
    #[test]
    fn freertos_capabilities_agree_with_freertosconfig() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("repo root from packages/cli/nros-cli-core")
            .to_path_buf();
        let cat = BoardCatalog::load(&root).expect("load real board catalog");
        let mut checked = 0usize;
        for d in cat.descriptors() {
            if d.platform != PlatformKind::Freertos {
                continue;
            }
            let Some(rel) = d.crate_path_rel() else {
                continue;
            };
            let cfg = root.join(&rel).join("config/FreeRTOSConfig.h");
            // Follows the W5.a relative include into `nros-board-freertos`; a
            // board with no co-located config at all yields `None` and is
            // skipped, exactly as before.
            let Some(src) = read_freertos_config(&cfg) else {
                continue; // board without a co-located config — nothing to cross-check
            };
            let caps = d.capabilities();
            let cfg_heap = freertos_define_is_one(&src, "configSUPPORT_DYNAMIC_ALLOCATION");
            let cfg_threads = freertos_define_is_one(&src, "configUSE_MUTEXES");
            let name = d.names.join("/");
            assert_eq!(
                caps.heap,
                cfg_heap,
                "board `{name}`: [board.capabilities] heap={} but \
                 configSUPPORT_DYNAMIC_ALLOCATION={} in {}",
                caps.heap,
                cfg_heap as u8,
                cfg.display()
            );
            assert_eq!(
                caps.threads,
                cfg_threads,
                "board `{name}`: [board.capabilities] threads={} but \
                 configUSE_MUTEXES={} in {}",
                caps.threads,
                cfg_threads as u8,
                cfg.display()
            );
            checked += 1;
        }
        assert!(
            checked > 0,
            "no FreeRTOS board with a co-located config/FreeRTOSConfig.h was \
             cross-checked — the C.2b agreement guard is vacuous"
        );
    }

    #[test]
    fn resolves_board_by_alias() {
        let cat = catalog();
        let d = cat.resolve("stm32f4", "thumbv7em-none-eabihf").unwrap();
        assert_eq!(d.platform, PlatformKind::Stm32);
        assert_eq!(d.chip.as_deref(), Some("stm32f429"));
        // alias of the same descriptor
        assert_eq!(
            cat.resolve("stm32f429", "thumbv7em-none-eabihf")
                .unwrap()
                .chip
                .as_deref(),
            Some("stm32f429")
        );
    }

    #[test]
    fn multi_board_crate_distinguishes_by_name() {
        let cat = catalog();
        // Same crate, different chip.
        let f407 = cat.resolve("stm32f407", "thumbv7em-none-eabihf").unwrap();
        assert_eq!(f407.chip.as_deref(), Some("stm32f407"));
        assert_eq!(f407.board_crate.as_deref(), Some("nros-board-stm32f4"));
    }

    #[test]
    fn crate_path_defaults_under_packages_boards() {
        let cat = catalog();
        let d = cat.resolve("stm32f4", "thumbv7em-none-eabihf").unwrap();
        assert_eq!(
            d.crate_path_rel().as_deref(),
            Some("packages/boards/nros-board-stm32f4")
        );
    }

    #[test]
    fn cargo_config_substitutes_workspace() {
        let descriptor = BoardDescriptor {
            names: vec!["x".into()],
            platform: PlatformKind::ThreadxRiscv64,
            target: None,
            toolchain: Toolchain::Stable,
            platform_feature: "platform-threadx".into(),
            local_aliases: vec![],
            link_kind: LinkKind::None,
            entry_kind: EntryKind::BoardRun,
            supported_netstacks: Vec::new(),
            chip: None,
            board_crate: None,
            crate_path: None,
            board_features: vec![],
            capability_features: vec![],
            cargo_config: Some("inc = \"${workspace}/third-party/x\"".into()),
            entry: None,
            target_contains: None,
            capabilities: None,
            cmake: None,
            source: None,
        };
        let rendered = descriptor.cargo_config_rendered(Path::new("/ws")).unwrap();
        assert_eq!(rendered, "inc = \"/ws/third-party/x\"");
    }

    /// phase-341 W2 — `PlatformKind::kebab()` must spell each variant exactly
    /// as serde deserializes it, since the deploy→descriptor fallback compares
    /// a leaf's `deploy` token against that spelling. A hand-written match can
    /// drift from `rename_all`; this closes the loop on every variant.
    #[test]
    fn platform_kebab_round_trips() {
        use PlatformKind::*;
        for p in [
            Posix,
            Freertos,
            BareMetal,
            Nuttx,
            Zephyr,
            ThreadxLinux,
            ThreadxRiscv64,
            Esp32,
            Stm32,
            OrinSpe,
        ] {
            let back: PlatformKind = serde_json::from_str(&format!("\"{}\"", p.kebab()))
                .unwrap_or_else(|e| panic!("`{}` does not deserialize back: {e}", p.kebab()));
            assert_eq!(back, p, "kebab() spelling disagrees with serde for {p:?}");
        }
    }

    /// phase-341 W2 — the ambiguity the phase doc names: two NuttX descriptors
    /// share `platform = "nuttx"` (and back the same crate), and are told apart
    /// only by `names`. Resolution must pick the arm board for `deploy =
    /// "nuttx"` and the riscv one for `deploy = "nuttx-riscv"` — a projection
    /// that swapped them would write the wrong triple and link args.
    #[test]
    fn deploy_resolves_the_two_nuttx_boards_apart() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("repo root")
            .to_path_buf();
        let cat = BoardCatalog::load(&root).expect("load real board catalog");
        let arm = match cat.resolve_deploy("nuttx") {
            DeployResolution::Board(d) => d,
            other => panic!("deploy `nuttx` did not resolve: {other:?}"),
        };
        let riscv = match cat.resolve_deploy("nuttx-riscv") {
            DeployResolution::Board(d) => d,
            other => panic!("deploy `nuttx-riscv` did not resolve: {other:?}"),
        };
        let arm_cfg = arm.cargo_config.as_deref().expect("arm board cargo_config");
        let riscv_cfg = riscv
            .cargo_config
            .as_deref()
            .expect("riscv board cargo_config");
        assert!(
            arm_cfg.contains("armv7a-nuttx-eabihf"),
            "`nuttx` resolved to a board whose cargo_config is not the arm one:\n{arm_cfg}"
        );
        assert!(
            riscv_cfg.contains("riscv"),
            "`nuttx-riscv` resolved to a board whose cargo_config is not the riscv one:\n{riscv_cfg}"
        );
        // Both descriptors are read from the same file; the header a projection
        // writes must still name it.
        assert_eq!(
            arm.source.as_deref(),
            Some("packages/boards/nros-board-nuttx-qemu/nros-board.toml")
        );
    }

    /// A token no descriptor claims resolves to `Unknown` — never to "the first
    /// board that looked close". The caller writes nothing for these.
    #[test]
    fn unclaimed_deploy_token_is_unknown() {
        let cat = catalog();
        assert!(matches!(
            cat.resolve_deploy("some-out-of-tree-board"),
            DeployResolution::Unknown
        ));
    }

    /// Two descriptors sharing a `names` entry (`threadx` → riscv64 vs linux)
    /// are AMBIGUOUS without a target to test `target_contains` against. Real
    /// catalog: the pair exists in tree, and no leaf may silently get one.
    #[test]
    fn deploy_shared_by_two_boards_is_ambiguous() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("repo root")
            .to_path_buf();
        let cat = BoardCatalog::load(&root).expect("load real board catalog");
        match cat.resolve_deploy("threadx") {
            DeployResolution::Ambiguous(names) => {
                assert!(
                    names.len() >= 2,
                    "expected several candidates, got {names:?}"
                );
            }
            other => panic!("`threadx` is claimed by two boards, got {other:?}"),
        }
    }

    /// The `platform` fallback — the mapping
    /// `check-board-cargo-config-applied.sh` uses — resolves a token no board
    /// NAMES, and only when a single board declares that platform.
    #[test]
    fn deploy_falls_back_to_platform_when_unique() {
        let cat = catalog();
        // `stm32` is declared by both stm32 descriptors → ambiguous, not a guess.
        assert!(matches!(
            cat.resolve_deploy("stm32"),
            DeployResolution::Ambiguous(_)
        ));
        let mut boards = catalog().descriptors;
        boards.retain(|d| d.names.iter().any(|n| n == "stm32f407"));
        let cat = BoardCatalog::from_descriptors(boards);
        match cat.resolve_deploy("stm32") {
            DeployResolution::Board(d) => assert_eq!(d.chip.as_deref(), Some("stm32f407")),
            other => panic!("unique platform must resolve, got {other:?}"),
        }
    }

    #[test]
    fn unknown_board_on_linux_target_falls_back_to_posix() {
        let mut boards = catalog().descriptors;
        boards.push(BoardDescriptor {
            names: vec!["native".into(), "posix".into()],
            platform: PlatformKind::Posix,
            target: None,
            toolchain: Toolchain::Stable,
            platform_feature: "platform-posix".into(),
            local_aliases: vec![],
            link_kind: LinkKind::None,
            entry_kind: EntryKind::HostedMain,
            supported_netstacks: Vec::new(),
            chip: None,
            board_crate: None,
            crate_path: None,
            board_features: vec![],
            capability_features: vec![],
            cargo_config: None,
            entry: None,
            target_contains: None,
            capabilities: None,
            cmake: None,
            source: None,
        });
        let cat = BoardCatalog::from_descriptors(boards);
        let d = cat
            .resolve("some-unknown", "x86_64-unknown-linux-gnu")
            .unwrap();
        assert_eq!(d.platform, PlatformKind::Posix);
    }
}
