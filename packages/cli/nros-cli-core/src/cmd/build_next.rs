//! phase-517 W6 — `nros build --board <b>`, and what to type after a build.
//!
//! Two halves, both small and both kept OUT of `cmd::build`'s planner:
//!
//! * **Selection** ([`images_for_board`]): a board word picks the image(s)
//!   whose `[image.*] board` it names — equal (case-insensitive), resolving to
//!   the same descriptor, or an index key the image's descriptor provisions
//!   through (W3's [`BoardDescriptor::setup_key`]). Image ids stay the unit
//!   (RFC-0065): one match is exactly `nros build <that image>`, several or
//!   none is a refusal that names them.
//! * **The next command** ([`next_steps`]): after a SUCCESSFUL build, the
//!   platform-native command that runs or flashes what was built, with real
//!   paths. One function from (plan, board descriptor) to an artifact path and
//!   command lines; it RUNS nothing. The artifact's location comes from
//!   [`BuildOutput`], which the planner fills from the same values it handed
//!   the native tool — never a second derivation here.

use std::path::{Path, PathBuf};

use eyre::{Result, WrapErr};

use crate::{
    builder::plan,
    orchestration::{
        board_descriptor::{
            BoardCatalog, BoardDescriptor, DeployResolution, PlatformKind, triple_of_build_target,
        },
        qemu_bin::QemuBin,
    },
};

/// Where a build put its artifact — recorded at plan time from the values the
/// native command was given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildOutput {
    /// `<target_dir>[/<triple>]/<profile_dir>/<bin>`, cargo's own layout.
    Cargo {
        /// The `--target-dir` the build used (the image's, or the caller's).
        target_dir: PathBuf,
        /// The triple cargo names its target-dir component after (a JSON
        /// spec's file stem); `None` for a host build.
        triple: Option<String>,
        /// `debug`, `release`, or a custom profile's name.
        profile_dir: String,
        /// The one `[[bin]]` the manifest builds; `None` when it builds none
        /// or several.
        bin: Option<String>,
    },
    /// An executable in a cmake binary dir.
    CMake {
        /// Absolute cmake binary dir.
        build_dir: PathBuf,
        /// The executable's path RELATIVE to `build_dir`; `None` when the
        /// project's own CMakeLists names it and it could not be read.
        executable: Option<String>,
    },
    /// A west build dir (`west build -d`).
    West {
        /// Absolute; `None` when the caller passed its own `-d` that could not
        /// be read back.
        build_dir: Option<PathBuf>,
    },
}

impl BuildOutput {
    /// The built file, when the output names one.
    #[must_use]
    pub fn artifact(&self) -> Option<PathBuf> {
        match self {
            BuildOutput::Cargo {
                target_dir,
                triple,
                profile_dir,
                bin,
            } => {
                let mut p = target_dir.clone();
                if let Some(t) = triple {
                    p.push(t);
                }
                p.push(profile_dir);
                Some(p.join(bin.as_ref()?))
            }
            BuildOutput::CMake {
                build_dir,
                executable,
            } => Some(build_dir.join(executable.as_ref()?)),
            BuildOutput::West { build_dir } => {
                let z = build_dir.as_ref()?.join("zephyr");
                // native_sim links a host executable beside the ELF.
                let exe = z.join("zephyr.exe");
                Some(if exe.is_file() {
                    exe
                } else {
                    z.join("zephyr.elf")
                })
            }
        }
    }
}

/// cargo's profile directory for the build: `image.profile`, overridden by a
/// `--release` / `--profile` the caller passed through to cargo.
#[must_use]
pub fn cargo_profile_dir(profile: Option<&str>, native_args: &[String]) -> String {
    let mut p = profile.map(str::to_string);
    let mut it = native_args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--" => break,
            "--release" | "-r" => p = Some("release".into()),
            "--profile" => p = it.next().cloned(),
            _ => {
                if let Some(v) = a.strip_prefix("--profile=") {
                    p = Some(v.to_string());
                }
            }
        }
    }
    match p.as_deref() {
        None | Some("dev" | "test") => "debug".to_string(),
        Some("bench") => "release".to_string(),
        Some(other) => other.to_string(),
    }
}

/// The one binary `<manifest_dir>/Cargo.toml` builds: its single `[[bin]]`,
/// else the package name when `src/main.rs` exists. `None` for a library or
/// several binaries.
#[must_use]
pub fn cargo_bin_name(manifest_dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(manifest_dir.join("Cargo.toml")).ok()?;
    let doc = text.parse::<toml::Table>().ok()?;
    let bins: Vec<&str> = doc
        .get("bin")
        .and_then(|b| b.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|b| b.get("name").and_then(|n| n.as_str()))
                .collect()
        })
        .unwrap_or_default();
    match bins.as_slice() {
        [one] => Some((*one).to_string()),
        [] if manifest_dir.join("src/main.rs").is_file() => doc
            .get("package")?
            .get("name")?
            .as_str()
            .map(str::to_string),
        _ => None,
    }
}

/// The executable a leaf's own `CMakeLists.txt` declares at its top level
/// (`nano_ros_add_executable(<name> …)`), when exactly one is declared.
#[must_use]
pub fn cmake_leaf_executable(leaf: &Path) -> Option<String> {
    let text = std::fs::read_to_string(leaf.join("CMakeLists.txt")).ok()?;
    let names: Vec<String> = text
        .lines()
        .map(str::trim_start)
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.strip_prefix("nano_ros_add_executable("))
        .filter_map(|r| {
            r.split(|c: char| c.is_whitespace() || c == ')')
                .next()
                .filter(|n| !n.is_empty() && !n.contains('$'))
                .map(str::to_string)
        })
        .collect();
    match names.as_slice() {
        [one] => Some(one.clone()),
        _ => None,
    }
}

/// The `-d` / `--build-dir` the caller passed to west, relative to `cwd`.
#[must_use]
pub fn west_dir_from_args(west_extra: &[String], cwd: &Path) -> Option<PathBuf> {
    let mut it = west_extra.iter();
    while let Some(a) = it.next() {
        let v = if a == "-d" || a == "--build-dir" {
            it.next().cloned()
        } else if let Some(v) = a.strip_prefix("--build-dir=") {
            Some(v.to_string())
        } else if a.starts_with("-d") && a.len() > 2 && !a.starts_with("--") {
            Some(a[2..].to_string())
        } else {
            None
        };
        if let Some(v) = v {
            return Some(cwd.join(v));
        }
    }
    None
}

// ── the next command ───────────────────────────────────────────────────

/// What `nros build` prints after a successful build of one image.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NextSteps {
    /// The built file.
    pub artifact: Option<PathBuf>,
    /// `(label, command)`, each pasteable as is.
    pub commands: Vec<(String, String)>,
    /// What could not be derived, and where to read instead.
    pub notes: Vec<String>,
}

/// The book chapter for a platform's run/flash flow, when it has one.
fn chapter(d: &BoardDescriptor) -> Option<&'static str> {
    platform_chapter(&d.platform)
}

fn platform_chapter(platform: &PlatformKind) -> Option<&'static str> {
    Some(match platform {
        PlatformKind::Freertos => "book/src/getting-started/freertos.md",
        PlatformKind::BareMetal => "book/src/getting-started/bare-metal.md",
        PlatformKind::Nuttx => "book/src/getting-started/integration-nuttx.md",
        PlatformKind::Zephyr => "book/src/getting-started/integration-zephyr.md",
        PlatformKind::ThreadxLinux | PlatformKind::ThreadxRiscv64 => {
            "book/src/getting-started/threadx.md"
        }
        _ => return None,
    })
}

/// `[target.<triple>] runner` from the descriptor's `cargo_config`, split.
fn cargo_runner(d: &BoardDescriptor) -> Option<Vec<String>> {
    let triple = triple_of_build_target(d.target.as_deref()?);
    let doc = d.cargo_config.as_deref()?.parse::<toml::Table>().ok()?;
    let runner = doc.get("target")?.get(triple)?.get("runner")?;
    match runner {
        toml::Value::String(s) => Some(s.split_whitespace().map(str::to_string).collect()),
        toml::Value::Array(a) => a
            .iter()
            .map(|v| v.as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>(),
        _ => None,
    }
}

/// The QEMU invocation a board with no cargo `runner` boots under — the
/// shared line (`nros_qemu_ladder::board_line`, phase-517 W14) the test
/// harness launches too (`nros_tests::qemu::QemuProcess::start_nuttx_virt` /
/// `start_nuttx_riscv` / `start_riscv64_virt`). Keyed on the descriptor's
/// platform and triple. ThreadX riscv64 gets peer 0's MAC — the firmware's
/// `Config::default()`, and the harness's talker/server.
fn derived_qemu(d: &BoardDescriptor) -> Option<nros_qemu_ladder::Line> {
    use nros_qemu_ladder::{Board, Net, board_line, threadx_riscv64_mac};
    let triple = d
        .target
        .as_deref()
        .map(triple_of_build_target)
        .unwrap_or("");
    let user = Net::User { mac: None };
    match d.platform {
        PlatformKind::Nuttx if triple.starts_with("armv7a") => {
            Some(board_line(Board::NuttxArmVirt, user))
        }
        PlatformKind::Nuttx if triple.starts_with("riscv32") => {
            Some(board_line(Board::NuttxRvVirt, user))
        }
        PlatformKind::ThreadxRiscv64 => {
            let mac = threadx_riscv64_mac(0);
            Some(board_line(
                Board::ThreadxRiscv64,
                Net::User { mac: Some(&mac) },
            ))
        }
        _ => None,
    }
}

/// A shell word: quoted when it holds anything a shell would split or expand.
fn shell_word(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./=:,+@%".contains(c))
    {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// `argv` (then `artifact`, when given) as a command line, with a leading
/// `qemu-system-*` resolved through the ONE ladder. When no rung answers, the
/// command asks `nros locate` for it at paste time and a note says how to
/// provision it — never `$PATH`.
fn qemu_line(
    argv: &[String],
    artifact: Option<&Path>,
    qemu: &dyn Fn(&str) -> QemuBin,
    notes: &mut Vec<String>,
) -> String {
    let mut words: Vec<String> = Vec::with_capacity(argv.len() + 1);
    for (i, a) in argv.iter().enumerate() {
        if i == 0 && a.starts_with("qemu-system-") {
            match qemu(a) {
                QemuBin::Found { path, .. } => words.push(shell_word(&path.display().to_string())),
                // `RefusedPrefix` needs `StalePrefix::Refuse`, which the CLI
                // never asks for; treated as a miss if that ever changes.
                QemuBin::Missing { why } | QemuBin::RefusedPrefix { why, .. } => {
                    notes.push(why);
                    words.push(format!("\"$(nros locate tool {a})\""));
                }
                QemuBin::Host { why } => {
                    notes.push(why);
                    words.push(shell_word(a));
                }
            }
        } else {
            words.push(shell_word(a));
        }
    }
    if let Some(artifact) = artifact {
        words.push(shell_word(&artifact.display().to_string()));
    }
    words.join(" ")
}

/// What `build.ninja` and `zephyr/runners.yaml` of a finished west build say:
/// the emulators the board supports (`run_<emu>` targets) and its default
/// flash runner. Zephyr owns both facts; nano-ros does not restate them.
fn west_capabilities(build_dir: &Path) -> Option<(Vec<String>, Option<String>)> {
    let ninja = std::fs::read_to_string(build_dir.join("build.ninja")).ok()?;
    let emus: Vec<String> = ninja
        .lines()
        .filter_map(|l| l.strip_prefix("build run_"))
        .filter_map(|r| r.split(':').next())
        .map(str::to_string)
        .collect();
    let flash = std::fs::read_to_string(build_dir.join("zephyr/runners.yaml"))
        .ok()
        .and_then(|y| {
            y.lines()
                .find_map(|l| l.strip_prefix("flash-runner:"))
                .map(|v| v.trim().to_string())
        })
        .filter(|v| !v.is_empty());
    Some((emus, flash))
}

/// phase-517 W6 — the follow-up command(s) for one built image. Reads only the
/// plan, the board descriptor and (for west) the finished build's own files;
/// runs nothing. `qemu` is the QEMU ladder ([`crate::orchestration::qemu_bin`]).
#[must_use]
pub fn next_steps(
    p: &super::build::ResolvedBuild,
    d: &BoardDescriptor,
    qemu: &dyn Fn(&str) -> QemuBin,
) -> NextSteps {
    let mut out = NextSteps::default();
    let see_chapter = |out: &mut NextSteps, why: &str| {
        out.notes.push(match chapter(d) {
            Some(c) => format!("{why} — see {c}"),
            None => format!("{why} — see the platform chapters under book/src/getting-started/"),
        });
    };
    let Some(output) = &p.output else {
        see_chapter(&mut out, "this build road records no artifact location");
        return out;
    };
    out.artifact = output.artifact();

    if let BuildOutput::West { build_dir } = output {
        let Some(dir) = build_dir else {
            see_chapter(&mut out, "the west build dir was named by the caller");
            return out;
        };
        let shown = shell_word(&dir.display().to_string());
        let run = format!("west build -d {shown} -t run");
        let flash = format!("west flash -d {shown}");
        if d.zephyr.as_ref().and_then(|z| z.runner.as_deref()) == Some("armfvp") {
            out.commands.push(("run on the FVP".into(), run));
            out.notes.push(
                "the FVP model must be resolvable when the build CONFIGURES (Zephyr caches it): \
                 `nros setup board fvp-aemv8r-smp`, or ARMFVP_BIN_PATH — see \
                 book/src/getting-started/arm-fvp.md"
                    .into(),
            );
            return out;
        }
        match west_capabilities(dir) {
            Some((emus, flash_runner)) => {
                if !emus.is_empty() {
                    out.commands
                        .push((format!("run ({})", emus.join(", ")), run));
                }
                if let Some(r) = flash_runner.filter(|r| !emus.contains(r)) {
                    out.commands.push((format!("flash ({r})"), flash));
                }
                if out.commands.is_empty() {
                    see_chapter(
                        &mut out,
                        "the board declares no emulator and no flash runner",
                    );
                }
            }
            None => {
                out.commands.push(("run (an emulated board)".into(), run));
                out.commands
                    .push(("flash (a hardware board)".into(), flash));
            }
        }
        return out;
    }

    let Some(artifact) = out.artifact.clone() else {
        see_chapter(
            &mut out,
            "the build names no single executable (a library, or several binaries)",
        );
        return out;
    };
    // Host-executed: no cross triple, and not Zephyr (whose host image is a
    // west build, above).
    if d.target.is_none() && d.platform != PlatformKind::Zephyr {
        out.commands
            .push(("run".into(), shell_word(&artifact.display().to_string())));
        return out;
    }
    if let Some(argv) = cargo_runner(d) {
        let line = qemu_line(&argv, Some(&artifact), qemu, &mut out.notes);
        out.commands.push(("run".into(), line));
        return out;
    }
    if let Some(board) = derived_qemu(d) {
        let mut argv = vec![board.exe.to_string()];
        argv.extend(board.argv(&artifact.display().to_string()));
        let line = qemu_line(&argv, None, qemu, &mut out.notes);
        out.commands.push(("run on QEMU".into(), line));
        return out;
    }
    see_chapter(
        &mut out,
        &format!(
            "no run or flash command is derivable for board `{}` (its descriptor names no \
             runner)",
            p.board
        ),
    );
    out
}

/// The text block [`next_steps`] prints, one image.
#[must_use]
pub fn render(qualified: &str, steps: &NextSteps) -> String {
    let mut s = format!("nros build: next for `{qualified}`:\n");
    if let Some(a) = &steps.artifact {
        s.push_str(&format!("  artifact: {}\n", a.display()));
    }
    for (label, cmd) in &steps.commands {
        s.push_str(&format!("  {label}:\n    {cmd}\n"));
    }
    for n in &steps.notes {
        s.push_str(&format!("  note: {n}\n"));
    }
    s
}

// ── --board ────────────────────────────────────────────────────────────

/// One declared image, as `--board` sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// `<bringup>:<id>` (or `<package>:<id>` for a single-package leaf).
    pub qualified: String,
    /// What to pass as the image argument to select exactly this image.
    pub arg: String,
    /// `[image.*] board`, as authored (empty when unstated).
    pub board: String,
}

/// Does an image whose `board` is `image_board` match the word `want`?
///
/// Equal ignoring case; or both resolve to the SAME descriptor; or `want` is
/// an index board key and the image's descriptor provisions through it.
#[must_use]
pub fn board_matches(
    cat: &BoardCatalog,
    image_board: &str,
    want: &str,
    is_index_key: &dyn Fn(&str) -> bool,
) -> bool {
    if image_board.is_empty() {
        return false;
    }
    if image_board.eq_ignore_ascii_case(want) {
        return true;
    }
    let DeployResolution::Board(have) = cat.resolve_deploy(image_board) else {
        return false;
    };
    if let DeployResolution::Board(w) = cat.resolve_deploy(want)
        && std::ptr::eq(w, have)
    {
        return true;
    }
    is_index_key(want) && have.setup_key(is_index_key).as_deref() == Some(want)
}

/// Pick the one image `want` selects, or refuse naming the candidates.
pub fn select_one(
    cands: &[Candidate],
    want: &str,
    matches: &dyn Fn(&str) -> bool,
) -> std::result::Result<String, String> {
    let hit: Vec<&Candidate> = cands.iter().filter(|c| matches(&c.board)).collect();
    match hit.as_slice() {
        [one] => Ok(one.arg.clone()),
        [] => Err(format!(
            "no image is built for board `{want}`. Declared images:\n{}",
            if cands.is_empty() {
                "  (none)".to_string()
            } else {
                cands
                    .iter()
                    .map(|c| {
                        format!(
                            "  {} — board `{}`",
                            c.qualified,
                            if c.board.is_empty() { "?" } else { &c.board }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        )),
        many => Err(format!(
            "board `{want}` matches {} images: {}. Name one: `nros build <image>`.",
            many.len(),
            many.iter()
                .map(|c| c.qualified.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// Every image `root` declares — a single-package leaf's `[image.*]`, or every
/// bringup's in a workspace — with its board, plus the package dirs (where a
/// workspace's own board descriptors live).
fn declared(root: &Path) -> Result<(Vec<Candidate>, Vec<PathBuf>)> {
    use nros_orchestration_ir::leaf_system;
    if root.join(leaf_system::SYSTEM_TOML).is_file() && leaf_system::is_package_dir(root) {
        let pkg = crate::cmd::leaf_settings::leaf_package(root)
            .or_else(|| {
                std::fs::read_to_string(root.join("package.xml"))
                    .ok()
                    .and_then(|x| crate::orchestration::prereq_resolve::package_name(&x))
            })
            .or_else(|| root.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "leaf".to_string());
        let path = root.join(leaf_system::SYSTEM_TOML);
        let doc = std::fs::read_to_string(&path)
            .wrap_err_with(|| format!("reading {}", path.display()))?
            .parse::<toml::Table>()
            .wrap_err_with(|| format!("parsing {}", path.display()))?;
        let ids: Vec<String> = doc
            .get("image")
            .and_then(|t| t.as_table())
            .map(|t| t.keys().cloned().collect())
            .unwrap_or_default();
        let mut out = Vec::new();
        for id in ids {
            let board = leaf_system::read_image(root, &id)
                .map_err(|e| eyre::eyre!(e))?
                .and_then(|s| s.board)
                .unwrap_or_default();
            out.push(Candidate {
                qualified: plan::qualified(&pkg, &id),
                arg: id,
                board,
            });
        }
        return Ok((out, Vec::new()));
    }
    let members = crate::builder::discover::cargo_members_or_walk(root);
    let found =
        crate::builder::discover::discover(root, &members).map_err(|e| eyre::eyre!("{e}"))?;
    let bringups = super::build::collect_images(&found.packages)?;
    let out = plan::all_images(&bringups)
        .into_iter()
        .map(|(b, _, id, img)| {
            let q = plan::qualified(&b, &id);
            Candidate {
                qualified: q.clone(),
                arg: q,
                board: img.board.unwrap_or_default(),
            }
        })
        .collect();
    Ok((out, found.packages.iter().map(|p| p.dir.clone()).collect()))
}

/// `nros build --board <want>`: the one image argument it stands for.
pub fn images_for_board(root: &Path, nano_ros_root: Option<&Path>, want: &str) -> Result<String> {
    let (cands, pkg_dirs) = declared(root)?;
    let Some(nros_root) = nano_ros_root else {
        eyre::bail!("{}", crate::orchestration::nano_ros_root::not_found_help());
    };
    let cat = BoardCatalog::load_with_packages(nros_root, &pkg_dirs).map_err(|e| {
        eyre::eyre!(
            "loading board descriptors from {}: {e}",
            nros_root.display()
        )
    })?;
    let index =
        crate::orchestration::sdk_index::SdkIndex::load(&nros_root.join("nros-sdk-index.toml"))
            .ok();
    let is_key = |k: &str| index.as_ref().is_some_and(|i| i.board.contains_key(k));
    select_one(&cands, want, &|b| board_matches(&cat, b, want, &is_key))
        .map_err(|e| eyre::eyre!("nros build --board: {e}"))
}

/// The refusal for `--board` beside explicit images or `--all`.
pub fn refuse_board_with(images: &[String], all: bool) -> Option<String> {
    if all {
        return Some(
            "`--board` selects images by board; `--all` builds every image. Pass one, not both."
                .into(),
        );
    }
    (!images.is_empty()).then(|| {
        format!(
            "`--board` selects the image for you; you also named {}. Pass the image(s) or \
             `--board`, not both.",
            images.join(", ")
        )
    })
}

#[cfg(test)]
mod tests {

    /// Every chapter a printed note points at exists — phase-517 W2 moved
    /// `getting-started/{zephyr,nuttx}.md` to `internals/` and this table kept
    /// naming them.
    #[test]
    fn every_chapter_named_exists() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let kinds = [
            PlatformKind::Freertos,
            PlatformKind::BareMetal,
            PlatformKind::Nuttx,
            PlatformKind::Zephyr,
            PlatformKind::ThreadxLinux,
            PlatformKind::ThreadxRiscv64,
        ];
        for k in kinds {
            let ch = platform_chapter(&k).unwrap_or_else(|| panic!("{k:?} names no chapter"));
            assert!(root.join(ch).is_file(), "{k:?}: {ch} does not exist");
        }
    }
    use super::*;
    use crate::{builder::plan::Driver, cmd::build::ResolvedBuild};

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("repo root")
            .to_path_buf()
    }

    fn catalog() -> BoardCatalog {
        BoardCatalog::load(&repo_root()).expect("load catalog")
    }

    fn descriptor(cat: &BoardCatalog, board: &str) -> BoardDescriptor {
        match cat.resolve_deploy(board) {
            DeployResolution::Board(d) => d.clone(),
            other => panic!("{board}: {other:?}"),
        }
    }

    fn plan(board: &str, driver: Driver, output: BuildOutput) -> ResolvedBuild {
        ResolvedBuild {
            qualified: format!("demo:{board}"),
            board: board.to_string(),
            platform: String::new(),
            driver,
            handoff: None,
            rmw: None,
            entry_package: None,
            target: None,
            profile: None,
            cmake_build_dir: None,
            capabilities: Vec::new(),
            configure: None,
            record: Default::default(),
            output: Some(output),
            descriptor: None,
        }
    }

    fn store_qemu(exe: &str) -> QemuBin {
        QemuBin::Found {
            path: PathBuf::from("/store/sdk/qemu/11.0/bin").join(exe),
            rung: "store",
        }
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nros-build-next-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// W6 done-when, cargo: a board whose descriptor carries a cargo `runner`
    /// prints that runner, with QEMU from the ladder and the artifact where
    /// the plan says cargo put it.
    #[test]
    fn a_cargo_runner_board_prints_its_runner_with_the_store_qemu() {
        let cat = catalog();
        let d = descriptor(&cat, "mps2-an385-baremetal");
        let p = plan(
            "mps2-an385-baremetal",
            Driver::Cargo,
            BuildOutput::Cargo {
                target_dir: "/ws/build/bare-metal/demo_entry/target".into(),
                triple: Some("thumbv7m-none-eabi".into()),
                profile_dir: "release".into(),
                bin: Some("demo_entry".into()),
            },
        );
        let s = next_steps(&p, &d, &store_qemu);
        let art = "/ws/build/bare-metal/demo_entry/target/thumbv7m-none-eabi/release/demo_entry";
        assert_eq!(s.artifact.as_deref(), Some(Path::new(art)));
        assert_eq!(
            s.commands,
            vec![(
                "run".to_string(),
                format!(
                    "/store/sdk/qemu/11.0/bin/qemu-system-arm -cpu cortex-m3 -machine mps2-an385 \
                     -nographic -semihosting-config enable=on,target=native -kernel {art}"
                )
            )]
        );
        assert!(s.notes.is_empty(), "{:?}", s.notes);

        // Nothing provisioned: never a bare PATH `qemu-system-arm`.
        let missing = |_: &str| QemuBin::Missing {
            why: "provision it with `nros setup --tool qemu`".into(),
        };
        let s = next_steps(&p, &d, &missing);
        assert!(
            s.commands[0]
                .1
                .starts_with("\"$(nros locate tool qemu-system-arm)\" -cpu"),
            "{:?}",
            s.commands
        );
        assert!(
            s.notes[0].contains("nros setup --tool qemu"),
            "{:?}",
            s.notes
        );
    }

    /// W6 done-when, west: the run / flash split is read from the finished
    /// build's own files (Zephyr owns which emulator a board has).
    #[test]
    fn a_west_board_prints_west_run_or_west_flash_from_its_build() {
        let cat = catalog();
        let d = descriptor(&cat, "zephyr");
        let dir = tmp("west");
        std::fs::create_dir_all(dir.join("zephyr")).unwrap();
        std::fs::write(
            dir.join("build.ninja"),
            "build run: phony zephyr/run\nbuild run_native: phony zephyr/run_native\n",
        )
        .unwrap();
        std::fs::write(dir.join("zephyr/runners.yaml"), "flash-runner: native\n").unwrap();
        let p = plan(
            "zephyr",
            Driver::West,
            BuildOutput::West {
                build_dir: Some(dir.clone()),
            },
        );
        let s = next_steps(&p, &d, &store_qemu);
        assert_eq!(
            s.commands,
            vec![(
                "run (native)".to_string(),
                format!("west build -d {} -t run", dir.display())
            )]
        );
        assert_eq!(s.artifact, Some(dir.join("zephyr/zephyr.elf")));

        // A hardware board: no emulator target, a flash runner.
        std::fs::write(dir.join("build.ninja"), "build flash: phony zephyr/flash\n").unwrap();
        std::fs::write(dir.join("zephyr/runners.yaml"), "flash-runner: jlink\n").unwrap();
        let s = next_steps(&p, &d, &store_qemu);
        assert_eq!(
            s.commands,
            vec![(
                "flash (jlink)".to_string(),
                format!("west flash -d {}", dir.display())
            )]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The FVP's descriptor says `runner = "armfvp"`: `-t run`, and the
    /// licence/model caveat as a note rather than an invented command.
    #[test]
    fn the_fvp_board_prints_west_run_and_names_the_model_requirement() {
        let cat = catalog();
        let d = descriptor(&cat, "fvp-aemv8r-smp");
        let p = plan(
            "fvp-aemv8r-smp",
            Driver::West,
            BuildOutput::West {
                build_dir: Some("/ws/build/zephyr/b__fvp_west".into()),
            },
        );
        let s = next_steps(&p, &d, &store_qemu);
        assert_eq!(
            s.commands[0].1,
            "west build -d /ws/build/zephyr/b__fvp_west -t run"
        );
        assert!(s.notes[0].contains("arm-fvp.md"), "{:?}", s.notes);
    }

    /// W6 done-when, cmake: a host-executed board prints the binary itself;
    /// a cross board prints its runner over the cmake executable.
    #[test]
    fn a_cmake_board_prints_its_executable_or_its_runner() {
        let cat = catalog();
        let host = plan(
            "freertos-posix",
            Driver::CMake,
            BuildOutput::CMake {
                build_dir: "/ws/build/freertos-zenoh-freertos-posix/cmake".into(),
                executable: Some("posix_entry".into()),
            },
        );
        let s = next_steps(&host, &descriptor(&cat, "freertos-posix"), &store_qemu);
        assert_eq!(
            s.commands,
            vec![(
                "run".to_string(),
                "/ws/build/freertos-zenoh-freertos-posix/cmake/posix_entry".to_string()
            )]
        );

        let cross = plan(
            "mps2-an385-freertos",
            Driver::CMake,
            BuildOutput::CMake {
                build_dir: "/leaf/build/mps2-an385-freertos/cmake".into(),
                executable: Some("c_talker".into()),
            },
        );
        let s = next_steps(
            &cross,
            &descriptor(&cat, "mps2-an385-freertos"),
            &store_qemu,
        );
        assert_eq!(
            s.commands[0].1,
            "/store/sdk/qemu/11.0/bin/qemu-system-arm -cpu cortex-m3 -machine mps2-an385 \
             -nographic -semihosting-config enable=on,target=native -kernel \
             /leaf/build/mps2-an385-freertos/cmake/c_talker"
        );
    }

    /// NuttX and ThreadX riscv64 carry no cargo runner: the harness's
    /// invocation, keyed on platform + triple. S32Z270 (hardware, no runner)
    /// gets a chapter pointer, never an invented command.
    #[test]
    fn boards_without_a_runner_get_the_harness_qemu_or_a_chapter() {
        let cat = catalog();
        let out = |t: &str| BuildOutput::Cargo {
            target_dir: "/t".into(),
            triple: Some(t.into()),
            profile_dir: "debug".into(),
            bin: Some("e".into()),
        };
        // phase-517 W14 — the printed command IS the shared board line (the
        // test harness's launchers assert the same against it in
        // `nros_tests::qemu`), with the program from the ladder.
        use nros_qemu_ladder::{Board, Net, board_line, threadx_riscv64_mac};
        let expected = |line: nros_qemu_ladder::Line, kernel: &str| {
            let mut words = vec![format!("/store/sdk/qemu/11.0/bin/{}", line.exe)];
            words.extend(line.argv(kernel));
            words.join(" ")
        };
        let user = Net::User { mac: None };
        let s = next_steps(
            &plan("nuttx", Driver::Cargo, out("armv7a-nuttx-eabihf")),
            &descriptor(&cat, "nuttx"),
            &store_qemu,
        );
        assert_eq!(
            s.commands[0].1,
            expected(
                board_line(Board::NuttxArmVirt, user),
                "/t/armv7a-nuttx-eabihf/debug/e"
            )
        );
        assert!(
            s.commands[0]
                .1
                .starts_with("/store/sdk/qemu/11.0/bin/qemu-system-arm -M virt -cpu cortex-a7"),
            "{:?}",
            s.commands
        );

        let s = next_steps(
            &plan(
                "nuttx-riscv",
                Driver::Cargo,
                out("riscv32imac-unknown-nuttx-elf"),
            ),
            &descriptor(&cat, "nuttx-riscv"),
            &store_qemu,
        );
        assert_eq!(
            s.commands[0].1,
            expected(
                board_line(Board::NuttxRvVirt, user),
                "/t/riscv32imac-unknown-nuttx-elf/debug/e"
            )
        );

        let s = next_steps(
            &plan(
                "threadx-riscv64",
                Driver::Cargo,
                out("riscv64gc-unknown-none-elf"),
            ),
            &descriptor(&cat, "threadx-riscv64"),
            &store_qemu,
        );
        let mac = threadx_riscv64_mac(0);
        assert_eq!(
            s.commands[0].1,
            expected(
                board_line(Board::ThreadxRiscv64, Net::User { mac: Some(&mac) }),
                "/t/riscv64gc-unknown-none-elf/debug/e"
            )
        );

        let s = next_steps(
            &plan(
                "s32z270",
                Driver::CMake,
                BuildOutput::CMake {
                    build_dir: "/b".into(),
                    executable: Some("e".into()),
                },
            ),
            &descriptor(&cat, "s32z270"),
            &store_qemu,
        );
        assert!(s.commands.is_empty(), "{:?}", s.commands);
        assert!(
            s.notes[0].contains("book/src/getting-started/freertos.md"),
            "{:?}",
            s.notes
        );
    }

    #[test]
    fn the_artifact_follows_cargos_layout() {
        assert_eq!(cargo_profile_dir(None, &[]), "debug");
        assert_eq!(cargo_profile_dir(Some("release"), &[]), "release");
        assert_eq!(cargo_profile_dir(None, &["--release".into()]), "release");
        assert_eq!(
            cargo_profile_dir(Some("release"), &["--profile=lto".into()]),
            "lto"
        );
        // After a second `--` the args are the program's, not cargo's.
        assert_eq!(
            cargo_profile_dir(None, &["--".into(), "--release".into()]),
            "debug"
        );
        let host = BuildOutput::Cargo {
            target_dir: "/t".into(),
            triple: None,
            profile_dir: "debug".into(),
            bin: Some("linux_entry".into()),
        };
        assert_eq!(host.artifact(), Some(PathBuf::from("/t/debug/linux_entry")));
    }

    // ── --board ──

    fn cands() -> Vec<Candidate> {
        [
            ("b:linux", "linux"),
            ("b:freertos", "mps2-an385-freertos"),
            ("b:bare", "baremetal"),
            ("b:rtic", "rtic-mps2-an385"),
            ("b:zephyr", "zephyr"),
        ]
        .into_iter()
        .map(|(q, b)| Candidate {
            qualified: q.into(),
            arg: q.into(),
            board: b.into(),
        })
        .collect()
    }

    fn pick(want: &str) -> std::result::Result<String, String> {
        let cat = catalog();
        let index = crate::orchestration::sdk_index::SdkIndex::load(
            &repo_root().join("nros-sdk-index.toml"),
        )
        .expect("index");
        let is_key = |k: &str| index.board.contains_key(k);
        select_one(&cands(), want, &|b| board_matches(&cat, b, want, &is_key))
    }

    #[test]
    fn board_selects_one_image_by_word_alias_or_setup_key() {
        // The authored word, any case.
        assert_eq!(pick("LINUX").as_deref(), Ok("b:linux"));
        // Another alias of the same descriptor (`freertos` names it too).
        assert_eq!(pick("freertos").as_deref(), Ok("b:freertos"));
        // The index key the board provisions through.
        assert_eq!(pick("mps2-an385-freertos").as_deref(), Ok("b:freertos"));
    }

    #[test]
    fn board_matching_several_images_is_refused_naming_them() {
        // `mps2-an385-baremetal` is the setup key of BOTH bare-metal images.
        let e = pick("mps2-an385-baremetal").expect_err("two images");
        assert!(e.contains("b:bare") && e.contains("b:rtic"), "{e}");
        assert!(e.contains("Name one"), "{e}");
    }

    #[test]
    fn board_matching_nothing_is_refused_listing_every_image_and_board() {
        let e = pick("threadx-linux").expect_err("no image");
        assert!(e.contains("b:zephyr — board `zephyr`"), "{e}");
        assert!(e.contains("b:linux — board `linux`"), "{e}");
    }

    #[test]
    fn board_with_images_or_all_is_refused() {
        assert!(refuse_board_with(&["linux".into()], false).is_some());
        assert!(refuse_board_with(&[], true).is_some());
        assert!(refuse_board_with(&[], false).is_none());
    }

    /// The leaf road end to end, minus the build: a single-package leaf's
    /// images are enumerated from its own `system.toml`, and the selected
    /// argument is the bare image id that road accepts.
    #[test]
    fn board_resolves_against_a_single_package_leaf() {
        let leaf = tmp("leaf");
        std::fs::write(
            leaf.join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.0.0\"\n",
        )
        .unwrap();
        std::fs::write(
            leaf.join("system.toml"),
            "[system]\nname = \"demo\"\n\n[image.host]\nboard = \"linux\"\n\n\
             [image.qemu]\nboard = \"baremetal\"\n",
        )
        .unwrap();
        let root = repo_root();
        assert_eq!(
            images_for_board(&leaf, Some(&root), "mps2-an385-baremetal").expect("one"),
            "qemu"
        );
        let e = images_for_board(&leaf, Some(&root), "zephyr").expect_err("none");
        assert!(format!("{e}").contains("demo:host"), "{e}");
        let _ = std::fs::remove_dir_all(&leaf);
    }

    /// The class behind phase-517's rv-virt finding: EVERY board whose next
    /// step names a `qemu-system-*` names one the store builds (resolved to
    /// the store path), or a bare one the notes say to install from the host
    /// — never a store QEMU that does not exist, never `nros setup --tool
    /// qemu` for a target it would not provide. The store's target list is
    /// the index's, not restated here. ESP32 boards are dormant (issue 1525).
    #[test]
    fn every_printed_qemu_is_the_stores_or_named_for_host_install() {
        use crate::orchestration::qemu_bin::{self, Ctx};
        let root = repo_root();
        let index =
            crate::orchestration::sdk_index::SdkIndex::load(&root.join("nros-sdk-index.toml"))
                .expect("index");
        let targets = qemu_bin::store_targets(&index).expect("[tool.qemu.source] target list");
        let store = tmp("every-qemu");
        for t in &targets {
            let b = store.join("bin").join(format!("qemu-system-{t}"));
            std::fs::create_dir_all(b.parent().unwrap()).unwrap();
            std::fs::write(&b, "").unwrap();
        }
        let none = |_: &str| None;
        let ctx = Ctx {
            getenv: &none,
            project_prefix: None,
            pin: None,
            store_dir: Some(store.clone()),
            store_targets: Some(targets.clone()),
            stale_prefix: qemu_bin::StalePrefix::Skip,
        };
        let qemu = |e: &str| qemu_bin::locate(e, &ctx);
        let cat = catalog();
        let (mut from_store, mut from_host) = (Vec::new(), Vec::new());
        for d in cat.descriptors() {
            if matches!(d.platform, PlatformKind::Zephyr | PlatformKind::Esp32) {
                continue;
            }
            let name = d.names.first().cloned().unwrap_or_default();
            let out = BuildOutput::Cargo {
                target_dir: "/t".into(),
                triple: d
                    .target
                    .as_deref()
                    .map(|t| triple_of_build_target(t).to_string()),
                profile_dir: "debug".into(),
                bin: Some("e".into()),
            };
            let s = next_steps(&plan(&name, Driver::Cargo, out), d, &qemu);
            // Every word naming a QEMU, in whatever form it was printed
            // (store path, bare, or a paste-time `"$(nros locate …)"`).
            let words = s.commands.iter().flat_map(|(_, c)| c.split_whitespace());
            for first in words.filter(|w| w.contains("qemu-system-")) {
                let at = first.find("qemu-system-").unwrap();
                let exe: String = first[at..]
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                    .collect();
                let arch = &exe["qemu-system-".len()..];
                if targets.iter().any(|t| t == arch) {
                    assert!(
                        Path::new(first).starts_with(&store),
                        "{name}: `{exe}` is a store target but printed as {first}"
                    );
                    from_store.push(name.clone());
                } else {
                    assert_eq!(first, exe, "{name}: a host QEMU is printed bare");
                    assert!(
                        s.notes
                            .iter()
                            .any(|n| n.contains(&format!("`{exe}` is your host's"))
                                && n.contains("apt install")
                                && !n.contains("nros setup")),
                        "{name}: `{exe}` is not in the store ({targets:?}) and no note says \
                         where it comes from: {:?}",
                        s.notes
                    );
                    from_host.push(name.clone());
                }
            }
        }
        // The sweep reached both kinds (a vacuous pass is a red).
        assert!(from_store.len() >= 3, "{from_store:?}");
        assert!(
            from_host.iter().any(|n| cat
                .descriptors()
                .iter()
                .any(|d| d.names.contains(n) && d.platform == PlatformKind::Nuttx)),
            "the rv-virt NuttX board should print a host QEMU: {from_host:?}"
        );
        let _ = std::fs::remove_dir_all(&store);
    }

    /// Every chapter a next-step note points at exists.
    #[test]
    fn every_chapter_a_note_names_exists() {
        let root = repo_root();
        for d in catalog().descriptors() {
            if let Some(c) = chapter(d) {
                assert!(root.join(c).is_file(), "{:?}: {c} does not exist", d.names);
            }
        }
    }
}
