//! `nros locate` — phase-484 W1 (RFC-0103 D4): where a resource is, and why.
//!
//! The front door for every road that cannot link `nros_build_paths` —
//! cmake, `just`, shell. The ladder itself is `nros_build_paths::locate`
//! (one implementation; build scripts call it directly); this verb builds the
//! row from the index model, gathers the context, and prints.
//!
//! `--why` prints every rung and what it saw, so "which FreeRTOS did this
//! use?" is answered by reading, not by inferring.

use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use eyre::{Result, bail};
use nros_build_paths::locate::{self, Ctx, Row};

use crate::orchestration::{
    nano_ros_root,
    sdk_index::{SdkIndex, SourcePackage},
};

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum Format {
    /// The path alone (one name) or `name<TAB>path` lines.
    #[default]
    Path,
    /// `NROS_LOCATE_<NAME>=<path>` lines, shell-quoted.
    Sh,
    /// `set(NROS_LOCATE_<NAME> "<path>")` lines — normal variables, never CACHE.
    Cmake,
}

#[derive(Debug, Parser)]
pub struct Args {
    /// Resources to locate: a `[source.<name>]` of the index, or `store` /
    /// `nano-ros` for the two roots. `artifact <name>…` instead locates a
    /// cargo-built artifact (`c-archive`, `cpp-archive`, `c-include`,
    /// `cpp-include`, `c-config-header`, `cpp-config-header`); `tool <name>`
    /// a host binary (`nros-launch-resolve`, `renode`, or `qemu-system-<arch>`
    /// through the one QEMU ladder), or the install directory of an SDK tool
    /// `nros setup <board>` provisioned (`tool zephyr-sdk`).
    pub names: Vec<String>,

    /// Every `[source.*]` row. A row that resolves nowhere is skipped in
    /// batch output (and named on stderr), never invented.
    #[arg(long)]
    pub all: bool,

    /// Print every rung and what it saw.
    #[arg(long)]
    pub why: bool,

    #[arg(long, value_enum, default_value_t)]
    pub format: Format,

    /// Path to the SDK index.
    #[arg(long, default_value = "nros-sdk-index.toml")]
    pub index: PathBuf,

    /// `artifact` mode: the target triple the artifact was built for. Omit
    /// for a host build cargo ran without `--target`.
    #[arg(long)]
    pub target: Option<String>,

    /// `artifact` mode: the cargo PROFILE (`dev`, `release`, a custom one).
    #[arg(long, default_value = "release")]
    pub profile: String,

    /// `artifact` mode: the cargo target dir. Default: cargo's own answer
    /// for the nano-ros workspace (`CARGO_TARGET_DIR`, `[build] target-dir`,
    /// else `<root>/target`).
    #[arg(long)]
    pub target_dir: Option<PathBuf>,

    /// `artifact` mode: the directory a build PLACED its config headers in.
    ///
    /// REQUIRED for the header artifacts, which live only in cargo's
    /// `$OUT_DIR`; an archive is never placed, so naming one with this is
    /// refused.
    // The placing step is `scripts/build/cargo-out-dir-headers.py --dest`
    // (issue 1824).
    #[arg(long)]
    pub placed_dir: Option<PathBuf>,

    /// The project whose `system.toml` `[sources]` is rung 3.
    ///
    /// Default: `$NROS_WORKSPACE_ROOT`, else the nearest ancestor of the
    /// working directory that holds a `system.toml`.
    // The rung ladder is RFC-0103 D3.
    #[arg(long)]
    pub project: Option<PathBuf>,
}

/// The project directory for rung 3: explicit, else the one the generated
/// build config names (`NROS_WORKSPACE_ROOT`), else the nearest ancestor of
/// the working directory holding a `system.toml`.
pub(crate) fn project_dir(explicit: Option<&std::path::Path>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        return Some(p.to_path_buf());
    }
    if let Some(p) = std::env::var_os("NROS_WORKSPACE_ROOT").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(p));
    }
    let cwd = std::env::current_dir().ok()?;
    cwd.ancestors()
        .find(|d| d.join("system.toml").is_file())
        .map(std::path::Path::to_path_buf)
}

fn row_of(name: &str, s: &SourcePackage) -> Row {
    crate::orchestration::sdk_store::locate_row(name, s)
}

/// One `[source.<name>]` located through the ONE ladder, with `root` as the
/// checkout — the same answer `nros locate <name>` prints. For other verbs
/// (`board-facts`) that need a source's location, so none re-derives it.
pub(crate) fn locate_source(
    index: &SdkIndex,
    root: &std::path::Path,
    name: &str,
) -> std::result::Result<PathBuf, String> {
    locate_answer(index, root, name, project_dir(None).as_deref())
        .map(|a| nros_build_paths::canonical(&a.path))
}

/// [`locate_source`] with the project stated and the whole answer kept (the
/// rung it came from), for a caller that RECORDS where a tree came from —
/// `resolved.toml`'s `[locations]`.
pub(crate) fn locate_answer(
    index: &SdkIndex,
    root: &std::path::Path,
    name: &str,
    project: Option<&std::path::Path>,
) -> std::result::Result<locate::Answer, String> {
    let src = index
        .source
        .get(name)
        .ok_or_else(|| format!("no [source.{name}] in the index"))?;
    let row = row_of(name, src);
    let checkout = root
        .join(nros_build_paths::CHECKOUT_MARKER)
        .is_file()
        .then(|| root.to_path_buf());
    let ctx = Ctx {
        arg: None,
        env_value: row
            .env
            .as_deref()
            .and_then(std::env::var_os)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from),
        project: project.and_then(|d| locate::project_source(d, name)),
        checkout_edit: locate::checkout_edit(&row, checkout.as_deref()),
        checkout,
        store: crate::orchestration::store::root(),
    };
    locate::resolve(&row, &ctx).map_err(|r| r.to_string())
}

fn var_name(name: &str) -> String {
    format!(
        "NROS_LOCATE_{}",
        name.to_ascii_uppercase().replace(['-', '.'], "_")
    )
}

pub fn run(args: Args) -> Result<()> {
    if let Some(e) = crate::orchestration::store::retired_in_env() {
        bail!(e);
    }
    // `store` and `nano-ros` need no index: the store is `$NROS_HOME`, and the
    // SDK root is a function of where this binary lives (the `nano_ros_root`
    // ladder). Loading the index for them would make `nros locate nano-ros`
    // fail in a project with no index, or block on the network to answer a
    // question the binary already knows (phase-532 W9).
    if !args.all
        && !args.names.is_empty()
        && args.names.iter().all(|n| n == "store" || n == "nano-ros")
    {
        return run_roots(&args);
    }
    let (index_path, index) = crate::cmd::setup::load_resolved_index(&args.index)?;
    // Absolute: a `--why` trace and a cmake `set()` are read from other cwds.
    let sdk_root = nros_build_paths::canonical(&crate::cmd::setup::index_workspace(&index_path));
    let checkout = sdk_root
        .join(nros_build_paths::CHECKOUT_MARKER)
        .is_file()
        .then(|| sdk_root.clone());
    let store = crate::orchestration::store::root();
    let project = project_dir(args.project.as_deref());

    if args.names.first().map(String::as_str) == Some("artifact") {
        return run_artifact(&args, checkout.as_deref());
    }
    if args.names.first().map(String::as_str) == Some("tool") {
        return run_tool(&args, checkout.as_deref(), &sdk_root, &index);
    }

    let mut names = args.names.clone();
    if args.all {
        names.extend(index.source.keys().cloned());
    }
    if names.is_empty() {
        bail!("name a resource (`nros locate zenoh-pico`), or pass --all");
    }
    let batch = names.len() > 1;

    let mut failed = 0usize;
    for name in &names {
        let answer: Result<(PathBuf, Vec<locate::Step>), String> = match name.as_str() {
            "store" => Ok((
                store.clone(),
                vec![locate::Step {
                    rung: "root",
                    saw: format!(
                        "{} ({})",
                        store.display(),
                        crate::orchestration::store::root_origin()
                    ),
                    chosen: true,
                }],
            )),
            "nano-ros" => locate_nano_ros(project.as_deref()),
            _ => {
                let Some(src) = index.source.get(name) else {
                    // An SDK tool is a `tool <name>` (RFC-0103 §3): say so
                    // rather than list sources it is not among.
                    if index.tool.contains_key(name) {
                        bail!("`{name}` is an SDK tool, not a source: `nros locate tool {name}`");
                    }
                    let known: Vec<&str> = index.source.keys().map(String::as_str).collect();
                    bail!(
                        "no `[source.{name}]` in {} — the index has: store, nano-ros, {}",
                        index_path.display(),
                        known.join(", ")
                    );
                };
                let row = row_of(name, src);
                let ctx = Ctx {
                    arg: None,
                    env_value: row
                        .env
                        .as_deref()
                        .and_then(std::env::var_os)
                        .filter(|v| !v.is_empty())
                        .map(PathBuf::from),
                    project: project
                        .as_deref()
                        .and_then(|d| locate::project_source(d, name)),
                    checkout_edit: locate::checkout_edit(&row, checkout.as_deref()),
                    checkout: checkout.clone(),
                    store: store.clone(),
                };
                match locate::resolve(&row, &ctx) {
                    Ok(a) => Ok((nros_build_paths::canonical(&a.path), a.trace)),
                    Err(r) => Err(r.to_string()),
                }
            }
        };
        match answer {
            Ok((path, trace)) => {
                match args.format {
                    Format::Path if batch => println!("{name}\t{}", path.display()),
                    Format::Path => println!("{}", path.display()),
                    Format::Sh => println!(
                        "{}='{}'",
                        var_name(name),
                        path.display().to_string().replace('\'', "'\\''")
                    ),
                    Format::Cmake => {
                        println!("set({} \"{}\")", var_name(name), path.display());
                        // The row's override NAME as a second key, so a cmake
                        // consumer that only knows `FREERTOS_DIR` asks the same
                        // ladder rather than reading `$ENV{}` raw (phase-484 W2c).
                        if let Some(env) = index.source.get(name).and_then(|s| s.env.as_deref()) {
                            println!("set(NROS_LOCATE_ENV_{env} \"{}\")", path.display());
                        }
                    }
                }
                if args.why {
                    for s in &trace {
                        eprintln!(
                            "  {:<10} {}{}",
                            s.rung,
                            s.saw,
                            if s.chosen { "        <- chosen" } else { "" }
                        );
                    }
                }
            }
            Err(e) => {
                failed += 1;
                eprintln!("{e}");
            }
        }
    }
    // A batch reports what it found and names the rest; a single lookup that
    // missed is a failure — the caller was about to use the path.
    if failed > 0 && (!batch || !args.all) {
        bail!("{failed} resource(s) not located");
    }
    Ok(())
}

/// The SDK root for the project in hand, through the ONE `nano_ros_root`
/// ladder `nros dev sdk root` uses: `--nano-ros-path`, then the nano-ros
/// checkout enclosing the project, then this binary's own SDK root
/// (`share/nano-ros` beside a released `nros`).
///
/// Several projects on one machine may pin different nano-ros versions
/// (`nros-toolchain.toml`). The launcher reads that pin and runs the pinned
/// toolchain before any verb, so in the project the binary answering IS the
/// pinned one and its own root is that version's root. The case that slips
/// past the launcher is `--project` naming ANOTHER project, pinned to another
/// version: the own-root rung would then answer for the wrong version, so it
/// refuses and says where to ask instead.
fn locate_nano_ros(
    project: Option<&std::path::Path>,
) -> Result<(PathBuf, Vec<locate::Step>), String> {
    let cwd = std::env::current_dir().map_err(|e| format!("nano-ros: {e}"))?;
    let workspace = project.map(std::path::Path::to_path_buf).unwrap_or(cwd);
    let Some((root, rung)) = nano_ros_root::resolve_explained(None, &workspace) else {
        return Err(format!("nano-ros: {}", nano_ros_root::not_found_help()));
    };
    let running = std::env::current_exe()
        .ok()
        .and_then(|exe| nros_launcher::pin::running_version(&exe))
        .map(|(v, _)| v);
    let pin = nros_launcher::pin::find_and_load(&workspace).ok().flatten();
    if matches!(rung, nano_ros_root::Rung::OwnShipped) {
        if let Some(msg) = pin_mismatch(running.as_deref(), pin.as_ref(), &workspace) {
            return Err(msg);
        }
    }
    let mut trace = vec![locate::Step {
        rung: "root",
        saw: format!("{} (via {})", root.display(), rung.describe()),
        chosen: true,
    }];
    trace.push(locate::Step {
        rung: "version",
        saw: match (&running, &pin) {
            (Some(v), Some(p)) => format!("nros {v}, pinned by {}", p.path.display()),
            (Some(v), None) => format!("nros {v}, no pin (the project floats)"),
            (None, _) => "an in-tree build (no store version)".to_string(),
        },
        chosen: false,
    });
    Ok((nros_build_paths::canonical(&root), trace))
}

/// `Some(refusal)` when the project is pinned to a version other than the
/// one running. Pure, so the refusal is unit-tested without a store.
fn pin_mismatch(
    running: Option<&str>,
    pin: Option<&nros_launcher::pin::Pin>,
    workspace: &std::path::Path,
) -> Option<String> {
    let (running, pin) = (running?, pin?);
    (running != pin.version).then(|| {
        format!(
            "nano-ros: {} is pinned to nano-ros {} ({}), but this is nros {running}, so its \
             SDK root would be the wrong version's. Run `nros locate nano-ros` inside that \
             project: there, `nros` runs the pinned toolchain, which answers.",
            workspace.display(),
            pin.version,
            pin.path.display()
        )
    })
}

/// `nros locate store` / `nros locate nano-ros`, with no index loaded.
fn run_roots(args: &Args) -> Result<()> {
    let project = project_dir(args.project.as_deref());
    let batch = args.names.len() > 1;
    let mut failed = 0usize;
    for name in &args.names {
        let answer = if name == "store" {
            let store = crate::orchestration::store::root();
            Ok((
                store.clone(),
                vec![locate::Step {
                    rung: "root",
                    saw: format!(
                        "{} ({})",
                        store.display(),
                        crate::orchestration::store::root_origin()
                    ),
                    chosen: true,
                }],
            ))
        } else {
            locate_nano_ros(project.as_deref())
        };
        match answer {
            Ok((path, trace)) => {
                match args.format {
                    Format::Path if batch => println!("{name}\t{}", path.display()),
                    Format::Path => println!("{}", path.display()),
                    Format::Sh => println!(
                        "{}='{}'",
                        var_name(name),
                        path.display().to_string().replace('\'', "'\\''")
                    ),
                    Format::Cmake => println!("set({} \"{}\")", var_name(name), path.display()),
                }
                if args.why {
                    for s in &trace {
                        eprintln!("  {:<10} {}", s.rung, s.saw);
                    }
                }
            }
            Err(e) => {
                failed += 1;
                eprintln!("{e}");
            }
        }
    }
    if failed > 0 {
        bail!("{failed} resource(s) not located");
    }
    Ok(())
}

/// `nros locate artifact <name>…` — where a cargo-built nano-ros artifact
/// lands (phase-484 W6b, RFC-0103 D7). The layout under the target dir is
/// `nros_build_paths::artifact` (the function the build scripts that WRITE
/// the headers call); WHERE the target dir is, is cargo's answer.
fn run_artifact(args: &Args, checkout: Option<&std::path::Path>) -> Result<()> {
    use nros_build_paths::artifact::Artifact;
    let names = &args.names[1..];
    if names.is_empty() {
        let known: Vec<&str> = Artifact::ALL.iter().map(|a| a.name()).collect();
        bail!("name an artifact: {}", known.join(", "));
    }
    let mut wanted = Vec::new();
    for n in names {
        let Some(a) = Artifact::parse(n) else {
            let known: Vec<&str> = Artifact::ALL.iter().map(|a| a.name()).collect();
            bail!("no artifact `{n}` — known: {}", known.join(", "));
        };
        wanted.push(a);
    }
    if args.placed_dir.is_some() {
        refuse_placed_archive(&wanted)?;
    } else {
        refuse_unplaced_header(&wanted)?;
    }
    // Asked only when something resolves under it: a header-only lookup with
    // --placed-dir must not need the cargo workspace.
    let target_dir = match (&args.target_dir, &args.placed_dir) {
        (Some(d), _) => d.clone(),
        (None, Some(p)) => p.clone(),
        (None, None) => cargo_target_dir(checkout)?,
    };
    let batch = wanted.len() > 1;
    for a in wanted {
        let root = args.placed_dir.as_deref().unwrap_or(&target_dir);
        let path = a.path(root, args.target.as_deref(), &args.profile);
        let key = format!("artifact-{}", a.name());
        match args.format {
            Format::Path if batch => println!("{}\t{}", a.name(), path.display()),
            Format::Path => println!("{}", path.display()),
            Format::Sh => println!(
                "{}='{}'",
                var_name(&key),
                path.display().to_string().replace('\'', "'\\''")
            ),
            Format::Cmake => println!("set({} \"{}\")", var_name(&key), path.display()),
        }
        if args.why {
            eprintln!(
                "  {:<10} {} ({}) + {}",
                "artifact",
                root.display(),
                if args.placed_dir.is_some() {
                    "placed dir"
                } else {
                    "cargo target dir"
                },
                a.relative(args.target.as_deref(), &args.profile).display()
            );
        }
    }
    Ok(())
}

/// `--placed-dir` holds what a build PLACED — headers. An archive is cargo's
/// output and is never placed, so resolving one there would name a file that
/// cannot exist.
fn refuse_placed_archive(wanted: &[nros_build_paths::artifact::Artifact]) -> Result<()> {
    if let Some(a) = wanted.iter().find(|a| a.per_profile()) {
        bail!(
            "`{}` is cargo's output, never placed — drop --placed-dir, which only header \
             artifacts resolve under",
            a.name()
        );
    }
    Ok(())
}

/// Issue 1824 — a config header lives in a cargo `$OUT_DIR`, and nothing writes
/// one under the target dir any more, so the target-dir join would name a file
/// that cannot exist. The answer is the dir a placed build copied it to.
fn refuse_unplaced_header(wanted: &[nros_build_paths::artifact::Artifact]) -> Result<()> {
    if let Some(a) = wanted.iter().find(|a| !a.per_profile()) {
        bail!(
            "`{}` is a per-build header, which lives in cargo's `$OUT_DIR` — pass \
             --placed-dir <dir>, the --dest of the build that placed it \
             (`scripts/build/cargo-out-dir-headers.py --package nros-c … --dest <dir> -- \
             cargo build … --message-format=json-render-diagnostics`)",
            a.name()
        );
    }
    Ok(())
}

/// `nros locate tool <name>` — a first-party host binary (issue 1850). One
/// tool today, `nros-launch-resolve`, through the ONE ladder
/// (`model_location::launch_resolver_bin_for_exe`) — the answer `nros sync`
/// itself spawns, so the shell's `nros_launch_resolve_bin` (and the identity
/// fixtures record) can never name a different binary from the one that ran.
/// "This checkout" is the one the index belongs to, else this binary's own.
///
/// phase-517 W6 — and `qemu-system-<arch>`, through
/// [`crate::orchestration::qemu_bin`]: the QEMU `nros build` prints for a QEMU
/// board. It has no `$PATH` rung, so a host with nothing provisioned is told
/// to run `nros setup --tool qemu` rather than handed the distro build.
///
/// phase-524 W4c -- and `renode`, the Nucleo-F767ZI's emulator twin, through
/// [`renode_bin`]: the one place anything asks where Renode is (`nros run`
/// hands its answer to the `renode` runner as the plan's tool, phase-524 W4b;
/// the test harness's `nros_tests::renode`, through
/// `scripts/probe/twin/renode.sh`, prints it).
fn run_tool(
    args: &Args,
    checkout: Option<&std::path::Path>,
    sdk_root: &std::path::Path,
    index: &SdkIndex,
) -> Result<()> {
    use crate::orchestration::model_location::{LAUNCH_RESOLVER, launch_resolver_bin_for_exe};
    let [_, name] = args.names.as_slice() else {
        bail!("name one tool: `nros locate tool {LAUNCH_RESOLVER}`");
    };
    // phase-517 W6 — the QEMU ladder (issue 1851), the CLI's twin of
    // `nros_qemu_bin`: what `nros build` prints for a QEMU board.
    let (path, rung) = if name.starts_with("qemu-system-") {
        use crate::orchestration::qemu_bin::{QemuBin, for_sdk, locate};
        match locate(name, &for_sdk(Some(sdk_root))) {
            QemuBin::Found { path, rung } => (path, rung),
            // `RefusedPrefix` needs `StalePrefix::Refuse`, which the CLI never
            // asks for; reported like a miss if that ever changes.
            QemuBin::Missing { why } | QemuBin::RefusedPrefix { why, .. } => bail!("{why}"),
            // The store builds no QEMU for this target: the host's, from
            // `$PATH`, is the only answer there is.
            QemuBin::Host { why } => match std::env::var_os("PATH").and_then(|p| {
                std::env::split_paths(&p)
                    .map(|d| d.join(name))
                    .find(|c| c.is_file())
            }) {
                Some(p) => (p, "host $PATH (the store builds no such target)"),
                None => bail!("{why}"),
            },
        }
    } else if name == RENODE {
        match renode_bin(index, &|k| std::env::var_os(k)) {
            Ok(found) => found,
            Err(why) => bail!("{why}"),
        }
    } else if name == LAUNCH_RESOLVER {
        let exe = std::env::current_exe()?;
        let Some(path) = launch_resolver_bin_for_exe(&exe, checkout) else {
            bail!(
                "{}",
                crate::orchestration::model_location::launch_resolver_missing_help_for_exe(
                    &exe, checkout
                )
            );
        };
        (path, "model_location::launch_resolver_from")
    } else {
        // RFC-0103 §3 — every other `[tool.*]` of the index (the cross gcc,
        // `zephyr-sdk`, …) answers its install DIRECTORY, through the one
        // resolver `nros dev sdk path` prints.
        match sdk_tool_dir(index, name) {
            Ok(dir) => (dir, "store (the pinned [tool.*] version)"),
            Err(why) => bail!("{why}"),
        }
    };
    match args.format {
        Format::Path => println!("{}", path.display()),
        Format::Sh => println!(
            "{}='{}'",
            var_name(&format!("tool-{name}")),
            path.display().to_string().replace('\'', "'\\''")
        ),
        Format::Cmake => println!(
            "set({} \"{}\")",
            var_name(&format!("tool-{name}")),
            path.display()
        ),
    }
    if args.why {
        eprintln!("  {:<10} {} ({rung})", "tool", path.display());
    }
    Ok(())
}

/// `nros locate tool <name>` for an SDK-index `[tool.<name>]` that `nros
/// setup` provisions: the directory `nros dev sdk path <name> --require`
/// answers (one resolver, [`crate::cmd::sdk_path::tool_path`]), or a refusal
/// that names `nros setup <board>`. The path is CONSTRUCTED from the pin and
/// the store is never enumerated (issue 1546), so another installed version
/// of the tool is never the answer.
fn sdk_tool_dir(index: &SdkIndex, name: &str) -> std::result::Result<PathBuf, String> {
    crate::cmd::sdk_path::tool_path(index, name, true)
        .map(|p| nros_build_paths::canonical(&p))
        .map_err(|miss| tool_miss_message(name, miss))
}

/// The refusal text for a [`ToolMiss`]. Pure, so it is unit-tested.
fn tool_miss_message(name: &str, miss: crate::cmd::sdk_path::ToolMiss) -> String {
    use crate::{cmd::sdk_path::ToolMiss, orchestration::model_location::LAUNCH_RESOLVER};
    match miss {
        ToolMiss::Unknown { known } => format!(
            "no tool `{name}`. `nros locate tool` knows {LAUNCH_RESOLVER}, {RENODE}, \
             qemu-system-<arch>, and the SDK tools `nros setup <board>` installs: {}",
            known.join(", ")
        ),
        ToolMiss::ProjectNotDir(p) => format!(
            "{name}: the project's system.toml names {} under [tools], which is not a directory",
            p.display()
        ),
        ToolMiss::NotInstalled { version, tried } => format!(
            "{name} {version} is not installed (looked at {}). Run `nros setup <board>` for a \
             board that uses it.",
            tried
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// The `[tool.*]` key (and front name) of the Renode emulator.
const RENODE: &str = "renode";

/// The environment rung of [`renode_bin`]: an absolute path to a `renode`
/// program, for a host (the CI runner image) whose copy is not the store's.
pub const RENODE_ENV: &str = "NROS_RENODE";

/// phase-524 W4c -- where Renode is, as `(path, rung)`, or why it is nowhere.
///
/// Two rungs. `$NROS_RENODE` first (the runner image installs the pinned
/// archive outside the store, which is a volume, and names it here); then the
/// store at the PINNED `[tool.renode]` version, `<store>/renode/<version>/
/// <subdir>/renode`, which `nros setup --tool renode` (or `nros setup
/// nucleo-f767zi-zephyr`) writes. No `$PATH` rung: a `renode` on PATH is
/// whatever the host has, and the twin was measured with the pinned build
/// only. A named path that is not a file is refused, never skipped, so a
/// mistyped override cannot fall through to a different emulator.
pub fn renode_bin(
    index: &SdkIndex,
    getenv: &dyn Fn(&str) -> Option<std::ffi::OsString>,
) -> std::result::Result<(PathBuf, &'static str), String> {
    if let Some(v) = getenv(RENODE_ENV).filter(|v| !v.is_empty()) {
        let p = PathBuf::from(v);
        return if p.is_file() {
            Ok((p, "$NROS_RENODE"))
        } else {
            Err(format!(
                "${RENODE_ENV} names {}, which is not a file",
                p.display()
            ))
        };
    }
    let Some(tool) = index.tool.get(RENODE) else {
        return Err(format!("the index declares no [tool.{RENODE}]"));
    };
    let front = tool.front.first().map_or(RENODE, String::as_str);
    let Some(dir) = crate::orchestration::sdk_store::tool_dir(index, RENODE) else {
        return Err(format!("the index declares no [tool.{RENODE}]"));
    };
    let bin = dir.join(front);
    if bin.is_file() {
        Ok((bin, "store (pinned [tool.renode])"))
    } else {
        Err(format!(
            "Renode {} is not installed (looked for {}, and ${RENODE_ENV} is unset); \
             `nros setup --tool {RENODE}` installs it",
            tool.version,
            bin.display()
        ))
    }
}

/// The nano-ros workspace's cargo target dir, as cargo resolves it — so
/// `CARGO_TARGET_DIR` and a `[build] target-dir` are honoured exactly as the
/// build that produced the artifact honoured them.
fn cargo_target_dir(checkout: Option<&std::path::Path>) -> Result<PathBuf> {
    let Some(root) = checkout else {
        bail!(
            "`nros locate artifact` needs the nano-ros cargo workspace, and this index \
             belongs to no checkout — pass --target-dir"
        );
    };
    let out =
        std::process::Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .args(["metadata", "--format-version=1", "--no-deps"])
            .arg("--manifest-path")
            .arg(root.join("Cargo.toml"))
            .output()?;
    if !out.status.success() {
        bail!(
            "`cargo metadata` failed in {}: {}",
            root.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    meta.get("target_directory")
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .ok_or_else(|| eyre::eyre!("`cargo metadata` reported no target_directory"))
}

#[cfg(test)]
mod tests {
    use super::{
        RENODE_ENV, pin_mismatch, refuse_placed_archive, refuse_unplaced_header, renode_bin,
        var_name,
    };

    fn pin(v: &str) -> nros_launcher::pin::Pin {
        nros_launcher::pin::Pin {
            path: std::path::PathBuf::from("/p/nros-toolchain.toml"),
            version: v.to_string(),
        }
    }

    #[test]
    fn sdk_root_answers_only_for_the_pinned_version() {
        let ws = std::path::Path::new("/p");
        // The project's pinned toolchain is the one running: answer.
        assert_eq!(pin_mismatch(Some("0.7.0"), Some(&pin("0.7.0")), ws), None);
        // No pin (the project floats), or an in-tree build: answer.
        assert_eq!(pin_mismatch(Some("0.7.0"), None, ws), None);
        assert_eq!(pin_mismatch(None, Some(&pin("0.6.0")), ws), None);
        // Another project pinned to another version: refuse, naming both.
        let e = pin_mismatch(Some("0.7.0"), Some(&pin("0.6.0")), ws).expect("refuses");
        assert!(
            e.contains("0.6.0") && e.contains("0.7.0") && e.contains("nros-toolchain.toml"),
            "{e}"
        );
    }
    use nros_build_paths::artifact::Artifact;

    /// phase-524 W4c -- the env rung wins when it names a file, and a name that
    /// is not a file is REFUSED rather than falling through to the store (a
    /// mistyped override must not run a different emulator).
    #[test]
    fn renode_env_rung_is_taken_or_refused_never_skipped() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let index =
            crate::orchestration::sdk_index::SdkIndex::load(&root.join("nros-sdk-index.toml"))
                .expect("load the checkout's index");
        // Any existing file stands in for the program: the rung asks only that.
        let file = root.join("nros-sdk-index.toml");
        let named = file.clone().into_os_string();
        let (path, rung) = renode_bin(&index, &|k| (k == RENODE_ENV).then(|| named.clone()))
            .expect("an existing file is taken");
        assert_eq!(path, file);
        assert_eq!(rung, "$NROS_RENODE");

        let missing = std::ffi::OsString::from("/nonexistent/nros-renode-probe/renode");
        let err = renode_bin(&index, &|k| (k == RENODE_ENV).then(|| missing.clone()))
            .expect_err("a missing file is refused");
        assert!(err.contains("not a file"), "{err}");
    }

    #[test]
    fn placed_dir_takes_headers_and_refuses_archives() {
        let headers = [
            Artifact::CInclude,
            Artifact::CppInclude,
            Artifact::CConfigHeader,
            Artifact::CppConfigHeader,
        ];
        assert!(refuse_placed_archive(&headers).is_ok());
        for a in [Artifact::CArchive, Artifact::CppArchive] {
            let err = refuse_placed_archive(&[Artifact::CInclude, a]).unwrap_err();
            assert!(err.to_string().contains(a.name()), "{err}");
        }
        // Issue 1824 — and the converse: without a placed dir, an archive
        // resolves and a header is refused (there is no target-dir copy).
        assert!(refuse_unplaced_header(&[Artifact::CArchive, Artifact::CppArchive]).is_ok());
        for h in headers {
            let err = refuse_unplaced_header(&[Artifact::CArchive, h]).unwrap_err();
            assert!(err.to_string().contains(h.name()), "{err}");
        }
    }

    fn checkout_index() -> crate::orchestration::sdk_index::SdkIndex {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        crate::orchestration::sdk_index::SdkIndex::load(&root.join("nros-sdk-index.toml"))
            .expect("load the checkout's index")
    }

    /// A pinned tool with no install is a refusal that names the pin, the
    /// paths tried and `nros setup <board>`: never another version (issue
    /// 1546). The found case runs through the binary in `nros-cli/tests`.
    #[test]
    fn missing_sdk_tool_names_the_pin_and_setup() {
        let tried = std::path::PathBuf::from("/s/sdk/zephyr-sdk/0.16.8/zephyr-sdk-0.16.8");
        let err = super::tool_miss_message(
            "zephyr-sdk",
            crate::cmd::sdk_path::ToolMiss::NotInstalled {
                version: "0.16.8".into(),
                tried: vec![tried.clone()],
            },
        );
        assert!(err.contains("zephyr-sdk 0.16.8 is not installed"), "{err}");
        assert!(err.contains(&tried.display().to_string()), "{err}");
        assert!(err.contains("nros setup <board>"), "{err}");
    }

    #[test]
    fn unknown_tool_names_setup_and_the_known_tools() {
        let index = checkout_index();
        let err = super::sdk_tool_dir(&index, "no-such-tool").expect_err("unknown");
        assert!(err.contains("no tool `no-such-tool`"), "{err}");
        assert!(err.contains("nros setup <board>"), "{err}");
        assert!(
            err.contains("zephyr-sdk"),
            "the index's tools are listed: {err}"
        );
    }

    #[test]
    fn variable_names_are_mechanical() {
        assert_eq!(var_name("zenoh-pico"), "NROS_LOCATE_ZENOH_PICO");
        assert_eq!(
            var_name("micro-xrce-dds-client"),
            "NROS_LOCATE_MICRO_XRCE_DDS_CLIENT"
        );
    }
}
