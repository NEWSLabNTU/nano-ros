//! Stage 3 — is this build's toolchain present? (phase-383 W2.d, RFC-0065 D2/D14).
//!
//! ## What this checks, and what it deliberately does not
//!
//! A missing prerequisite must fail HERE, naming the `nros setup` line that
//! fixes it — never mid-compile with a cryptic linker error. That is D2's whole
//! promise.
//!
//! It checks what is **cheap and board-specific**: the Rust target the board
//! pins, and the SDK directories its site config names. It does NOT re-walk the
//! whole `nros-sdk-index.toml` the way `nros setup --check` does. Two reasons:
//!
//! * `nros setup --check` prints a report; its predicate is not separable
//!   without refactoring `setup.rs`, and doing that inside a wave about
//!   something else is how unrelated changes ride along;
//! * a full index walk probes every tool for every board, most of which this
//!   build does not need. Preflight should be fast enough that nobody wants a
//!   flag to skip it — a skipped check is not a check.
//!
//! So this is the narrow, high-yield subset, and it says so rather than
//! implying full coverage. `nros doctor` remains the thorough answer.
//!
//! ## Offline (D14)
//!
//! Preflight never fetches. `--offline` changes nothing here — it changes what
//! stage 5 is told — which is the point: a build that would have fetched fails
//! at stage 3 either way, with the manual command named.

use std::path::Path;

use crate::orchestration::board_descriptor::BoardDescriptor;

/// The remedy preflight names for a workspace that has never been
/// configured (RFC-0120 D2: `nros configure`, of which `nros sync` is the
/// retiring alias). One spelling, so `nros build` can tell this row from the
/// toolchain rows it refuses on.
pub const CONFIGURE_REMEDY: &str = "nros configure";

/// A missing prerequisite, with the command that installs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Missing {
    /// What is absent, in the user's vocabulary.
    pub what: String,
    /// The exact command to run. Never a description of one.
    pub remedy: String,
}

/// Check the prerequisites `board` needs, in the workspace at `root`.
///
/// Returns every problem rather than the first: a user fixing three missing
/// things one build at a time is three round trips, and D2's promise is that
/// stage 3 tells you everything before anything compiles.
#[must_use]
pub fn check(board: &BoardDescriptor, root: &Path, nano_ros_root: Option<&Path>) -> Vec<Missing> {
    let mut out = Vec::new();

    // The rustc target triple. A cross board that pins one needs it installed,
    // and `cargo build --target` fails deep in the build otherwise.
    if let Some(target) = board.target.as_deref() {
        // The BOARD can say it builds std itself, and then no prebuilt
        // `rust-std` is consulted whatever the list says. The esp32 board's
        // `cargo_config` carries `[unstable] build-std`, and its workspace row
        // builds under the pinned nightly (`RUSTUP_TOOLCHAIN`), which has no
        // `riscv32imc-unknown-none-elf` added — nor needs one. Asking only
        // `config/rust-targets.txt` (`rustup` for that triple, true of the
        // stable leaves) refused the tier-2 esp32 workspace image with a
        // `rustup target add` it did not need, before anything compiled.
        let provisioning = if board_builds_std(board) {
            Provisioning::BuildStd
        } else {
            target_provisioning(nano_ros_root, target)
        };
        match provisioning {
            // A prebuilt `rust-std` exists, so "is it installed" is the right
            // question and `rustup target add` is the answer.
            Provisioning::Rustup => {
                if !rust_target_installed(target, root) {
                    out.push(Missing {
                        what: format!("Rust target `{target}` (board `{}`)", board.names[0]),
                        remedy: format!("rustup target add {target}"),
                    });
                }
            }
            // Tier 3 / custom JSON: there is NOTHING to install, and this used
            // to say `rustup target add armv7a-nuttx-eabihf` — a command that
            // cannot succeed. `config/rust-targets.txt` says so in as many
            // words: "There is nothing to install, so neither the installer nor
            // the doctor touches these rows."
            //
            // It cost the nightly's `nuttx` and `esp32` cells, which failed
            // preflight before compiling anything, with a remedy that would
            // have failed too. The requirement these targets DO have is the
            // `rust-src` component, because the leaf builds them with
            // `-Zbuild-std`.
            //
            // On the toolchain the board BUILDS with: a `toolchain = "nightly"`
            // board is built by the pinned nightly (Corrosion's configure asks
            // for it by name), so `rust-src` on rustup's default stable answers
            // a question nobody asked — phase-517 W10 measured both halves:
            // preflight refused a host whose nightly HAD `rust-src` because the
            // default did not, and would have passed one whose nightly lacked it.
            Provisioning::BuildStd => {
                let channel = board_rust_channel(board, nano_ros_root);
                if !rust_component_installed("rust-src", channel.as_deref(), root) {
                    let on = channel
                        .as_deref()
                        .map(|c| format!(" --toolchain {c}"))
                        .unwrap_or_default();
                    out.push(Missing {
                        what: format!(
                            "`rust-src` for build-std target `{target}` (board `{}`)",
                            board.names[0]
                        ),
                        remedy: format!("rustup component add rust-src{on}"),
                    });
                }
            }
        }
    }

    // A workspace that has never been synced has no generated message crates,
    // and every leaf `.cargo/config.toml` include points at a tree that does
    // not exist — a cargo manifest-PARSE error four frames deep that never
    // names sync (issue 0463).
    //
    // **Two locations, and probing only one was a defect W8 caught.** `nros
    // sync` writes generated msg crates to `<ws>/generated/` and resolved
    // models to `<ws>/build/nros/`. The first version of this check looked only
    // at `build/nros`, so `nano-ros-rt-eval` — which carries a populated
    // `generated/` with fourteen msg packages — was told it had never been
    // synced. Either output is evidence that sync has run.
    let synced = root.join("build/nros").is_dir() || root.join("generated").is_dir();
    if root.join("src").is_dir() && !synced {
        out.push(Missing {
            what: "generated message bindings (this workspace has never been synced)".to_string(),
            remedy: CONFIGURE_REMEDY.to_string(),
        });
    }

    out
}

/// Issue 2151 — what a RUST image on a Zephyr board needs beyond a C one: the
/// triple zephyr-lang-rust compiles it for (`[board.zephyr] rust_target`), and
/// the host prerequisites its bindgen run needs (`rust_prereqs`, checked by
/// `missing_prereqs` — the `nros setup --check` probe). Without this the build
/// failed inside cargo with `can't find crate for core`, then inside
/// `zephyr-sys`'s build script with `Unable to find libclang`.
///
/// Both remedies are `nros setup`, which installs the target for the board and
/// asks for the prerequisites when the workspace holds a Rust package; the
/// target's own `rustup` line is named too, since it needs no privilege.
#[must_use]
pub fn check_zephyr_rust(
    board: &BoardDescriptor,
    root: &Path,
    missing_prereqs: impl Fn(&[String]) -> Vec<String>,
) -> Vec<Missing> {
    let mut out = Vec::new();
    let Some(z) = board.zephyr.as_ref() else {
        return out;
    };
    if let Some(target) = z.rust_target_for_host()
        && !rust_target_installed(&target, root)
    {
        out.push(Missing {
            what: format!(
                "Rust target `{target}` (a Rust image on board `{}`)",
                board.names[0]
            ),
            remedy: format!("rustup target add {target}"),
        });
    }
    for c in &z.rust_components {
        if !rust_component_installed(c, None, root) {
            out.push(Missing {
                what: format!(
                    "rustup component `{c}` (a Rust image on board `{}`)",
                    board.names[0]
                ),
                remedy: format!("rustup component add {c}"),
            });
        }
    }
    let absent = missing_prereqs(&z.rust_prereqs);
    if !absent.is_empty() {
        out.push(Missing {
            what: format!(
                "host package(s) {} (bindgen in a Rust image on board `{}`)",
                absent.join(", "),
                board.names[0]
            ),
            remedy: "nros setup --sudo".to_string(),
        });
    }
    out
}

/// Issue 2358 — a Rust image on Zephyr needs the zephyr-lang-rust module in
/// the west workspace it builds against. Without it the build ran west anyway
/// and failed in Zephyr's words (`attempt to assign the value 'y' to the
/// undefined symbol RUST`, then `Aborting due to Kconfig warnings`), naming
/// nothing about the module, although `nros setup board` already knew.
///
/// `zephyr_base` is the Zephyr tree the build resolved; its parent is the west
/// workspace. `None` — no workspace resolved — says nothing here: the build
/// reports that itself. `remedy` is the `nros setup board` line, which prints
/// the `projects:` entry that fetches the module.
#[must_use]
pub fn check_zephyr_lang_rust(
    board: &BoardDescriptor,
    zephyr_base: Option<&Path>,
    remedy: &str,
) -> Option<Missing> {
    let ws = zephyr_base?.parent()?;
    let module = crate::builder::zephyr::lang_rust_module_dir(ws);
    if module.join("Kconfig").is_file() {
        return None;
    }
    Some(Missing {
        what: format!(
            "the zephyr-lang-rust module at {} (a Rust image on board `{}`; this west \
             workspace does not fetch it)",
            module.display(),
            board.names[0]
        ),
        remedy: remedy.to_string(),
    })
}

/// What rustup must add for `board`'s cross target — the same rule [`check`]
/// refuses a build by, so `nros setup <board>` installs exactly what that check
/// would ask for (phase-517 W10): a prebuilt `rust-std` for an ordinary
/// triple, the `rust-src` component for a build-std one, nothing for a host
/// board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RustNeed {
    /// `rustup target add <triple>`.
    Target(String),
    /// `rustup component add <name>` — `rust-src`, for `-Zbuild-std`.
    Component(String),
}

#[must_use]
pub fn rust_need(board: &BoardDescriptor, nano_ros_root: Option<&Path>) -> Option<RustNeed> {
    let target = board.target.as_deref()?;
    let provisioning = if board_builds_std(board) {
        Provisioning::BuildStd
    } else {
        target_provisioning(nano_ros_root, target)
    };
    Some(match provisioning {
        Provisioning::Rustup => RustNeed::Target(target.to_string()),
        Provisioning::BuildStd => RustNeed::Component("rust-src".to_string()),
    })
}

/// Whether the board's own cargo config builds `core`/`alloc` from source
/// (`[unstable] build-std`), so its target needs `rust-src` and no prebuilt
/// `rust-std`. An unparseable config answers `false`: the list then decides,
/// which is the behaviour before this existed.
fn board_builds_std(board: &BoardDescriptor) -> bool {
    board
        .cargo_config
        .as_deref()
        .and_then(|c| c.parse::<toml::Table>().ok())
        .and_then(|t| t.get("unstable")?.get("build-std").cloned())
        .is_some_and(|v| v.as_array().is_some_and(|a| !a.is_empty()))
}

/// How `config/rust-targets.txt` says a triple is provided.
///
/// Column 2 of the ONE list (issue 0833). Unknown or unreadable ⇒ `Rustup`,
/// which is the pre-existing behaviour: a triple nobody declared is more likely
/// a normal target than a custom JSON one, and preflight must never refuse a
/// build the compiler would have accepted.
#[derive(Debug, PartialEq, Eq)]
enum Provisioning {
    Rustup,
    BuildStd,
}

fn target_provisioning(nano_ros_root: Option<&Path>, target: &str) -> Provisioning {
    // The NANO-ROS CHECKOUT, not the workspace root — `config/rust-targets.txt`
    // lives in this repository, and a user's workspace has no such file.
    //
    // The first version of this read it from the workspace root, which is what
    // `preflight::check` is otherwise given. It therefore found nothing, fell
    // back to `Rustup`, and changed NOTHING: the nightly's `nuttx` cell failed
    // on the identical `rustup target add armv7a-nuttx-eabihf` line after the
    // "fix" landed. The local check that was supposed to catch that passed for
    // an unrelated reason, so the wrong root shipped.
    let Some(repo) = nano_ros_root else {
        return Provisioning::Rustup;
    };
    let Ok(text) = std::fs::read_to_string(repo.join("config/rust-targets.txt")) else {
        return Provisioning::Rustup;
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut cols = line.split_whitespace();
        if cols.next() == Some(target) {
            return match cols.next() {
                Some("build-std") => Provisioning::BuildStd,
                _ => Provisioning::Rustup,
            };
        }
    }
    Provisioning::Rustup
}

/// The pinned nightly channel a `toolchain = "nightly"` board builds with —
/// the `nightly-*` row of `config/rust-toolchains.txt`, the generated listing
/// of every `rust-toolchain.toml` in the tree (the NuttX FFI crates' among
/// them, which is what pins Corrosion's configure). Read from that listing
/// because it is what an INSTALLED SDK root ships; `tools/` is not staged.
/// `None` when unreadable: the check then asks rustup's default, as before.
pub(crate) fn pinned_nightly(nano_ros_root: Option<&Path>) -> Option<String> {
    let text = std::fs::read_to_string(nano_ros_root?.join("config/rust-toolchains.txt")).ok()?;
    text.lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_whitespace().next())
        .find(|c| c.starts_with("nightly-"))
        .map(str::to_string)
}

/// The Rust channel cargo must build `board` with: the pinned nightly for a
/// `toolchain = "nightly"` board (its `cargo_config` carries `[unstable]
/// build-std`, which a stable cargo ignores — and then fails with `can't find
/// crate for core`), else `None` (whatever rustup picks). Issue 2149: the
/// preflight asked this channel for `rust-src` while the build itself ran on
/// the default, so a NuttX Rust image from `nros build` could not compile.
#[must_use]
pub(crate) fn board_rust_channel(
    board: &BoardDescriptor,
    nano_ros_root: Option<&Path>,
) -> Option<String> {
    (board.toolchain == crate::orchestration::board_descriptor::Toolchain::Nightly)
        .then(|| pinned_nightly(nano_ros_root))
        .flatten()
}

/// A `rustup` command that answers for the toolchain cargo will BUILD with in
/// `root` (issue 2288).
///
/// rustup picks its toolchain from the CURRENT DIRECTORY: `RUSTUP_TOOLCHAIN`,
/// then the nearest `rust-toolchain.toml` above the cwd, then the default. The
/// build's cargo runs in the image's workspace, so that is the directory to ask
/// from. A bare `rustup` asked the caller's cwd instead, and the Zephyr fixture
/// driver runs `nros build` from the west workspace, OUTSIDE every checkout:
/// there rustup answered for the default `stable` (minimal profile, no
/// `x86_64-unknown-none`, no clippy) while the build would use the pinned
/// `1.99.0`, which has both. The tier-2 nightly refused three Rust native_sim
/// entries on that false answer.
pub(crate) fn rustup_in(root: &Path) -> std::process::Command {
    let mut c = std::process::Command::new("rustup");
    if root.is_dir() {
        c.current_dir(root);
    }
    c
}

/// Whether `rustup` reports a COMPONENT as installed (on `toolchain`, or else
/// on the toolchain `root` resolves to -- [`rustup_in`]).
///
/// Same false-negative-is-safe rule as [`rust_target_installed`]: no rustup ⇒
/// report installed and let the build speak.
fn rust_component_installed(component: &str, toolchain: Option<&str>, root: &Path) -> bool {
    let mut args = vec!["component", "list", "--installed"];
    if let Some(t) = toolchain {
        args.extend(["--toolchain", t]);
    }
    let Ok(out) = rustup_in(root).args(&args).output() else {
        return true;
    };
    if !out.status.success() {
        return true;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|l| l.trim() == component || l.trim().starts_with(&format!("{component}-")))
}

/// Whether `rustup` reports `target` as installed.
///
/// A false NEGATIVE is the safe direction: with no rustup (a distro toolchain,
/// a nix shell), this reports installed and lets the build speak for itself.
/// Preflight exists to give a better message than the compiler, never to refuse
/// a build the compiler would have accepted.
/// The installed-target list, probed ONCE per process and directory.
///
/// `None` means the probe could not answer -- no `rustup` on PATH, or it
/// failed. Both callers then report the target as installed, because inventing
/// a missing target is the worse error.
///
/// Probed once because the two failure modes are INDISTINGUISHABLE at the call
/// site and only one of them is stable. "no rustup here" gives the same `Err`
/// as a spawn that failed under load, and under an 80-way parallel build the
/// second happens intermittently -- so two calls in one process could disagree,
/// and `check()` would report a target as present that a caller had just
/// measured as absent. Caching does not make the probe more accurate; it makes
/// the process SELF-CONSISTENT, which is what a caller comparing two answers
/// actually needs. Issue 0726's class.
///
/// Keyed by the directory asked from (issue 2288): two workspaces in one
/// process may pin different toolchains, and each answer is about its own.
fn installed_rust_targets(root: &Path) -> Option<Vec<String>> {
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<std::path::PathBuf, Option<Vec<String>>>>,
    > = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let mut map = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    map.entry(root.to_path_buf())
        .or_insert_with(|| {
            let out = rustup_in(root)
                .args(["target", "list", "--installed"])
                .output()
                .ok()?;
            if !out.status.success() {
                return None;
            }
            Some(
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .map(|l| l.trim().to_string())
                    .collect(),
            )
        })
        .clone()
}

fn rust_target_installed(target: &str, root: &Path) -> bool {
    match installed_rust_targets(root) {
        // Cannot answer -- see above.
        None => true,
        Some(list) => list.iter().any(|l| l == target),
    }
}

/// Issue 2312 — whether the C/C++ runtime (`nros-c` / `nros-cpp`) for `triple`
/// builds `core`/`alloc` from source on the pinned nightly: the embedded
/// triples (`armv7a*`, `thumbv*`, `riscv32*`). The SAME rule as cmake's
/// `nros_rust_target_builds_std` (`cmake/NanoRosRustToolchain.cmake`), which
/// picks the channel the Zephyr C road builds with.
#[must_use]
pub fn runtime_builds_std(triple: &str) -> bool {
    ["armv7a", "thumbv", "riscv32"]
        .iter()
        .any(|p| triple.starts_with(p))
}

/// Issue 2312 — the components the Zephyr C road's nightly needs: `cargo`
/// (the issue's partial install had a toolchain without it, and `rustup
/// component add cargo` repairs one), `rust-src` for `-Z build-std`, and
/// `rustfmt`, which the FFI crate's generated `rust-toolchain.toml`
/// (`zephyr/cmake/nros_generate_interfaces.cmake`) names, so a toolchain
/// without it is auto-installed by rustup.
pub const ZEPHYR_RUNTIME_COMPONENTS: [&str; 3] = ["cargo", "rust-src", "rustfmt"];

/// Issue 2312 — what a Zephyr board's C/C++ runtime needs on the pinned
/// nightly: the channel, then [`ZEPHYR_RUNTIME_COMPONENTS`] and every board
/// triple ([`runtime_builds_std`]) as needs on it.
///
/// A C image on a Cortex-M Zephyr board still builds `nros-c` with cargo, on
/// the pinned nightly with `-Z build-std` (`zephyr/cmake/nros_cargo_build.cmake`).
/// Nothing installed that channel: the descriptor says `toolchain = "stable"`
/// and the nightly was a cmake literal, so rustup's auto-install was the only
/// provisioner, and a partial one stopped the build inside a cmake custom
/// command. `None` for a non-Zephyr board, one whose triples are all host
/// ones (native_sim), or when the pin cannot be read.
#[must_use]
pub fn zephyr_runtime_needs(
    board: &BoardDescriptor,
    nano_ros_root: Option<&Path>,
) -> Option<(String, Vec<RustNeed>)> {
    let z = board.zephyr.as_ref()?;
    let mut triples: Vec<String> = board
        .provisioning
        .as_ref()
        .map(|p| p.rust_targets.clone())
        .unwrap_or_default();
    triples.extend(z.rust_target_for_host());
    triples.retain(|t| runtime_builds_std(t));
    let mut seen = std::collections::BTreeSet::new();
    triples.retain(|t| seen.insert(t.clone()));
    if triples.is_empty() {
        return None;
    }
    let channel = pinned_nightly(nano_ros_root)?;
    let needs = ZEPHYR_RUNTIME_COMPONENTS
        .iter()
        .map(|c| RustNeed::Component((*c).to_string()))
        .chain(triples.into_iter().map(RustNeed::Target))
        .collect();
    Some((channel, needs))
}

/// One question [`check_zephyr_runtime_with`] asks rustup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RustupQuery {
    /// Is the toolchain installed at all?
    Toolchain(String),
    /// Is the component installed on the toolchain? `cargo` is asked this way.
    Component { toolchain: String, name: String },
    /// Is the target installed on the toolchain?
    Target { toolchain: String, triple: String },
}

/// The real rustup answer to a [`RustupQuery`], asked from `root`. `None`: no
/// rustup, or it could not answer, which the check reads as installed (the
/// false-negative-is-safe rule of [`rust_target_installed`]).
fn rustup_answers(q: &RustupQuery, root: &Path) -> Option<bool> {
    let lines = |args: &[&str]| -> Option<Vec<String>> {
        let out = rustup_in(root).args(args).output().ok()?;
        out.status.success().then(|| {
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(|l| l.trim().to_string())
                .collect()
        })
    };
    match q {
        RustupQuery::Toolchain(t) => {
            let list = lines(&["toolchain", "list"])?;
            Some(list.iter().any(|l| {
                l.split_whitespace()
                    .next()
                    .is_some_and(|name| name == t || name.starts_with(&format!("{t}-")))
            }))
        }
        RustupQuery::Component { toolchain, name } => {
            let list = lines(&["component", "list", "--installed", "--toolchain", toolchain])?;
            Some(
                list.iter()
                    .any(|l| l == name || l.starts_with(&format!("{name}-"))),
            )
        }
        RustupQuery::Target { toolchain, triple } => {
            let list = lines(&["target", "list", "--installed", "--toolchain", toolchain])?;
            Some(list.iter().any(|l| l == triple))
        }
    }
}

/// Issue 2312 — refuse a C/C++ image on a Zephyr board BEFORE cmake when the
/// pinned nightly its runtime builds with ([`zephyr_runtime_needs`]) is
/// absent, lacks `cargo`, or lacks a component or target. `remedy` is the
/// `nros setup board` line that installs all of it. Asks rustup from `root`.
#[must_use]
pub fn check_zephyr_runtime(
    board: &BoardDescriptor,
    root: &Path,
    nano_ros_root: Option<&Path>,
    remedy: &str,
) -> Vec<Missing> {
    check_zephyr_runtime_with(board, nano_ros_root, remedy, |q| rustup_answers(q, root))
}

/// [`check_zephyr_runtime`] with rustup's answers passed in, so the rule is
/// testable without a rustup or a toolchain on the host.
pub fn check_zephyr_runtime_with(
    board: &BoardDescriptor,
    nano_ros_root: Option<&Path>,
    remedy: &str,
    answers: impl Fn(&RustupQuery) -> Option<bool>,
) -> Vec<Missing> {
    let Some((channel, needs)) = zephyr_runtime_needs(board, nano_ros_root) else {
        return Vec::new();
    };
    let name = &board.names[0];
    let absent = |q: RustupQuery| answers(&q) == Some(false);
    let miss = |what: String| Missing {
        what,
        remedy: remedy.to_string(),
    };
    if absent(RustupQuery::Toolchain(channel.clone())) {
        return vec![miss(format!(
            "the pinned nightly `{channel}` that the C/C++ runtime on board `{name}` builds \
             with (`-Z build-std`)"
        ))];
    }
    let mut out = Vec::new();
    for n in &needs {
        let (query, what) = match n {
            RustNeed::Component(c) => (
                RustupQuery::Component {
                    toolchain: channel.clone(),
                    name: c.clone(),
                },
                format!("rustup component `{c}`"),
            ),
            RustNeed::Target(t) => (
                RustupQuery::Target {
                    toolchain: channel.clone(),
                    triple: t.clone(),
                },
                format!("Rust target `{t}`"),
            ),
        };
        if absent(query) {
            out.push(miss(format!(
                "{what} on `{channel}` (the C/C++ runtime on board `{name}`)"
            )));
        }
    }
    out
}

/// Stage 3's refusal, as a typed error (phase-530 W1).
///
/// Its `Display` is exactly [`report`]. Typed so `nros configure` can tell
/// "this host cannot build that image" (skip it, when the caller did not name
/// it) from every other planning failure, by type rather than by message text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflightRefusal(pub Vec<Missing>);

impl PreflightRefusal {
    /// One line: each missing item with its remedy, for a skip notice.
    #[must_use]
    pub fn summary(&self) -> String {
        self.0
            .iter()
            .map(|m| format!("missing {} (run: {})", m.what, m.remedy))
            .collect::<Vec<_>>()
            .join("; ")
    }
}

impl std::fmt::Display for PreflightRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&report(&self.0))
    }
}

impl std::error::Error for PreflightRefusal {}

/// Render the problems as the message stage 3 fails with.
#[must_use]
pub fn report(missing: &[Missing]) -> String {
    let mut s = String::from("missing prerequisites for this build:\n");
    for m in missing {
        s.push_str(&format!("  - {}\n      run: {}\n", m.what, m.remedy));
    }
    s.push_str(
        "\nnothing was built. `nros doctor` checks the whole toolchain; this \
         list is only what THIS image needs.",
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::board_descriptor::BoardCatalog;

    #[derive(serde::Deserialize)]
    struct BoardFile {
        #[serde(rename = "board")]
        boards: Vec<BoardDescriptor>,
    }

    fn board(extra: &str) -> BoardDescriptor {
        let src = format!(
            "[[board]]\nnames = [\"testboard\"]\nplatform = \"freertos\"\n\
             toolchain = \"stable\"\nplatform_feature = \"platform-freertos\"\n\
             link_kind = \"none\"\nentry_kind = \"board-run\"\n{extra}"
        );
        let f: BoardFile = toml::from_str(&src).expect("parse");
        BoardCatalog::from_descriptors(f.boards)
            .descriptors()
            .first()
            .cloned()
            .expect("one board")
    }

    /// A board whose cargo config builds std is a build-std target whatever
    /// `config/rust-targets.txt` says about its triple — the esp32 workspace
    /// image, built under a nightly with no `riscv32imc` installed.
    #[test]
    fn a_board_that_builds_std_needs_no_prebuilt_target() {
        let b = board(
            "target = \"riscv32imc-unknown-none-elf\"\n\
             cargo_config = \"\"\"\n[unstable]\nbuild-std = [\"core\", \"alloc\"]\n\"\"\"\n",
        );
        assert!(board_builds_std(&b));
        let tmp = tempfile::tempdir().unwrap();
        let m = check(&b, tmp.path(), None);
        assert!(
            !m.iter().any(|m| m.remedy.starts_with("rustup target add")),
            "{m:?}"
        );
        // and a board without it is not mistaken for one
        assert!(!board_builds_std(&board(
            "cargo_config = \"\"\"\n[build]\ntarget = \"x\"\n\"\"\"\n"
        )));
        assert!(!board_builds_std(&board("")));
    }

    #[test]
    fn a_board_with_no_pinned_target_needs_no_rust_target() {
        let tmp = tempfile::tempdir().unwrap();
        let m = check(&board(""), tmp.path(), None);
        assert!(!m.iter().any(|m| m.what.contains("Rust target")), "{m:?}");
    }

    /// A `build-std` target must NOT be reported as an uninstallable rustup one.
    ///
    /// `config/rust-targets.txt` marks `armv7a-nuttx-eabihf` and
    /// `riscv32imac-unknown-nuttx-elf` as `build-std`: Tier 3, no prebuilt
    /// `rust-std`, provided by `-Zbuild-std` from the leaf. Preflight demanded
    /// `rustup target add` for them anyway — a prerequisite that can never be
    /// satisfied — and the nightly's `nuttx` and `esp32` cells died at stage 3
    /// with a remedy that would also have failed.
    ///
    /// Reads the REAL list rather than a fixture: the point of issue 0833's one
    /// list is that a second copy drifts, and a test asserting against its own
    /// private copy would be exactly that.
    #[test]
    fn the_provisioning_column_decides_which_prerequisite_is_checked() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find(|p| p.join("config/rust-targets.txt").is_file())
            .expect("the repo's rust-targets list");

        assert_eq!(
            target_provisioning(Some(root), "armv7a-nuttx-eabihf"),
            Provisioning::BuildStd,
            "a NuttX target is build-std; demanding `rustup target add` for it \
             names a command that cannot succeed"
        );
        assert_eq!(
            target_provisioning(Some(root), "thumbv7m-none-eabi"),
            Provisioning::Rustup,
            "a target with a prebuilt rust-std keeps the rustup check"
        );
        // A triple nobody declared falls back to the pre-existing behaviour.
        assert_eq!(
            target_provisioning(Some(root), "nros-not-a-declared-triple"),
            Provisioning::Rustup
        );
    }

    #[test]
    fn an_uninstalled_target_is_reported_with_the_rustup_line() {
        let tmp = tempfile::tempdir().unwrap();
        // A triple no host has installed, and which is not a real target — so
        // this cannot pass by accident on a well-provisioned machine.
        let b = board("target = \"nros-not-a-real-triple\"\n");
        // Probe ONCE, before `check`, and branch on that one answer.
        //
        // `rust_target_installed` returns TRUE when the fork fails (`:186`,
        // `:189`) -- "no rustup here, so report installed". Asking twice means
        // the two answers can disagree under load: `check`'s call forks fine and
        // reports the missing target, this one fails to spawn and takes the
        // "nothing to assert" branch, and the test fails on a tree that is
        // correct. Observed in `check::build`'s parallel lane; passes solo and
        // twice in a row standalone.
        //
        // Issue 0726's class exactly: a forked process that failed to START
        // under a fan-out, reported as a finding about the source tree.
        let installed = rust_target_installed("nros-not-a-real-triple", tmp.path());
        let m = check(&b, tmp.path(), None);
        if installed {
            // No rustup on this host: the check reports installed by design
            // (see `rust_target_installed`), so there is nothing to assert.
            assert!(m.iter().all(|m| !m.what.contains("Rust target")));
            return;
        }
        let hit = m
            .iter()
            .find(|m| m.what.contains("Rust target"))
            .expect("must report the missing target");
        assert_eq!(hit.remedy, "rustup target add nros-not-a-real-triple");
    }

    /// A `build-std` board is not told to `rustup target add` — through
    /// `check()`, with the REAL checkout.
    ///
    /// The unit test on `target_provisioning` alone was not enough, and this is
    /// the test that would have caught what shipped: the classifier was right
    /// and the ROOT handed to it was wrong. `check()` receives the WORKSPACE
    /// root, `config/rust-targets.txt` lives in the nano-ros CHECKOUT, so the
    /// lookup found nothing, fell back to `Rustup`, and the nightly's `nuttx`
    /// cell failed on the same `rustup target add armv7a-nuttx-eabihf` line the
    /// fix was supposed to remove.
    ///
    /// I "verified" that version with a `nros build --dry-run` that passed for
    /// an unrelated reason. A passing command is not a passing assertion.
    #[test]
    fn a_build_std_board_is_never_told_to_rustup_target_add() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find(|p| p.join("config/rust-targets.txt").is_file())
            .expect("the repo's rust-targets list");
        let ws = tempfile::tempdir().unwrap();
        let b = board("target = \"armv7a-nuttx-eabihf\"\n");

        let m = check(&b, ws.path(), Some(repo));
        assert!(
            !m.iter().any(|m| m.remedy.starts_with("rustup target add")),
            "a build-std target has nothing to install; reported: {:?}",
            m.iter().map(|m| &m.remedy).collect::<Vec<_>>()
        );

        // ...and with NO checkout to read the list from, the old conservative
        // behaviour stands rather than a silent pass.
        let blind = check(&b, ws.path(), None);
        if !rust_target_installed("armv7a-nuttx-eabihf", ws.path()) {
            assert!(
                blind
                    .iter()
                    .any(|m| m.remedy.starts_with("rustup target add")),
                "without the list this cannot be classified, and preflight must \
                 not invent a pass"
            );
        }
    }

    #[test]
    fn an_unsynced_workspace_is_told_to_sync() {
        // issue 0463 — without this the failure is a cargo manifest-PARSE error
        // four frames deep that never names `nros configure`.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src/talker_pkg")).unwrap();
        let m = check(&board(""), tmp.path(), None);
        let hit = m
            .iter()
            .find(|m| m.remedy == CONFIGURE_REMEDY)
            .expect("must name configure");
        assert_eq!(hit.remedy, "nros configure", "{hit:?}");
        assert!(hit.what.contains("never been synced"), "{hit:?}");
    }

    #[test]
    fn a_synced_workspace_is_not_flagged() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src/talker_pkg")).unwrap();
        std::fs::create_dir_all(tmp.path().join("build/nros")).unwrap();
        let m = check(&board(""), tmp.path(), None);
        assert!(m.iter().all(|m| m.remedy != CONFIGURE_REMEDY), "{m:?}");
    }

    #[test]
    fn generated_msg_crates_also_count_as_synced() {
        // phase-383 W8.a — `nano-ros-rt-eval` carries a populated `generated/`
        // with fourteen msg packages and no `build/nros`. Probing only the
        // latter told a fully synced workspace it had never been synced.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("src/actuator_pkg")).unwrap();
        std::fs::create_dir_all(tmp.path().join("generated/std_msgs")).unwrap();
        let m = check(&board(""), tmp.path(), None);
        assert!(
            m.iter().all(|m| m.remedy != CONFIGURE_REMEDY),
            "generated/ is sync output too: {m:?}"
        );
    }

    #[test]
    fn the_report_names_every_problem_and_the_exact_command() {
        // Every problem, not the first: three missing things fixed one build at
        // a time is three round trips.
        let missing = vec![
            Missing {
                what: "a".to_string(),
                remedy: "nros setup a".to_string(),
            },
            Missing {
                what: "b".to_string(),
                remedy: "nros setup b".to_string(),
            },
        ];
        let r = report(&missing);
        assert!(r.contains("nros setup a"), "{r}");
        assert!(r.contains("nros setup b"), "{r}");
        assert!(r.contains("nothing was built"), "{r}");
    }

    /// Issue 2149 — a `toolchain = "nightly"` board builds on the pinned
    /// nightly (what the cargo handoffs set as `RUSTUP_TOOLCHAIN`), a stable
    /// board names no channel. Read against the in-tree catalog and the
    /// shipped `config/rust-toolchains.txt`.
    #[test]
    fn a_nightly_board_builds_on_the_pinned_nightly() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let cat = BoardCatalog::load(&root).expect("catalog");
        let nuttx =
            crate::cmd::board_facts::resolve_board(&cat, "qemu-armv7a-nuttx").expect("nuttx board");
        let channel = board_rust_channel(nuttx, Some(&root)).expect("a nightly channel");
        assert!(channel.starts_with("nightly-"), "{channel}");
        assert_eq!(Some(channel), pinned_nightly(Some(&root)));
        let freertos = crate::cmd::board_facts::resolve_board(&cat, "mps2-an385-freertos")
            .expect("freertos board");
        assert_eq!(board_rust_channel(freertos, Some(&root)), None);
    }

    /// Issue 2312 — the Nucleo's C/C++ runtime builds on the pinned nightly
    /// (the row the cmake road reads), so that is what its needs name: the
    /// toolchain with cargo, rust-src and rustfmt, and the board's triple on
    /// it. native_sim's runtime compiles for the host and needs none.
    #[test]
    fn a_cortex_m_zephyr_board_needs_the_pinned_nightly_for_its_runtime() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let cat = BoardCatalog::load(&root).expect("catalog");
        let nucleo = crate::cmd::board_facts::resolve_board(&cat, "nucleo-f767zi-zephyr")
            .expect("nucleo board");
        let (channel, needs) = zephyr_runtime_needs(nucleo, Some(&root)).expect("nightly needs");
        assert_eq!(Some(channel.clone()), pinned_nightly(Some(&root)));
        assert!(channel.starts_with("nightly-"), "{channel}");
        assert_eq!(
            needs,
            vec![
                RustNeed::Component("cargo".into()),
                RustNeed::Component("rust-src".into()),
                RustNeed::Component("rustfmt".into()),
                RustNeed::Target("thumbv7em-none-eabi".into()),
            ]
        );
        let native_sim =
            crate::cmd::board_facts::resolve_board(&cat, "zephyr").expect("native_sim board");
        assert_eq!(zephyr_runtime_needs(native_sim, Some(&root)), None);
        // Not a Zephyr board at all.
        let freertos = crate::cmd::board_facts::resolve_board(&cat, "mps2-an385-freertos")
            .expect("freertos board");
        assert_eq!(zephyr_runtime_needs(freertos, Some(&root)), None);
        for (t, want) in [
            ("thumbv7em-none-eabihf", true),
            ("armv7a-none-eabi", true),
            ("riscv32imc-unknown-none-elf", true),
            ("x86_64-unknown-none", false),
            ("i686-unknown-linux-gnu", false),
        ] {
            assert_eq!(runtime_builds_std(t), want, "{t}");
        }
    }

    /// Issue 2312 — the build refuses before cmake, naming `nros setup board`,
    /// for each shape of a missing nightly: absent, present without cargo (the
    /// partial install the issue's reader had), or without a component or the
    /// triple. A rustup that cannot answer refuses nothing.
    #[test]
    fn a_missing_runtime_nightly_is_refused_naming_setup_board() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let cat = BoardCatalog::load(&root).expect("catalog");
        let nucleo = crate::cmd::board_facts::resolve_board(&cat, "nucleo-f767zi-zephyr")
            .expect("nucleo board");
        let channel = pinned_nightly(Some(&root)).unwrap();
        let remedy = "nros setup board nucleo-f767zi-zephyr --zephyr-workspace /ws";
        let check = |answers: &dyn Fn(&RustupQuery) -> Option<bool>| {
            check_zephyr_runtime_with(nucleo, Some(&root), remedy, answers)
        };

        let none = check(&|q| Some(!matches!(q, RustupQuery::Toolchain(_))));
        assert_eq!(none.len(), 1, "{none:?}");
        assert!(none[0].what.contains(&channel), "{}", none[0].what);
        assert_eq!(none[0].remedy, remedy);

        let no_cargo =
            check(&|q| Some(!matches!(q, RustupQuery::Component { name, .. } if name == "cargo")));
        assert_eq!(no_cargo.len(), 1, "{no_cargo:?}");
        assert!(no_cargo[0].what.contains("`cargo`"), "{}", no_cargo[0].what);

        let no_src_no_target = check(&|q| match q {
            RustupQuery::Component { name, .. } => Some(name != "rust-src"),
            RustupQuery::Target { .. } => Some(false),
            RustupQuery::Toolchain(_) => Some(true),
        });
        let whats: Vec<&str> = no_src_no_target.iter().map(|m| m.what.as_str()).collect();
        assert_eq!(whats.len(), 2, "{whats:?}");
        assert!(whats[0].contains("`rust-src`"), "{whats:?}");
        assert!(whats[1].contains("`thumbv7em-none-eabi`"), "{whats:?}");

        assert!(check(&|_| Some(true)).is_empty());
        assert!(
            check(&|_| None).is_empty(),
            "no rustup answer refuses nothing"
        );

        // A C image on native_sim builds its runtime for the host: nothing asked.
        let native_sim =
            crate::cmd::board_facts::resolve_board(&cat, "zephyr").expect("native_sim board");
        assert!(
            check_zephyr_runtime_with(native_sim, Some(&root), remedy, |_| Some(false)).is_empty()
        );
    }

    /// Issue 2151 — a Rust image on a Zephyr board is refused before anything
    /// compiles when the triple zephyr-lang-rust picks, or the host packages
    /// its bindgen needs, are absent; and a board stating neither adds nothing.
    #[test]
    fn a_rust_zephyr_image_names_its_target_and_host_packages() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let catalog = crate::orchestration::board_descriptor::BoardCatalog::load(&root).unwrap();
        let mut board = catalog
            .descriptors()
            .iter()
            .find(|d| d.answers_to("zephyr"))
            .expect("the native_sim descriptor")
            .clone();
        let z = board.zephyr.as_ref().unwrap();
        assert_eq!(
            z.rust_target_for_host(),
            Some(format!("{}-unknown-none", std::env::consts::ARCH)),
            "native_sim's Rust triple follows the host"
        );
        assert_eq!(z.rust_prereqs, vec!["clang", "libclang-dev"]);

        assert_eq!(z.rust_components, vec!["clippy"]);
        let z = board.zephyr.as_mut().unwrap();
        z.rust_target = Some("nros-not-a-real-triple".into());
        z.rust_components = vec!["nros-not-a-real-component".into()];
        let got = check_zephyr_rust(&board, &root, |keys| keys.to_vec());
        assert_eq!(got.len(), 3, "{got:?}");
        assert_eq!(got[0].remedy, "rustup target add nros-not-a-real-triple");
        assert_eq!(
            got[1].remedy,
            "rustup component add nros-not-a-real-component"
        );
        assert!(
            got[2].what.contains("clang, libclang-dev"),
            "{}",
            got[2].what
        );
        assert_eq!(got[2].remedy, "nros setup --sudo");

        let z = board.zephyr.as_mut().unwrap();
        z.rust_target = None;
        z.rust_prereqs.clear();
        z.rust_components.clear();
        assert!(check_zephyr_rust(&board, &root, |keys| keys.to_vec()).is_empty());
    }

    /// Issue 2358 -- a west workspace without zephyr-lang-rust refuses a Rust
    /// image by naming the module; one that has it, or no resolved workspace,
    /// adds nothing.
    #[test]
    fn a_workspace_without_zephyr_lang_rust_is_refused_by_name() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let board = crate::orchestration::board_descriptor::BoardCatalog::load(&root)
            .expect("catalog")
            .descriptors()
            .iter()
            .find(|d| d.answers_to("zephyr"))
            .expect("the native_sim descriptor")
            .clone();
        let ws = tempfile::tempdir().unwrap();
        let zbase = ws.path().join("zephyr");
        std::fs::create_dir_all(&zbase).unwrap();
        let got = check_zephyr_lang_rust(&board, Some(&zbase), "nros setup board zephyr")
            .expect("refused without the module");
        assert!(got.what.contains("zephyr-lang-rust"), "{}", got.what);
        assert!(got.what.contains("modules/lang/rust"), "{}", got.what);
        assert_eq!(got.remedy, "nros setup board zephyr");

        // Negative controls: the module present, and no workspace at all.
        let module = ws.path().join("modules/lang/rust");
        std::fs::create_dir_all(&module).unwrap();
        std::fs::write(module.join("Kconfig"), "").unwrap();
        assert!(check_zephyr_lang_rust(&board, Some(&zbase), "x").is_none());
        assert!(check_zephyr_lang_rust(&board, None, "x").is_none());
    }

    /// Issue 2288 -- every rustup probe asks from the image's WORKSPACE, where
    /// cargo resolves its `rust-toolchain.toml`, never from the caller's cwd.
    ///
    /// Asserted on the command rather than on rustup's answer: under `cargo
    /// test` rustup's proxy exports `RUSTUP_TOOLCHAIN` to this process, which
    /// outranks the directory, so an answer-level test would pass with or
    /// without the fix. The runner's `nros` is not under that proxy, which is
    /// why the nightly saw the default toolchain.
    #[test]
    fn rustup_is_asked_from_the_workspace_it_builds() {
        let ws = tempfile::tempdir().unwrap();
        assert_eq!(rustup_in(ws.path()).get_current_dir(), Some(ws.path()));
        // A root that does not exist (a plan for a workspace not yet created)
        // keeps the caller's cwd rather than failing to spawn.
        let gone = ws.path().join("absent");
        assert_eq!(rustup_in(&gone).get_current_dir(), None);
    }
}
