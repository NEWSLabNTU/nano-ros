//! `nros new <name>` — Phase 111.A.4.
//!
//! Forwards to `cargo_nano_ros::scaffold::scaffold_package` so the CLI
//! stays in lockstep with the shared scaffolding implementation.
//! Use-case (`talker` / `listener` / `service` / `action`) and RMW-choice
//! diversification are accepted at the CLI for forward-compat but
//! currently affect only the printed "Next steps" banner — full
//! per-use-case template trees land alongside the Phase 112 example
//! sweep.

use cargo_nano_ros::scaffold::{
    ComponentScaffoldConfig, ScaffoldConfig, scaffold_component, scaffold_package,
};
use clap::Args as ClapArgs;
use eyre::{Result, bail};
use std::path::PathBuf;

use crate::cmd::{
    new_system::{BringupScaffold, scaffold_bringup},
    scaffold_deploy::{DeployScaffold, ScaffoldTable, scaffold_deploy},
};

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Project directory to create (project mode), or the literal keyword
    /// `system` to enter Phase 212.F bringup-scaffold mode: `nros new system
    /// <name>_bringup --components <pkg1,pkg2,...>`.
    pub name: Option<PathBuf>,

    /// Phase 212.F bringup-scaffold mode — the bringup package directory.
    /// Only consumed when the first positional is the literal `system`.
    /// phase-290 W4.b — also the <name> when the first positional is the
    /// literal `platform` or `board` (package-scaffold modes).
    pub system_name: Option<PathBuf>,

    /// phase-290 W4.b — the platform a `nros new board <name>` targets
    /// (a platform directory name, e.g. `zephyr`, `bare-metal`, or one
    /// scaffolded via `nros new platform`).
    #[arg(long = "for-platform")]
    pub for_platform: Option<String>,

    /// Phase 212.F — comma-separated component package names for
    /// `nros new system <bringup> --components <list>`.
    #[arg(long, value_delimiter = ',')]
    pub components: Vec<String>,

    /// Phase 212.F — repeatable single-component form (alternative to
    /// `--components <a,b,c>` when commas in the shell are awkward). Merged
    /// with `--components` at dispatch time.
    #[arg(long = "component-name")]
    pub component_name: Vec<String>,

    /// Phase 212.F — workspace root holding the cargo `Cargo.toml` to
    /// update. Defaults to the parent of the bringup dir.
    #[arg(long)]
    pub workspace_root: Option<PathBuf>,

    /// Phase 212.F — parent dir under which the bringup pkg is created.
    /// Defaults to the current working directory.
    #[arg(long)]
    pub into: Option<PathBuf>,

    /// Phase 212.F — skip the optional `config/` sub-dir.
    #[arg(long)]
    pub no_config: bool,

    /// Phase 212.F — skip the optional `README.md`.
    #[arg(long)]
    pub no_readme: bool,

    /// Target platform (required in project mode)
    #[arg(long, value_parser = ["native", "freertos", "nuttx", "threadx", "zephyr", "esp32", "posix", "baremetal"])]
    pub platform: Option<String>,

    /// RMW backend. Defaults per mode: `zenoh` for project/component
    /// scaffolds (matches the tracked examples), `cyclonedds` for
    /// `--workspace` (needs no router — the quick-start default).
    #[arg(long, value_parser = ["zenoh", "xrce", "cyclonedds"])]
    pub rmw: Option<String>,

    /// ROS edition (drives the `ros-<edition>` cargo feature; RFC-0056)
    #[arg(long = "ros-edition", value_parser = ["humble", "iron", "jazzy"], default_value = "humble")]
    pub ros_edition: String,

    /// Source language. Defaults per mode: `rust` for project/component
    /// scaffolds, `cpp` for `--workspace` (the quick-start language).
    #[arg(long, value_parser = ["rust", "c", "cpp"])]
    pub lang: Option<String>,

    /// Use case template
    #[arg(long = "use-case", value_parser = ["talker", "listener", "service", "action"], default_value = "talker")]
    pub use_case: String,

    /// Phase 172 W.3 — scaffold a planned-mode **component** (a reusable
    /// library node with an `nros::Component` + a folded `[component]`
    /// `nros.toml`) instead of a direct-mode binary project. Platform-agnostic
    /// (platform/RMW are chosen at deploy time), so `--platform` is not needed.
    #[arg(long)]
    pub component: bool,

    /// Scaffold an `[image.<name>]` — a buildable image — into the bringup
    /// package's `system.toml` (RFC-0065 D6) instead of a project.
    #[arg(long)]
    pub image: Option<String>,

    /// Scaffold a `[host.<name>]` — a machine nodes run on — into the bringup
    /// package's `system.toml`.
    #[arg(long)]
    pub host: Option<String>,

    /// DEPRECATED (issue 0951): `[deploy.*]` split into `[image.*]` (what is
    /// BUILT) and `[host.*]` (WHERE it runs). Still accepted, and dispatched
    /// by `--kind`: `self` scaffolds a host, anything else an image.
    #[arg(long)]
    pub deploy: Option<String>,

    /// phase-368 W8 — scaffold a minimal multi-node WORKSPACE (node pkgs +
    /// bringup + entry) instead of a standalone project: the canonical
    /// copy-out template with the RMW choice baked into every file that
    /// spells it. Defaults: `--lang cpp --rmw cyclonedds`.
    #[arg(long)]
    pub workspace: bool,

    /// DEPRECATED (issue 0951) — only read by `--deploy`, to decide whether it
    /// meant a machine (`self`) or a board build (anything else). `--image` and
    /// `--host` say which directly.
    #[arg(long, default_value = "self")]
    pub kind: String,

    /// DEPRECATED (issue 0951): the rustc triple comes from the board
    /// descriptor, so an image carries none. Accepted and ignored.
    #[arg(long)]
    pub target: Option<String>,

    /// Board this image is built for (`--image` mode).
    #[arg(long)]
    pub board: Option<String>,

    /// Deploy mode: pick the bringup package whose `system.toml` to edit
    /// when the workspace exposes more than one.
    #[arg(long)]
    pub bringup: Option<String>,

    /// Deploy mode: also set the bringup `[system].default_launch` (bootstrap)
    #[arg(long)]
    pub from_launch: Option<String>,

    /// Fork an existing block of the same table (`[image.<name>]` with
    /// `--image`, `[host.<name>]` with `--host`).
    #[arg(long)]
    pub from_profile: Option<String>,

    /// Overwrite an existing directory / `[deploy.<name>]` table
    #[arg(long)]
    pub force: bool,
}

pub fn run(args: Args) -> Result<()> {
    // phase-290 W4.b — package-scaffold modes: `nros new platform <name>` /
    // `nros new board <name> --for-platform <p>`.
    let keyword = args.name.as_ref().and_then(|p| p.to_str());
    if keyword == Some("platform") || keyword == Some("board") {
        let pkg_name = args
            .system_name
            .as_ref()
            .and_then(|p| p.to_str())
            .ok_or_else(|| eyre::eyre!("`nros new {} <name>` requires a name", keyword.unwrap()))?
            .to_string();
        let into = args
            .into
            .clone()
            .map(Ok)
            .unwrap_or_else(std::env::current_dir)?;
        return if keyword == Some("platform") {
            crate::cmd::new_platform::scaffold_platform(&pkg_name, &into)
        } else {
            let platform = args.for_platform.clone().ok_or_else(|| {
                eyre::eyre!("`nros new board <name>` requires --for-platform <platform>")
            })?;
            crate::cmd::new_platform::scaffold_board(&pkg_name, &platform, &into)
        };
    }

    // `nros new node <name>` — a NODE package. Board- and RMW-agnostic; see
    // `new_node`'s module docs for why `--platform native` is not the way to
    // make one.
    if args
        .name
        .as_ref()
        .and_then(|p| p.to_str())
        .map(|s| s == "node")
        .unwrap_or(false)
    {
        let node_name = args
            .system_name
            .as_ref()
            .and_then(|p| p.to_str())
            .ok_or_else(|| eyre::eyre!("`nros new node <name>` requires a package name"))?
            .to_string();
        crate::cmd::new_entry::validate_entry_name(&node_name)?;

        let cwd = std::env::current_dir()?;
        let ws_root = args.workspace_root.clone().unwrap_or_else(|| cwd.clone());
        let into = args.into.clone().unwrap_or_else(|| ws_root.join("src"));
        // A bringup is optional here: a node package written before any system
        // exists is a legitimate order to work in, and refusing it would make
        // the scaffolds usable in exactly one sequence.
        let bringup_dir = match args.bringup.as_deref() {
            Some(b) => {
                let p = PathBuf::from(b);
                Some(if p.is_absolute() {
                    p
                } else if ws_root.join(b).join("system.toml").is_file() {
                    ws_root.join(b)
                } else {
                    ws_root.join("src").join(b)
                })
            }
            None => crate::cmd::new_entry::sole_bringup(&ws_root.join("src")).ok(),
        };

        let out = crate::cmd::new_node::scaffold_node(&crate::cmd::new_node::NodeScaffold {
            node_dir: into.join(&node_name),
            bringup_dir,
        })?;

        println!(
            "nros new node: scaffolded {} ({} file(s))",
            out.node_dir.display(),
            out.files.len()
        );
        match &out.declared_in {
            Some(p) => println!("  declared [[component]] in {}", p.display()),
            None => println!(
                "  no bringup found, so nothing declares it yet — add a \
                 [[component]] row, or pass --bringup <dir>"
            ),
        }
        return Ok(());
    }

    // `nros new entry <name> --platform zephyr` — a framework ENTRY package,
    // which is not a standalone project (see `new_entry`'s module docs for why
    // it is a separate noun rather than a `--platform` value on the form
    // above).
    if args
        .name
        .as_ref()
        .and_then(|p| p.to_str())
        .map(|s| s == "entry")
        .unwrap_or(false)
    {
        let entry_name = args
            .system_name
            .as_ref()
            .and_then(|p| p.to_str())
            .ok_or_else(|| eyre::eyre!("`nros new entry <name>` requires a package name"))?
            .to_string();
        crate::cmd::new_entry::validate_entry_name(&entry_name)?;

        let platform = args
            .platform
            .clone()
            .unwrap_or_else(|| "zephyr".to_string());
        if platform != "zephyr" {
            bail!(
                "`nros new entry` currently scaffolds Zephyr entries only \
                 (got --platform {platform}).\n  \
                 Every other platform builds through cargo or cmake, where the \
                 entry is GENERATED from the image and needs no package of its \
                 own (RFC-0065 D3)."
            );
        }

        let cwd = std::env::current_dir()?;
        let ws_root = args.workspace_root.clone().unwrap_or_else(|| cwd.clone());
        let into = args.into.clone().unwrap_or_else(|| ws_root.join("src"));
        let bringup_dir = match args.bringup.as_deref() {
            Some(b) => {
                let p = PathBuf::from(b);
                if p.is_absolute() {
                    p
                } else if ws_root.join(b).join("system.toml").is_file() {
                    ws_root.join(b)
                } else {
                    // A bare package name is the natural spelling — `--bringup
                    // demo_bringup` rather than `--bringup src/demo_bringup`.
                    ws_root.join("src").join(b)
                }
            }
            None => crate::cmd::new_entry::sole_bringup(&ws_root.join("src"))?,
        };

        let out = crate::cmd::new_entry::scaffold_entry(&crate::cmd::new_entry::EntryScaffold {
            entry_dir: into.join(&entry_name),
            bringup_dir: bringup_dir.clone(),
            workspace_root: ws_root,
            board: args
                .board
                .clone()
                .unwrap_or_else(|| "native_sim/native/64".to_string()),
            rmw: args.rmw.clone().unwrap_or_else(|| "zenoh".to_string()),
        })?;

        println!(
            "nros new entry: scaffolded {} ({} file(s))",
            out.entry_dir.display(),
            out.files.len()
        );
        let bringup_name = bringup_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("<bringup>");
        println!(
            "  declared [image.{}] in {}/system.toml",
            out.image_id,
            bringup_dir.display()
        );
        println!("\nNext:");
        println!("  nros sync");
        println!(
            "  nros build {bringup_name}:{} --zephyr-workspace <dir>",
            out.image_id
        );
        return Ok(());
    }

    // Phase 212.F — system / bringup mode: `nros new system <name>_bringup
    // --components <list>`. The literal `system` keyword as the first
    // positional dispatches here.
    if args
        .name
        .as_ref()
        .and_then(|p| p.to_str())
        .map(|s| s == "system")
        .unwrap_or(false)
    {
        let bringup_path = args.system_name.clone().ok_or_else(|| {
            eyre::eyre!("`nros new system <name>_bringup` requires a bringup pkg name")
        })?;
        // Phase 212.F: validate the user-supplied name early so
        // `foo/bar`, `..`, absolute paths surface a clean diagnostic
        // before we touch the filesystem.
        crate::cmd::new_system::validate_bringup_name(&bringup_path)?;
        // Merge --components <a,b,c> with repeatable --component-name <x>.
        let mut components: Vec<String> = args.components.clone();
        components.extend(args.component_name.clone());
        if components.is_empty() {
            bail!(
                "`nros new system <bringup>` requires --components <pkg1,pkg2,...> \
                 (at least one component); --component-name <x> may be repeated as an alternative"
            );
        }
        let cwd = std::env::current_dir()?;
        // --into <dir> overrides cwd as the parent directory for the bringup.
        let into = args.into.clone().unwrap_or_else(|| cwd.clone());
        let bringup_dir = if bringup_path.is_absolute() {
            bringup_path
        } else {
            into.join(&bringup_path)
        };
        let pkg_name = bringup_dir
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| eyre::eyre!("invalid bringup package name"))?
            .to_string();
        let workspace_root = args
            .workspace_root
            .clone()
            .or_else(|| bringup_dir.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| cwd.clone());
        let out = scaffold_bringup(&BringupScaffold {
            bringup_dir: bringup_dir.clone(),
            pkg_name: pkg_name.clone(),
            components: components.clone(),
            workspace_root,
            emit_config: !args.no_config,
            emit_readme: !args.no_readme,
            force: args.force,
        })?;
        eprintln!(
            "nros new system: scaffolded bringup pkg {pkg_name} at {} ({} component(s))",
            out.bringup_dir.display(),
            components.len()
        );
        if let Some(ws) = out.workspace_cargo_toml.as_ref() {
            eprintln!(
                "nros new system: updated [workspace] exclude in {}",
                ws.display()
            );
        }
        let _ = out; // silence unused warning under future changes
        return Ok(());
    }

    // Issue 0951 — `[deploy.*]` split into `[image.*]` (what is BUILT) and
    // `[host.*]` (WHERE it runs), so the scaffolder asks which one.
    //
    // `--deploy` still works, dispatched by the `--kind` it already took:
    // `self` was always a machine and everything else a board build. That is
    // the same meaning the old flag had, routed to the table that now holds it,
    // rather than a rename that would quietly write the wrong one.
    let scaffold = match (&args.image, &args.host, &args.deploy) {
        (Some(_), Some(_), _) | (Some(_), _, Some(_)) | (_, Some(_), Some(_)) => {
            eyre::bail!("--image, --host and --deploy are alternatives; pass one")
        }
        (Some(name), None, None) => Some((name.clone(), ScaffoldTable::Image)),
        (None, Some(name), None) => Some((name.clone(), ScaffoldTable::Host)),
        (None, None, Some(name)) => {
            let table = if args.kind == "self" {
                ScaffoldTable::Host
            } else {
                ScaffoldTable::Image
            };
            let replacement = match table {
                ScaffoldTable::Host => "--host",
                ScaffoldTable::Image => "--image",
            };
            eprintln!(
                "nros new: --deploy is deprecated (issue 0951) — `[deploy.*]` split \
                 into `[image.*]` (what is BUILT) and `[host.*]` (WHERE it runs). \
                 With --kind {}, this scaffolds {} block; use `{replacement}` \
                 directly.",
                args.kind,
                match table {
                    ScaffoldTable::Image => "an [image.*]",
                    ScaffoldTable::Host => "a [host.*]",
                },
            );
            Some((name.clone(), table))
        }
        (None, None, None) => None,
    };
    if let Some((name, table)) = scaffold {
        return scaffold_deploy(&DeployScaffold {
            name,
            table,
            target: args.target,
            board: args.board,
            from_launch: args.from_launch,
            from_profile: args.from_profile,
            workspace_root: std::env::current_dir()?,
            bringup: args.bringup,
            force: args.force,
        });
    }

    let name = args
        .name
        .as_ref()
        .ok_or_else(|| eyre::eyre!("`nros new <name>` requires a project name"))?
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| eyre::eyre!("invalid project name"))?
        .to_string();

    // phase-368 W8 — workspace mode: `nros new <name> --workspace`.
    if args.workspace {
        let lang = args.lang.clone().unwrap_or_else(|| "cpp".to_string());
        let rmw = args.rmw.clone().unwrap_or_else(|| "cyclonedds".to_string());
        if args.platform.as_deref().unwrap_or("native") != "native" {
            bail!(
                "`nros new --workspace` scaffolds the native workspace shape; \
                 add embedded deploys afterwards with `nros new --deploy <name> \
                 --board <board>` (see the book's Growing Your Project section)."
            );
        }
        return cargo_nano_ros::workspace_scaffold::scaffold_workspace(
            &cargo_nano_ros::workspace_scaffold::WorkspaceScaffold {
                dir: PathBuf::from(&name),
                lang,
                rmw,
                force: args.force,
            },
        );
    }

    // Component mode (Phase 172 W.3): a reusable planned-mode library node.
    // Platform-agnostic. Phase 172 W.3 landed Rust; Phase 219.M landed C++;
    // Phase 223 adds the C Node pkg scaffold using the same declarative
    // §212.L.9 shape.
    if args.component {
        let lang = args.lang.clone().unwrap_or_else(|| "rust".to_string());
        match lang.as_str() {
            "rust" | "cpp" | "c" => {}
            other => bail!(
                "`nros new --component --lang {other}` is not supported. Use \
                 `rust`, `c`, or `cpp`."
            ),
        }
        return scaffold_component(&ComponentScaffoldConfig {
            name,
            use_case: args.use_case,
            lang,
            force: args.force,
        });
    }

    // Project mode.
    let platform = args
        .platform
        .ok_or_else(|| eyre::eyre!("`nros new <name>` requires `--platform <p>`"))?;
    scaffold_package(&ScaffoldConfig {
        name,
        lang: args.lang.unwrap_or_else(|| "rust".to_string()),
        platform,
        rmw: args.rmw.unwrap_or_else(|| "zenoh".to_string()),
        ros_edition: args.ros_edition,
        use_case: args.use_case,
        force: args.force,
    })
}
