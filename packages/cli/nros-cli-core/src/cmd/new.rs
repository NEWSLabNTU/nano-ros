//! `nros new <name>` — Phase 111.A.4.
//!
//! Forwards to `cargo_nano_ros::scaffold::scaffold_package` so the CLI
//! stays in lockstep with the shared scaffolding implementation.
//!
//! The node is the `--template` role (phase-517 W12), and the package flags
//! mirror `ros2 pkg create`: `--node-name`, `--dependencies`, `--license`,
//! `--description`, `--maintainer-name`, `--maintainer-email` and
//! `--destination-directory`. `--build-type` is not taken (`--lang` decides
//! it) and neither is `--library-name` (`--component` is the library shape).

use cargo_nano_ros::scaffold::{
    ComponentScaffoldConfig, PackageMeta, Role, ScaffoldConfig, scaffold_component_at,
    scaffold_package_at,
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
    /// Project directory to create, or a mode keyword.
    ///
    /// The literal keyword `system` enters bringup-scaffold mode: `nros new
    /// system <name>_bringup --components <pkg1,pkg2,...>`.
    // Bringup-scaffold mode is Phase 212.F.
    pub name: Option<PathBuf>,

    /// The bringup package directory, or the package name in a package-scaffold mode.
    ///
    /// The bringup directory when the first positional is the literal
    /// `system`; the <name> when it is the literal `platform` or `board`.
    // Bringup-scaffold mode is Phase 212.F; the `platform` / `board`
    // package-scaffold modes are phase-290 W4.b.
    pub system_name: Option<PathBuf>,

    /// The platform a `nros new board <name>` targets.
    ///
    /// A platform directory name, e.g. `zephyr`, `bare-metal`, or one
    /// scaffolded via `nros new platform`.
    // phase-290 W4.b.
    #[arg(long = "for-platform")]
    pub for_platform: Option<String>,

    /// Comma-separated component package names for
    /// `nros new system <bringup> --components <list>`.
    // Phase 212.F.
    #[arg(long, value_delimiter = ',')]
    pub components: Vec<String>,

    /// One component package name; repeatable.
    ///
    /// An alternative to `--components <a,b,c>` when commas in the shell are
    /// awkward. Merged with `--components`.
    // Phase 212.F. Merged at dispatch time.
    #[arg(long = "component-name")]
    pub component_name: Vec<String>,

    /// Workspace root holding the cargo `Cargo.toml` to update.
    ///
    /// Defaults to the parent of the bringup dir.
    // Phase 212.F.
    #[arg(long)]
    pub workspace_root: Option<PathBuf>,

    /// Parent dir under which the bringup package is created.
    ///
    /// Defaults to the current working directory.
    // Phase 212.F.
    #[arg(long)]
    pub into: Option<PathBuf>,

    /// Skip the optional `config/` sub-dir.
    // Phase 212.F.
    #[arg(long)]
    pub no_config: bool,

    /// Skip the optional `README.md`.
    // Phase 212.F.
    #[arg(long)]
    pub no_readme: bool,

    /// Target platform (required in project mode).
    ///
    /// `linux` is the host board. `esp32` is not offered: ESP32 support is
    /// dormant.
    // ESP32 dormancy is issue 1525 — the scaffold code is kept, the front door
    // is closed.
    //
    // `linux` is the host board (phase-500 W1). `native` and `posix` were
    // deprecated aliases for it until phase-500 W5; they are still PARSED
    // (hidden from `--help`) only so `run` can refuse them naming `linux`,
    // instead of clap's bare "invalid value".
    #[arg(long, value_parser = platform_values())]
    pub platform: Option<String>,

    /// RMW backend. Defaults per mode: `zenoh` for project/component
    /// scaffolds (matches the tracked examples), `cyclonedds` for
    /// `--workspace` (needs no router — the quick-start default).
    #[arg(long, value_parser = ["zenoh", "xrce", "cyclonedds"])]
    pub rmw: Option<String>,

    /// ROS edition (drives the `ros-<edition>` cargo feature)
    // RFC-0056.
    #[arg(long = "ros-edition", value_parser = ["humble", "iron", "jazzy"], default_value = rosidl_codegen::DEFAULT_ROS_EDITION)]
    pub ros_edition: String,

    /// Source language. Defaults per mode: `rust` for project/component
    /// scaffolds, `cpp` for `--workspace` (the quick-start language).
    #[arg(long, value_parser = ["rust", "c", "cpp"])]
    pub lang: Option<String>,

    /// The node the package holds: a talker (std_msgs/String on /chatter), a
    /// listener, an AddTwoInts `server` / `client`, or a Fibonacci
    /// `action-server` / `action-client` — the roles of the ROS demos, under
    /// the demos' own interfaces. Default: `talker`.
    #[arg(long, value_parser = template_values())]
    pub template: Option<String>,

    /// Retired: use `--template`.
    // RETIRED (phase-517 W12). Parsed, hidden, so it is refused naming its
    // replacement instead of with clap's bare "unexpected argument".
    #[arg(long = "use-case", hide = true)]
    pub use_case: Option<String>,

    /// The node's ROS name (as `ros2 pkg create --node-name`). Default: the
    /// package name (`-` spelled `_`); for `--component`, the role's demo node
    /// name (`talker`, `add_two_ints_server`, …).
    #[arg(long = "node-name")]
    pub node_name: Option<String>,

    /// Extra `<depend>` packages for package.xml, after the template's own
    /// interface package (as `ros2 pkg create --dependencies`). Space- or
    /// comma-separated.
    #[arg(long, num_args = 1.., value_delimiter = ',')]
    pub dependencies: Vec<String>,

    /// package.xml `<license>` (as `ros2 pkg create --license`). Default:
    /// `Apache-2.0`.
    #[arg(long)]
    pub license: Option<String>,

    /// package.xml `<description>` (as `ros2 pkg create --description`).
    #[arg(long)]
    pub description: Option<String>,

    /// package.xml maintainer name (as `ros2 pkg create --maintainer-name`).
    /// Default: `git config user.name`.
    #[arg(long = "maintainer-name")]
    pub maintainer_name: Option<String>,

    /// package.xml maintainer email (as `ros2 pkg create --maintainer-email`).
    /// Default: `git config user.email`.
    #[arg(long = "maintainer-email")]
    pub maintainer_email: Option<String>,

    /// Directory to create the package in (as `ros2 pkg create
    /// --destination-directory`). Default: the current directory.
    #[arg(long = "destination-directory")]
    pub destination_directory: Option<PathBuf>,

    /// Scaffold a planned-mode component instead of a direct-mode binary project.
    ///
    /// A component is a reusable library node with an `nros::Component` and a
    /// folded `[component]` `nros.toml`. It is platform-agnostic (platform and
    /// RMW are chosen at deploy time), so `--platform` is not needed.
    // Phase 172 W.3.
    #[arg(long)]
    pub component: bool,

    /// Scaffold an `[image.<name>]` — a buildable image — into the bringup
    /// package's `system.toml` instead of a project.
    // RFC-0065 D6.
    #[arg(long)]
    pub image: Option<String>,

    /// Scaffold a `[host.<name>]` — a machine nodes run on — into the bringup
    /// package's `system.toml`.
    #[arg(long)]
    pub host: Option<String>,

    /// Deprecated: use `--image` or `--host`.
    ///
    /// `[deploy.*]` split into `[image.*]` (what is built) and `[host.*]`
    /// (where it runs). Still accepted, and dispatched by `--kind`: `self`
    /// scaffolds a host, anything else an image.
    // DEPRECATED by issue 0951.
    #[arg(long)]
    pub deploy: Option<String>,

    /// Scaffold a minimal multi-node workspace instead of a standalone project.
    ///
    /// The workspace holds node packages, a bringup and an entry: the
    /// canonical copy-out template with the RMW choice baked into every file
    /// that spells it. Defaults: `--lang cpp --rmw cyclonedds`.
    // phase-368 W8.
    #[arg(long)]
    pub workspace: bool,

    /// Deprecated: use `--image` or `--host`.
    ///
    /// Only read by `--deploy`, to decide whether it meant a machine (`self`)
    /// or a board build (anything else). `--image` and `--host` say which
    /// directly.
    // DEPRECATED by issue 0951.
    #[arg(long, default_value = "self")]
    pub kind: String,

    /// Deprecated: accepted and ignored.
    ///
    /// The rustc triple comes from the board descriptor, so an image carries
    /// none.
    // DEPRECATED by issue 0951.
    #[arg(long)]
    pub target: Option<String>,

    /// Board this image is built for (`--image` mode), or the board a C/C++
    /// project scaffold's image names. A project defaults to the platform's
    /// own board from the board catalog (`freertos` -> `mps2-an385-freertos`)
    /// and refuses, naming the candidates, when the platform has several
    /// (`threadx`). Not taken by a Rust project, whose board key is fixed by
    /// its template.
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

    /// Overwrite an existing directory / `[image.<name>]` or `[host.<name>]` table
    #[arg(long)]
    pub force: bool,
}

/// `--lang`, parsed ONCE, with each mode's own default (phase-469).
///
/// The three scaffold modes had three copies of this — two
/// `unwrap_or_else(|| "<default>".to_string())` and one that also re-validated
/// the value against a `match` on three literals. All three then handed a
/// String to a scaffolder that matched on it again and carried its own
/// "Unknown language" refusal.
///
/// The accepted SET is unchanged, and this is why: clap already restricts
/// `--lang` with `value_parser = ["rust", "c", "cpp"]`, so nothing outside that
/// set ever reaches here — the parse cannot widen what a user may type, and the
/// refusals it replaces were unreachable. [`nros_lang::Language::parse`] also
/// takes `c++`/`cxx`, which clap rejects first; if the flag's own list ever
/// grows to match, it grows THERE, in one place a reader can see.
fn language_arg(flag: Option<&str>, default: nros_lang::Language) -> Result<nros_lang::Language> {
    match flag {
        Some(s) => nros_lang::Language::parse(s).map_err(|e| eyre::eyre!("nros new --lang: {e}")),
        None => Ok(default),
    }
}

/// The `--template` values: the roles, in [`Role::ALL`] order.
fn template_values() -> clap::builder::PossibleValuesParser {
    clap::builder::PossibleValuesParser::new(Role::ALL.map(Role::name))
}

/// phase-517 W12 — `--use-case` is REFUSED, naming `--template`. It accepted
/// `talker`/`listener`/`service`/`action` and wrote a talker for every one, so
/// mapping it silently would keep a word whose only behaviour was a TODO.
fn refuse_use_case(use_case: Option<&str>) -> Result<()> {
    let Some(u) = use_case else { return Ok(()) };
    let now = match u {
        "service" => "`--template server` (or `--template client`)".to_string(),
        "action" => "`--template action-server` (or `--template action-client`)".to_string(),
        other if Role::parse(other).is_some() => format!("`--template {other}`"),
        _ => "`--template <role>`".to_string(),
    };
    bail!(
        "nros new: `--use-case` is retired: write {now}. The roles are {}.",
        Role::ALL.map(Role::name).join(", ")
    );
}

/// The `--template` role, the node name and the package metadata a package or
/// component scaffold takes.
fn package_args(args: &Args) -> Result<(Role, Option<String>, PackageMeta)> {
    let role = match args.template.as_deref() {
        // clap has already restricted the value to `Role::ALL`.
        Some(t) => Role::parse(t).ok_or_else(|| eyre::eyre!("nros new: unknown --template {t}"))?,
        None => Role::Talker,
    };
    let dependencies = args
        .dependencies
        .iter()
        .flat_map(|d| d.split_whitespace())
        .map(str::to_string)
        .collect();
    Ok((
        role,
        args.node_name.clone(),
        PackageMeta {
            description: args.description.clone(),
            license: args.license.clone(),
            maintainer_name: args.maintainer_name.clone(),
            maintainer_email: args.maintainer_email.clone(),
            dependencies,
        },
    ))
}

/// The package flags, set: a mode that writes no package (`--workspace`,
/// `--image`, `nros new node`, …) refuses them rather than ignore them.
fn package_flags_given(args: &Args) -> Vec<&'static str> {
    let mut v = Vec::new();
    if args.template.is_some() {
        v.push("--template");
    }
    if args.node_name.is_some() {
        v.push("--node-name");
    }
    if !args.dependencies.is_empty() {
        v.push("--dependencies");
    }
    if args.license.is_some() {
        v.push("--license");
    }
    if args.description.is_some() {
        v.push("--description");
    }
    if args.maintainer_name.is_some() {
        v.push("--maintainer-name");
    }
    if args.maintainer_email.is_some() {
        v.push("--maintainer-email");
    }
    if args.destination_directory.is_some() {
        v.push("--destination-directory");
    }
    v
}

/// The `--platform` choices offered (and listed by `--help`).
const OFFERED_PLATFORMS: [&str; 6] = [
    "linux",
    "freertos",
    "nuttx",
    "threadx",
    "zephyr",
    "baremetal",
];

/// The rows of [`nros_entry_lower::RETIRED_BOARD_KEYS`] that retired a
/// `--platform` word: those whose replacement is itself an offered platform.
fn retired_platform_keys()
-> impl Iterator<Item = &'static (&'static str, &'static str, &'static str)> {
    nros_entry_lower::RETIRED_BOARD_KEYS
        .iter()
        .filter(|(_, now, _)| OFFERED_PLATFORMS.contains(now))
}

/// The `--platform` values: the offered set, plus the retired host aliases
/// (`nros_entry_lower::RETIRED_BOARD_KEYS`) HIDDEN, so they reach
/// [`refuse_retired_platform`] rather than a generic clap rejection.
fn platform_values() -> clap::builder::PossibleValuesParser {
    use clap::builder::PossibleValue;
    let offered = OFFERED_PLATFORMS.into_iter().map(PossibleValue::new);
    // Only a retired key whose replacement IS a `--platform` choice (the host
    // aliases -> `linux`); a retired BOARD key such as `qemu-mps2-an385`
    // (issue 2063) was never a platform word, so clap rejects it plainly.
    let retired = retired_platform_keys().map(|(old, _, _)| PossibleValue::new(*old).hide(true));
    clap::builder::PossibleValuesParser::new(offered.chain(retired))
}

/// phase-500 W5 — `--platform native|posix` is REFUSED, naming
/// `--platform linux`. W1 kept both as deprecated aliases of `linux` for one
/// cycle (released book pages typed them); the cycle is over, and an alias
/// that silently maps is how a reader learns a word the project retired.
fn refuse_retired_platform(platform: Option<&str>) -> Result<()> {
    let Some(p) = platform else { return Ok(()) };
    if let Some((_, now, why)) = retired_platform_keys().find(|(old, _, _)| *old == p) {
        bail!(
            "nros new: `--platform {p}` is not a platform choice: write `--platform {now}`. \
             {why}."
        );
    }
    Ok(())
}

pub fn run(args: Args) -> Result<()> {
    refuse_retired_platform(args.platform.as_deref())?;
    refuse_use_case(args.use_case.as_deref())?;
    // Every mode returning before project/component mode writes no package,
    // so the package flags are refused there (phase-517 W12).
    let package_mode = !args.workspace
        && args.image.is_none()
        && args.host.is_none()
        && args.deploy.is_none()
        && !matches!(
            args.name.as_ref().and_then(|p| p.to_str()),
            Some("platform" | "board" | "node" | "entry" | "system")
        );
    if !package_mode {
        let given = package_flags_given(&args);
        if !given.is_empty() {
            bail!(
                "nros new: {} {} for a package (`nros new <name> --platform <p>`) or a \
                 `--component`; this mode writes no package",
                given.join(", "),
                if given.len() == 1 { "is" } else { "are" }
            );
        }
    }
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
    // `new_node`'s module docs for why `--platform linux` is not the way to
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

        // Issue 1519 — `--board` is the image's `board`, so it is a NANO-ROS id
        // (`zephyr`), never the framework's (`native_sim/native/64`). The
        // scaffold still needs the framework's id for the one thing it writes
        // per board — Zephyr's own `boards/<west board>.conf` — so it resolves
        // the id through the catalog exactly as `nros build` does, which also
        // refuses a framework string here instead of writing it into the
        // user's `system.toml`. With no nano-ros checkout to read, the
        // authored id is all there is, which is what `west_build_board` falls
        // back to as well.
        let board = args.board.clone().unwrap_or_else(|| "zephyr".to_string());
        let west_board = match crate::orchestration::nano_ros_root::resolve(None, &ws_root) {
            Some(root) => {
                let catalog = crate::orchestration::board_descriptor::BoardCatalog::load(&root)
                    .map_err(|e| eyre::eyre!("board catalog under {}: {e}", root.display()))?;
                let image = crate::orchestration::image::ImageBlock {
                    board: Some(board.clone()),
                    ..Default::default()
                };
                crate::orchestration::image::resolve_image_board(&catalog, &entry_name, &image)
                    .map_err(|e| eyre::eyre!("{e}"))?
                    .west_build_board(&board)
            }
            None => board.clone(),
        };
        let out = crate::cmd::new_entry::scaffold_entry(&crate::cmd::new_entry::EntryScaffold {
            entry_dir: into.join(&entry_name),
            bringup_dir: bringup_dir.clone(),
            workspace_root: ws_root,
            board,
            west_board,
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
        let lang = language_arg(args.lang.as_deref(), nros_lang::Language::Cpp)?;
        let rmw = args.rmw.clone().unwrap_or_else(|| "cyclonedds".to_string());
        if args.platform.as_deref().unwrap_or("linux") != "linux" {
            bail!(
                "`nros new --workspace` scaffolds the host workspace shape; \
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
        // phase-469 — the validation that stood here (`match lang.as_str() {
        // "rust" | "cpp" | "c" => {} other => bail!(…) }`) was a third copy of
        // "which languages exist", behind clap's own `value_parser`, in front
        // of an identical refusal inside `scaffold_component`. Parsing once
        // leaves one answer and no copies.
        let lang = language_arg(args.lang.as_deref(), nros_lang::Language::Rust)?;
        let (template, node_name, meta) = package_args(&args)?;
        let dir = package_dir(&args, &name);
        return scaffold_component_at(
            &dir,
            &ComponentScaffoldConfig {
                name,
                template,
                node_name,
                meta,
                lang,
                force: args.force,
            },
        );
    }

    // Project mode.
    let (template, node_name, meta) = package_args(&args)?;
    let dir = package_dir(&args, &name);
    let platform = args
        .platform
        .ok_or_else(|| eyre::eyre!("`nros new <name>` requires `--platform <p>`"))?;
    let lang = language_arg(args.lang.as_deref(), nros_lang::Language::Rust)?;
    // phase-517 W2 — a C/C++ project's image board is resolved through the
    // board catalog, the join every road that builds it reads. The platform
    // word it used to write (`freertos`) reached the cmake road as its own
    // deploy token and configured with no board module.
    let board = match lang {
        nros_lang::Language::Rust => {
            if let Some(b) = &args.board {
                bail!(
                    "nros new: `--board {b}` is for a C/C++ project (or `--image`); a Rust \
                     project's board key is fixed by its `--platform` template — edit \
                     `[image.*] board` in its system.toml afterwards"
                );
            }
            if RUST_WORKSPACE_PLATFORMS.contains(&platform.as_str()) {
                let cwd = std::env::current_dir()?;
                let root =
                    crate::orchestration::nano_ros_root::resolve(None, &cwd).ok_or_else(|| {
                        eyre::eyre!("{}", crate::orchestration::nano_ros_root::not_found_help())
                    })?;
                let catalog = crate::orchestration::board_descriptor::BoardCatalog::load(&root)
                    .map_err(|e| eyre::eyre!("board catalog under {}: {e}", root.display()))?;
                let lines = rust_workspace_commands(&catalog, &name, &platform);
                bail!(
                    "nros new: a Rust node on --platform {platform} is built as a workspace — a \
                     node package plus an entry the image is built from — not as a single \
                     package. Start one with:\n\n{}\n\n(book: getting-started/{}.md)",
                    lines
                        .iter()
                        .map(|l| format!("  {l}"))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    if platform == "zephyr" {
                        "integration-zephyr"
                    } else if platform == "nuttx" {
                        "integration-nuttx"
                    } else {
                        platform.as_str()
                    }
                );
            }
            None
        }
        nros_lang::Language::C | nros_lang::Language::Cpp => {
            let cwd = std::env::current_dir()?;
            let root =
                crate::orchestration::nano_ros_root::resolve(None, &cwd).ok_or_else(|| {
                    eyre::eyre!("{}", crate::orchestration::nano_ros_root::not_found_help())
                })?;
            let catalog = crate::orchestration::board_descriptor::BoardCatalog::load(&root)
                .map_err(|e| eyre::eyre!("board catalog under {}: {e}", root.display()))?;
            Some(project_board(&catalog, &platform, args.board.as_deref())?)
        }
    };
    scaffold_package_at(
        &dir,
        &ScaffoldConfig {
            name,
            lang,
            platform,
            rmw: args.rmw.unwrap_or_else(|| "zenoh".to_string()),
            ros_edition: args.ros_edition,
            template,
            node_name,
            meta,
            force: args.force,
            board,
        },
    )
}

/// Where a package or component is created: `<--destination-directory>/<name>`,
/// else `<name>` under the current directory.
fn package_dir(args: &Args, name: &str) -> PathBuf {
    args.destination_directory
        .as_ref()
        .map_or_else(|| PathBuf::from(name), |d| d.join(name))
}

/// The platforms whose Rust image is a node library plus an entry crate, so
/// `nros new --lang rust` writes no single package for them (phase-517 W13).
const RUST_WORKSPACE_PLATFORMS: [&str; 4] = ["freertos", "nuttx", "threadx", "zephyr"];

/// phase-517 W13 — what `nros new <name> --platform <p> --lang rust` says for
/// a platform whose Rust image is a workspace: the commands that start one,
/// with the platform's boards filled in from the catalog. The refusal used to
/// name an in-tree example directory, which a user with no checkout does not
/// have.
pub(crate) fn rust_workspace_commands(
    catalog: &crate::orchestration::board_descriptor::BoardCatalog,
    name: &str,
    platform: &str,
) -> Vec<String> {
    let mut v = vec![
        format!("nros new {name} --workspace --lang rust"),
        format!("cd {name}"),
    ];
    if platform == "zephyr" {
        v.push("nros new entry zephyr_entry --platform zephyr".to_string());
        return v;
    }
    let boards: Vec<String> = match catalog.default_board_for_platform(platform) {
        Ok(d) => vec![d.canonical_name().to_string()],
        // Several boards (threadx): one line each, the reader picks.
        Err(_) => catalog
            .boards_for_platform(platform)
            .into_iter()
            .map(|d| d.canonical_name().to_string())
            .collect(),
    };
    // One board: the image is named after the platform. Several: one image
    // per board, named after it, so the lines can all be run.
    let several = boards.len() > 1;
    for b in boards {
        let id = if several {
            b.replace('-', "_")
        } else {
            platform.to_string()
        };
        v.push(format!("nros new --image {id} --board {b}"));
    }
    v
}

/// The `board` a C/C++ project scaffold for `--platform <platform>` writes
/// (phase-517 W2): `--board` when given — resolved the way `nros build`
/// resolves an image board, and required to be a board OF that platform —
/// else the platform's default board's
/// [`canonical_name`](crate::orchestration::board_descriptor::BoardDescriptor::canonical_name).
pub(crate) fn project_board(
    catalog: &crate::orchestration::board_descriptor::BoardCatalog,
    platform: &str,
    given: Option<&str>,
) -> Result<String> {
    let Some(word) = given else {
        let d = catalog
            .default_board_for_platform(platform)
            .map_err(|e| eyre::eyre!("nros new --platform {platform}: {e}"))?;
        return Ok(d.canonical_name().to_string());
    };
    let image = crate::orchestration::image::ImageBlock {
        board: Some(word.to_string()),
        ..Default::default()
    };
    let d = crate::orchestration::image::resolve_image_board(catalog, platform, &image)
        .map_err(|e| eyre::eyre!("nros new: {e}"))?;
    if d.platform.cmake_deploy() != Some(platform) {
        bail!(
            "nros new: `--board {word}` is a `{}` board, not a `--platform {platform}` one",
            d.platform.cmake_deploy().unwrap_or(d.platform.kebab())
        );
    }
    Ok(word.to_string())
}

#[cfg(test)]
mod retired_platform_tests {
    use super::*;
    use clap::{CommandFactory, Parser};

    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        args: Args,
    }

    /// phase-500 W5 — `nros new --platform native|posix` PARSES (so the
    /// refusal can name the replacement) and then REFUSES, naming
    /// `--platform linux`, before any file is written. It once mapped both to
    /// `linux` with a deprecation notice (W1).
    #[test]
    fn a_retired_host_platform_is_refused_naming_linux() {
        for p in ["native", "posix"] {
            let cli = Cli::try_parse_from(["nros-new", "x", "--platform", p, "--lang", "rust"])
                .unwrap_or_else(|e| panic!("`--platform {p}` must reach the refusal: {e}"));
            let err = run(cli.args).expect_err("must refuse").to_string();
            assert!(
                err.contains(&format!("`--platform {p}`"))
                    && err.contains("write `--platform linux`"),
                "{p}: {err}"
            );
        }
        // The offered set does not advertise them.
        let help = Cli::command().render_long_help().to_string();
        let values = help
            .lines()
            .find(|l| l.contains("possible values: linux"))
            .unwrap_or_else(|| panic!("no --platform value list in help:\n{help}"));
        assert!(
            !values.contains("native") && !values.contains("posix"),
            "{values}"
        );
    }
}

#[cfg(test)]
mod project_board_tests {
    use super::*;
    use crate::orchestration::board_descriptor::BoardCatalog;
    use clap::CommandFactory;
    use std::path::Path;

    #[derive(clap::Parser)]
    struct Cli {
        #[command(flatten)]
        args: Args,
    }

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("repo root")
            .to_path_buf()
    }

    /// Every `--platform` value `nros new` OFFERS (hidden retired aliases
    /// excluded), read from the parser itself so a new platform is covered
    /// with no edit here.
    fn offered_platforms() -> Vec<String> {
        let cmd = Cli::command();
        let arg = cmd
            .get_arguments()
            .find(|a| a.get_id() == "platform")
            .expect("--platform");
        arg.get_possible_values()
            .into_iter()
            .filter(|v| !v.is_hide_set())
            .map(|v| v.get_name().to_string())
            .collect()
    }

    fn index_board_keys(root: &Path) -> std::collections::BTreeSet<String> {
        let text = std::fs::read_to_string(root.join("nros-sdk-index.toml")).expect("index");
        let doc: toml::Table = toml::from_str(&text).expect("parse index");
        doc["board"]
            .as_table()
            .expect("[board.*]")
            .keys()
            .cloned()
            .collect()
    }

    /// The board word resolves on every road that builds a C/C++ leaf:
    /// `nros build` (the image road), the cmake road (`nros ws leaf-system`,
    /// read by `find_package(nano_ros)` and the Zephyr module alike), and
    /// `nros setup` (the index join).
    fn assert_every_road_builds(cat: &BoardCatalog, root: &Path, platform: &str, board: &str) {
        let image = crate::orchestration::image::ImageBlock {
            board: Some(board.to_string()),
            ..Default::default()
        };
        let d = crate::orchestration::image::resolve_image_board(cat, platform, &image)
            .unwrap_or_else(|e| panic!("{platform}: `{board}` refused by the image road: {e}"));
        assert_eq!(
            d.platform.cmake_deploy(),
            Some(platform),
            "{platform}: `{board}` is not a board of the platform it was written for"
        );

        let deploy = crate::cmd::leaf_system::deploy_token(Some(cat), board);
        assert_eq!(deploy, platform, "{platform}: `{board}` deploys elsewhere");
        let provider = crate::cmd::leaf_system::cmake_board_provider(Some(cat), board);
        if provider.is_empty() {
            assert!(
                d.cmake_module.is_none(),
                "{platform}: `{board}` reads as its own platform on the cmake road, but its \
                 descriptor names the board module `{:?}` — the configure would look for \
                 `nano-ros-board-.cmake`",
                d.cmake_module
            );
        } else {
            let overlay = root
                .join("cmake/board")
                .join(format!("nano-ros-board-{provider}.cmake"));
            assert!(
                overlay.is_file(),
                "{platform}: `{board}` names board module `{provider}` and {} is missing",
                overlay.display()
            );
        }

        let keys = index_board_keys(root);
        cat.setup_board(board, &|k| keys.contains(k))
            .unwrap_or_else(|e| panic!("{platform}: `{board}` refused by `nros setup`: {e}"));
    }

    /// phase-517 W2 — for every platform `nros new` offers, a C and a C++
    /// project's written board resolves through the road that builds it (plan
    /// level, nothing compiled). `freertos`, `nuttx` and `baremetal` were
    /// written as the platform word and configured with an EMPTY board module;
    /// `threadx` named two boards.
    #[test]
    fn every_offered_platform_scaffolds_a_board_every_road_builds() {
        let root = repo_root();
        let cat = BoardCatalog::load(&root).expect("in-tree board catalog");
        let platforms = offered_platforms();
        assert!(platforms.len() >= 6, "offered: {platforms:?}");
        assert!(
            !platforms.iter().any(|p| p == "esp32"),
            "esp32 is dormant (issue 1525) and must not be offered"
        );
        let mut checked = 0;
        for platform in &platforms {
            for lang in ["c", "cpp"] {
                match project_board(&cat, platform, None) {
                    Ok(board) => {
                        // `linux` and `zephyr` ARE their platform's board, so
                        // the word may equal the platform; the roads decide.
                        assert_every_road_builds(&cat, &root, platform, &board);
                        checked += 1;
                    }
                    Err(e) => {
                        // A refusal must be a USABLE one: it names candidates,
                        // and each of them, given as `--board`, builds.
                        let e = e.to_string();
                        let named: Vec<&str> = e
                            .split('`')
                            .skip(1)
                            .step_by(2)
                            .filter(|w| *w != platform.as_str() && !w.starts_with("--"))
                            .collect();
                        assert!(named.len() >= 2, "{platform}/{lang}: {e}");
                        for b in named {
                            let got = project_board(&cat, platform, Some(b))
                                .unwrap_or_else(|e| panic!("{platform}: candidate `{b}`: {e}"));
                            assert_every_road_builds(&cat, &root, platform, &got);
                            checked += 1;
                        }
                    }
                }
            }
        }
        assert!(
            checked >= 12,
            "only {checked} (platform, lang, board) rows checked"
        );
    }

    /// The measured defaults — the derivation, pinned so a catalog edit that
    /// moves one is a reviewed change.
    #[test]
    fn the_derived_defaults() {
        let cat = BoardCatalog::load(&repo_root()).expect("catalog");
        for (platform, want) in [
            ("freertos", "mps2-an385-freertos"),
            ("nuttx", "qemu-armv7a-nuttx"),
            ("baremetal", "mps2-an385-baremetal"),
            ("linux", "linux"),
            ("zephyr", "zephyr"),
        ] {
            assert_eq!(project_board(&cat, platform, None).unwrap(), want);
        }
        let e = project_board(&cat, "threadx", None)
            .unwrap_err()
            .to_string();
        assert!(
            e.contains("`threadx-linux`")
                && e.contains("`rv-virt-threadx`")
                && e.contains("--board"),
            "{e}"
        );
    }

    /// `--board` is resolved like an image board, and must belong to the
    /// platform: a framework id and a board of another platform are refused.
    #[test]
    fn a_given_board_is_resolved_and_must_match_the_platform() {
        let cat = BoardCatalog::load(&repo_root()).expect("catalog");
        assert_eq!(
            project_board(&cat, "freertos", Some("mps3-an536-freertos")).unwrap(),
            "mps3-an536-freertos"
        );
        let e = project_board(&cat, "nuttx", Some("mps2-an385-freertos"))
            .unwrap_err()
            .to_string();
        assert!(e.contains("is a `freertos` board"), "{e}");
        let e = project_board(&cat, "zephyr", Some("native_sim/native/64"))
            .unwrap_err()
            .to_string();
        assert!(e.contains("FRAMEWORK"), "{e}");
        let e = project_board(&cat, "threadx", Some("threadx"))
            .unwrap_err()
            .to_string();
        assert!(e.contains("ambiguous"), "{e}");
    }

    /// phase-517 W13 — `nros new --platform <p> --lang rust` on a workspace
    /// platform prints the commands that start one, with the catalog's boards
    /// filled in: one board where the platform has one, each where it has
    /// several, and the entry scaffold on Zephyr. Every printed board must
    /// resolve back to a descriptor of that platform.
    #[test]
    fn a_rust_workspace_platform_prints_the_commands_with_catalog_boards() {
        let cat = BoardCatalog::load(&repo_root()).expect("catalog");
        for p in RUST_WORKSPACE_PLATFORMS {
            let lines = rust_workspace_commands(&cat, "my_ws", p);
            assert_eq!(lines[0], "nros new my_ws --workspace --lang rust", "{p}");
            assert_eq!(lines[1], "cd my_ws", "{p}");
            if p == "zephyr" {
                assert_eq!(
                    lines[2..],
                    ["nros new entry zephyr_entry --platform zephyr"]
                );
                continue;
            }
            let images: Vec<&String> = lines[2..].iter().collect();
            assert!(!images.is_empty(), "{p}: no --image line");
            for l in images {
                let board = l
                    .strip_prefix("nros new --image ")
                    .and_then(|rest| rest.split_once(" --board "))
                    .map(|(_, b)| b)
                    .unwrap_or_else(|| panic!("{p}: not an --image line: {l}"));
                assert_eq!(
                    project_board(&cat, p, Some(board)).unwrap(),
                    board,
                    "{p}: printed board {board} does not resolve to itself"
                );
            }
        }
        let threadx = rust_workspace_commands(&cat, "w", "threadx");
        assert!(threadx.len() >= 4, "threadx has two boards: {threadx:?}");
        assert_eq!(
            rust_workspace_commands(&cat, "w", "freertos")[2],
            "nros new --image freertos --board mps2-an385-freertos"
        );
    }
}
