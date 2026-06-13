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
//! boards — `posix`, `zephyr`, `orin-spe` — get a descriptor-only dir under
//! `packages/boards/`). A file holds a `[[board]]` array so one crate can back
//! several boards (e.g. `nros-board-stm32f4` → `stm32f429` + `stm32f407`,
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

/// Who owns NIC + IP bring-up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NetStack {
    /// RTOS brings up the stack (Zephyr / NuttX).
    RtosOwned,
    /// Board crate owns the stack (smoltcp / lwIP / NetX / esp-hal).
    NanorosOwned,
}

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
    pub net_stack: NetStack,
    /// esp-hal / stm32 chip feature; `None` for non-chip platforms.
    #[serde(default)]
    pub chip: Option<String>,
    /// Board crate to depend on; `None` for crate-less host boards
    /// (posix / zephyr / orin-spe) that pull static or `nros-platform-cffi` deps.
    #[serde(default)]
    pub board_crate: Option<String>,
    /// Board-crate path relative to the workspace root; defaults to
    /// `packages/boards/<board_crate>` when omitted.
    #[serde(default)]
    pub crate_path: Option<String>,
    /// Extra features to enable on the board crate dependency.
    #[serde(default)]
    pub board_features: Vec<String>,
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
            descriptors.extend(file.boards);
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
net_stack = "nanoros-owned"
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
net_stack = "nanoros-owned"
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
            let Ok(src) = std::fs::read_to_string(&cfg) else {
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
            net_stack: NetStack::NanorosOwned,
            chip: None,
            board_crate: None,
            crate_path: None,
            board_features: vec![],
            cargo_config: Some("inc = \"${workspace}/third-party/x\"".into()),
            entry: None,
            target_contains: None,
            capabilities: None,
        };
        let rendered = descriptor.cargo_config_rendered(Path::new("/ws")).unwrap();
        assert_eq!(rendered, "inc = \"/ws/third-party/x\"");
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
            net_stack: NetStack::NanorosOwned,
            chip: None,
            board_crate: None,
            crate_path: None,
            board_features: vec![],
            cargo_config: None,
            entry: None,
            target_contains: None,
            capabilities: None,
        });
        let cat = BoardCatalog::from_descriptors(boards);
        let d = cat
            .resolve("some-unknown", "x86_64-unknown-linux-gnu")
            .unwrap();
        assert_eq!(d.platform, PlatformKind::Posix);
    }
}
