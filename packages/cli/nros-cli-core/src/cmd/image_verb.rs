//! `nros image list` / `nros image show <image>` (RFC-0133, phase-569 W7).
//!
//! `list` prints the workspace's images as its bringups declare them: board,
//! RMW and launch file, read from each `system.toml` and never planned, so it
//! is cheap and works before anything is configured. `show` adds what the
//! build resolves for one image (its platform, entry package, profile and the
//! packages it builds) through `plan_query`, which writes nothing. With
//! `--nodes` / `--topics` it prints what the image's plan will create: the
//! nodes, and the topics, services and actions, read from the model the last
//! `nros configure` resolved, never resolving one itself. With `--derived`
//! (W3) it prints what that configure derived, from the image's sizing
//! descriptor: each value, with the reason recorded for it. `check
//! --no-alloc` reads the image `nros build` made and reports every allocation
//! symbol it links ([`crate::alloc_scan`], issue 2667).

use std::path::{Path, PathBuf};

use clap::{Args as ClapArgs, Subcommand};
use eyre::{Result, bail};
use serde::Serialize;

use crate::cmd::frame::{self, Fields, Table};

/// The `--json` schema names (RFC-0133 D1). Adding a key keeps the version;
/// removing or renaming one is a new version.
pub const LIST_SCHEMA: &str = "nros.image.list.v1";
pub const SHOW_SCHEMA: &str = "nros.image.show.v1";
pub const CHECK_SCHEMA: &str = "nros.image.check.v1";
/// The page on what a heap-free image may link, and how this check reads it.
pub const CHECK_BOOK_PAGE: &str = "background/static-memory-and-pools.html";
/// The page that explains an image and what it declares.
pub const BOOK_PAGE: &str = "guide/build/configure-an-image.html";

#[derive(Debug, Subcommand)]
pub enum Args {
    /// List the workspace's images, as its bringups declare them.
    List(ListArgs),
    /// Show one image: what it declares, and what the build resolves for it.
    Show(ShowArgs),
    /// Check a built image. `--no-alloc` reports every allocation symbol it
    /// links, and exits 1 when there is one.
    Check(CheckArgs),
}

/// What `--no-alloc` holds an image to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Tier {
    /// No allocation symbol at all, defined or referenced.
    HeapFree,
    /// Allocation only through the platform's allocator. Reads the build's
    /// objects, since a linked image no longer says who called `malloc`.
    Unified,
}

#[derive(Debug, ClapArgs)]
pub struct CheckArgs {
    /// The image, as `nros build` names it. Default: the workspace's only
    /// image.
    pub image: Option<String>,
    /// Report every allocation symbol the image links, by family: the C
    /// heap, the Rust allocator, C++ `operator new`, an RTOS heap.
    #[arg(long, required = true)]
    pub no_alloc: bool,
    /// What the image is held to.
    #[arg(long, value_enum, default_value = "heap-free")]
    pub tier: Tier,
    /// Read this linked image instead of the one `nros build` made. May be
    /// given more than once.
    #[arg(long, value_name = "PATH", conflicts_with = "image")]
    pub elf: Vec<PathBuf>,
    /// With `--tier unified`: the objects, archives or build directories to
    /// read. Default: the image's build directory.
    #[arg(long, value_name = "PATH", conflicts_with = "image")]
    pub objects: Vec<PathBuf>,
    /// Permit one symbol, with the reason: `--allow 'malloc=<why>'`.
    #[arg(long, value_name = "SYMBOL=REASON")]
    pub allow: Vec<String>,
    /// Workspace root. Default: the workspace the current directory is in.
    #[arg(long)]
    pub workspace: Option<PathBuf>,
    /// Print one JSON object instead of text.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, ClapArgs)]
pub struct ListArgs {
    /// Workspace root. Default: the workspace the current directory is in.
    #[arg(long)]
    pub workspace: Option<PathBuf>,
    /// Print one JSON object instead of text.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, ClapArgs)]
pub struct ShowArgs {
    /// The image, as `nros build` names it (`linux`, or `<bringup>:linux`
    /// when two bringups declare the same id).
    pub image: String,
    /// Workspace root. Default: the workspace the current directory is in.
    #[arg(long)]
    pub workspace: Option<PathBuf>,
    /// Print one JSON object instead of text.
    #[arg(long)]
    pub json: bool,
    /// Print the nodes the image's plan will start.
    #[arg(long)]
    pub nodes: bool,
    /// Print the topics, services and actions the image's plan will create,
    /// and those its sizing descriptor records.
    #[arg(long)]
    pub topics: bool,
    /// Print what the build derived for the image: its target, entity and
    /// type counts, each with the reason recorded for it.
    #[arg(long)]
    pub derived: bool,
}

/// One image as its bringup declares it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ImageItem {
    /// The name a user types: the bare id, or `<bringup>:<id>` when two
    /// bringups declare the id.
    pub name: String,
    pub id: String,
    pub bringup: String,
    pub board: Option<String>,
    pub rmw: Option<String>,
    pub launch: Option<String>,
    /// The bringup package's directory.
    #[serde(skip)]
    pub bringup_dir: PathBuf,
    /// The entry package the image runs instead of a launch file, when it
    /// names one.
    #[serde(skip)]
    pub entry: Option<String>,
    /// The launch arguments `[image.<id>] args` binds.
    #[serde(skip)]
    pub launch_args: Vec<(String, String)>,
}

/// One image with what the build resolves for it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ImageShow {
    #[serde(flatten)]
    pub declared: ImageItem,
    pub platform: String,
    pub entry_package: Option<String>,
    pub profile: Option<String>,
    pub capabilities: Vec<String>,
    /// The workspace packages this image builds, when the plan states them.
    pub packages: Option<Vec<String>>,
    /// With `--nodes`: the nodes the plan starts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub planned_nodes: Option<Vec<PlannedNode>>,
    /// With `--topics`: the topics, services and actions the plan creates.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub planned_endpoints: Option<Vec<PlannedEndpoint>>,
    /// With `--derived`: what the last `nros configure` derived.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub derived: Option<Derived>,
}

/// What the build derived for an image, read from its sizing descriptor.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Derived {
    /// The descriptor it was read from.
    pub descriptor: PathBuf,
    pub values: Vec<DerivedValue>,
}

/// One derived value and the reason for it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DerivedValue {
    /// `target`, `image` or `types`.
    pub section: String,
    pub name: String,
    /// `None` when the build did not derive it.
    pub value: Option<usize>,
    pub reason: String,
}

/// The reason column until a producer records one (phase-570).
pub const NO_REASON: &str = "(no reason recorded)";

/// One node an image's plan starts.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PlannedNode {
    /// Its name in the plan (the ROS name when the launch file gives one).
    pub name: String,
    pub package: Option<String>,
    /// The executable, or the component plugin and its container.
    pub runs: String,
}

/// One topic, service or action an image's plan creates.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PlannedEndpoint {
    /// `topic`, `service` or `action`.
    pub kind: &'static str,
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    /// Publishers, servers: the nodes on the providing side.
    pub providers: Vec<String>,
    /// Subscribers, clients: the nodes on the using side.
    pub users: Vec<String>,
    /// Where the row comes from: `plan` (the launch model's wiring) or
    /// `descriptor` (the image's sizing descriptor, which names the image on
    /// its side rather than a node).
    pub source: &'static str,
}

/// A row from the launch model's wiring.
pub const FROM_PLAN: &str = "plan";
/// A row only the image's sizing descriptor records.
pub const FROM_DESCRIPTOR: &str = "descriptor";

pub fn run(args: Args) -> Result<()> {
    match args {
        Args::List(a) => list(a),
        Args::Show(a) => show(a),
        Args::Check(a) => check(a),
    }
}

/// The workspace `workspace` names, or the one the current directory is in.
pub(crate) fn root(workspace: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(w) = workspace {
        return Ok(w);
    }
    let cwd = std::env::current_dir()?;
    if let Some(r) = crate::cmd::image_facts::workspace_root_from(&cwd) {
        return Ok(r);
    }
    if cwd.join("system.toml").is_file() {
        return Ok(cwd);
    }
    bail!(
        "{} is not in a nano-ros workspace: no `system.toml` declares an image here or above.\n  \
         Run this in a workspace, or name one with `--workspace <dir>`",
        cwd.display()
    )
}

/// Every image the workspace at `root` declares, sorted by name.
pub fn items(root: &Path) -> Result<Vec<ImageItem>> {
    let members = crate::builder::discover::cargo_members_or_walk(root);
    let found =
        crate::builder::discover::discover(root, &members).map_err(|e| eyre::eyre!("{e}"))?;
    let bringups = crate::cmd::build::collect_images(&found.packages)?;
    let all = crate::builder::plan::all_images(&bringups);
    let mut count = std::collections::BTreeMap::<&str, usize>::new();
    for (_, _, id, _) in &all {
        *count.entry(id.as_str()).or_default() += 1;
    }
    let mut out: Vec<ImageItem> = all
        .iter()
        .map(|(bringup, dir, id, block)| ImageItem {
            name: if count[id.as_str()] == 1 {
                id.clone()
            } else {
                crate::builder::plan::qualified(bringup, id)
            },
            id: id.clone(),
            bringup: bringup.clone(),
            board: block.board.clone(),
            rmw: block.rmw.clone(),
            launch: block.launch.clone(),
            bringup_dir: dir.clone(),
            entry: block.entry.clone(),
            launch_args: block
                .args
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn list(args: ListArgs) -> Result<()> {
    let items = items(&root(args.workspace)?)?;
    if args.json {
        return frame::print_json(LIST_SCHEMA, "items", &items, &[]);
    }
    print!("{}", render_list(&items));
    Ok(())
}

/// The image `name` names: its bare id or its qualified name.
pub fn find<'a>(items: &'a [ImageItem], name: &str) -> Result<&'a ImageItem> {
    let qualified = |i: &ImageItem| crate::builder::plan::qualified(&i.bringup, &i.id);
    if let Some(i) = items
        .iter()
        .find(|i| i.name == name || qualified(i) == name)
    {
        return Ok(i);
    }
    Err(frame::unknown(
        "image",
        name,
        items.iter().map(|i| i.name.as_str()),
        "nros image list",
    ))
}

/// The image `name` names (or the workspace's only one) with what `nros
/// build` resolves for it, written nowhere.
fn plan_of(
    root: &Path,
    name: Option<&str>,
) -> Result<(ImageItem, crate::cmd::build::ResolvedBuild)> {
    let items = items(root)?;
    let declared = match (name, items.as_slice()) {
        (Some(n), _) => find(&items, n)?.clone(),
        (None, [one]) => one.clone(),
        (None, []) => bail!("the workspace at {} declares no image", root.display()),
        (None, many) => bail!(
            "the workspace declares {} images; name one: {}",
            many.len(),
            many.iter()
                .map(|i| i.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };
    let qualified = crate::builder::plan::qualified(&declared.bringup, &declared.id);
    let plans = crate::cmd::build::plan_query(&query_args(root, &qualified))?;
    let Some(plan) = plans.into_iter().find(|p| p.qualified == qualified) else {
        bail!(
            "`nros build` resolved no plan for image `{}`",
            declared.name
        );
    };
    Ok((declared, plan))
}

fn show(args: ShowArgs) -> Result<()> {
    let root = root(args.workspace)?;
    let (declared, plan) = plan_of(&root, Some(&args.image))?;
    let planned_nodes = if args.nodes {
        Some(planned_nodes(&planned_model(&declared)?))
    } else {
        None
    };
    let planned_endpoints = if args.topics {
        Some(planned_topics(
            &declared,
            plan.sizing_descriptor.as_deref(),
        )?)
    } else {
        None
    };
    let derived = if args.derived {
        Some(derived_of(&declared, plan.sizing_descriptor.as_deref())?)
    } else {
        None
    };
    let item = ImageShow {
        derived,
        planned_nodes,
        planned_endpoints,
        declared,
        platform: plan.platform,
        entry_package: plan.entry_package,
        profile: plan.profile,
        capabilities: plan.capabilities,
        packages: plan.package_depends,
    };
    if args.json {
        return frame::print_json(SHOW_SCHEMA, "item", &item, &[]);
    }
    print!("{}", render_show(&item));
    Ok(())
}

/// The exit code of a check that could not be run: never a pass.
pub const CANNOT_CHECK: i32 = 2;

/// A check that could not run: the reason, and exit [`CANNOT_CHECK`].
fn cannot_check(why: &str) -> ! {
    eprintln!("nros image check: cannot check: {why}");
    std::process::exit(CANNOT_CHECK)
}

/// The symbols `--allow` permits, or the refusal of one with no reason.
fn allows(specs: &[String]) -> Result<Vec<String>> {
    specs
        .iter()
        .map(|s| match s.split_once('=') {
            Some((sym, why)) if !sym.trim().is_empty() && !why.trim().is_empty() => {
                Ok(sym.trim().to_string())
            }
            _ => bail!(
                "--allow {s}: give the symbol and the reason it is allowed, as \
                 `--allow 'malloc=<why>'`"
            ),
        })
        .collect()
}

/// The linked image `nros build` made for the image `name` (or the
/// workspace's only one), and the directory its objects are in.
fn built_image(root: &Path, name: Option<&str>) -> Result<(String, PathBuf, PathBuf)> {
    use crate::cmd::build_next::BuildOutput;
    // The target resolution `nros run` and `nros monitor` use, so the image
    // read is the one they would run.
    let sel = crate::cmd::run_plan::Selection {
        workspace: Some(root.to_path_buf()),
        image: name.map(str::to_string),
        ..Default::default()
    };
    let plan = crate::cmd::run_plan::select_target(&sel, "image check")
        .unwrap_or_else(|e| cannot_check(&e));
    let declared_name = plan.qualified.clone();
    let Some(output) = plan.output.as_ref() else {
        cannot_check(&format!(
            "image `{declared_name}` names no build output to read"
        ));
    };
    let host = plan
        .descriptor
        .as_ref()
        .and_then(|d| d.run.as_ref())
        .is_some_and(|r| r.host);
    let elf = match crate::cmd::run_plan::build_elf(output, host, plan.launch_exec.as_deref()) {
        Ok(Some(e)) => e,
        Ok(None) => cannot_check(&format!(
            "image `{declared_name}`: the build names no single program to read"
        )),
        Err(e) => cannot_check(&format!("image `{declared_name}`: {e}")),
    };
    let objects = match output {
        BuildOutput::Cargo {
            target_dir,
            triple,
            profile_dir,
            ..
        } => {
            let mut d = target_dir.clone();
            if let Some(t) = triple {
                d.push(t);
            }
            d.join(profile_dir)
        }
        BuildOutput::CMake { build_dir, .. } => build_dir.clone(),
        BuildOutput::West { build_dir } => build_dir
            .clone()
            .unwrap_or_else(|| elf.parent().unwrap_or(root).to_path_buf()),
    };
    Ok((declared_name, elf, objects))
}

fn check(args: CheckArgs) -> Result<()> {
    use crate::alloc_scan;
    let allows = allows(&args.allow)?;
    let from_build = args.elf.is_empty() && args.objects.is_empty();
    let built = if from_build {
        Some(built_image(&root(args.workspace)?, args.image.as_deref())?)
    } else {
        None
    };
    let hint = |path: &Path| match &built {
        Some((name, ..)) => format!(
            "{} does not exist. Build it first: `nros build {name}`",
            path.display()
        ),
        None => format!("{} does not exist", path.display()),
    };
    let (reports, failed) = match args.tier {
        Tier::HeapFree => {
            if !args.objects.is_empty() {
                cannot_check("`--objects` is the `unified` tier's input; add `--tier unified`");
            }
            let elves = match &built {
                Some((_, elf, _)) => vec![elf.clone()],
                None => args.elf.clone(),
            };
            let mut reports = Vec::new();
            for e in &elves {
                if !e.exists() {
                    cannot_check(&hint(e));
                }
                match alloc_scan::check_image(e, &allows) {
                    Ok(r) => reports.push(r),
                    Err(v) => cannot_check(&v.0),
                }
            }
            let failed = reports.iter().any(|r| !r.findings.is_empty());
            if args.json {
                frame::print_json(
                    CHECK_SCHEMA,
                    "items",
                    &reports,
                    &[("tier", "heap-free".into())],
                )?;
            } else {
                for r in &reports {
                    print!("{}", alloc_scan::render_image(r));
                }
            }
            (reports.len(), failed)
        }
        Tier::Unified => {
            if !args.elf.is_empty() {
                cannot_check(
                    "tier `unified` is a rule about which object calls the allocator, and \
                     a linked image no longer records that. Pass `--objects <build-dir>` \
                     instead of `--elf`",
                );
            }
            let paths = match &built {
                Some((_, _, objects)) => vec![objects.clone()],
                None => args.objects.clone(),
            };
            let r = match alloc_scan::check_objects(&paths, &allows) {
                Ok(r) => r,
                Err(v) => cannot_check(&v.0),
            };
            let failed = !r.findings.is_empty();
            if args.json {
                frame::print_json(CHECK_SCHEMA, "items", &[&r], &[("tier", "unified".into())])?;
            } else {
                print!("{}", alloc_scan::render_objects(&r));
            }
            (1, failed)
        }
    };
    if !args.json && reports > 0 {
        print!("{}", frame::book_line(CHECK_BOOK_PAGE));
    }
    if failed {
        std::process::exit(1);
    }
    Ok(())
}

/// `nros build`'s arguments for a query of one image: resolve, never build,
/// never reach the network.
fn query_args(root: &Path, qualified: &str) -> crate::cmd::build::Args {
    crate::cmd::build::Args {
        images: vec![qualified.to_string()],
        workspace: Some(root.to_path_buf()),
        nano_ros_path: None,
        zephyr_workspace: None,
        board: None,
        all: false,
        dry_run: true,
        verbose: false,
        explain_sizing: false,
        offline: true,
        selection: Default::default(),
        continue_on_error: false,
        cmake_args: Vec::new(),
        cargo_args: Vec::new(),
        profile: None,
        jobs: None,
        parallel_workers: None,
        build_base: None,
        check: None,
        allow_overriding: Vec::new(),
        native_args: Vec::new(),
    }
}

fn dash(v: &Option<String>) -> String {
    v.clone().unwrap_or_else(|| "-".into())
}

pub fn render_list(items: &[ImageItem]) -> String {
    let mut t = Table::new(
        &["IMAGE", "BRINGUP", "BOARD", "RMW", "LAUNCH"],
        "No images: no bringup's `system.toml` declares an `[image.<id>]`.",
    );
    for i in items {
        t.row(vec![
            i.name.clone(),
            i.bringup.clone(),
            dash(&i.board),
            dash(&i.rmw),
            dash(&i.launch),
        ]);
    }
    t.footer("`nros image show <image>` shows what the build resolves for one.");
    t.render()
}

/// The model the last `nros configure` resolved for the image, through the
/// gate every reader uses: resolved for nothing here, and refused when stale.
pub(crate) fn planned_model(d: &ImageItem) -> Result<ros_launch_manifest_model::SystemModel> {
    use nros_orchestration_ir::model_location::launch_to_model_rel;
    if let (Some(entry), None) = (&d.entry, &d.launch) {
        bail!(
            "image `{}` runs its own entry package `{entry}`, which starts its nodes \
             itself: no launch file plans them, so there is no plan to print",
            d.name
        );
    }
    let rel = launch_to_model_rel(&d.bringup_dir, d.launch.as_deref(), &d.launch_args)
        .map_err(|e| eyre::eyre!("image `{}`: {e}", d.name))?;
    let path = match crate::model_gate::open(&d.bringup_dir, &rel) {
        Ok(Some(p)) => p,
        Ok(None) => bail!(
            "image `{}` has no plan yet. Run `nros configure` first: it resolves what the \
             image's launch file starts",
            d.name
        ),
        Err(r) => bail!("image `{}`: {}", d.name, with_configure(&r.to_string())),
    };
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| eyre::eyre!("reading {}: {e}", path.display()))?;
    ros_launch_manifest_model::SystemModel::from_yaml_str(&raw)
        .map_err(|e| eyre::eyre!("reading {}: {e}", path.display()))
}

/// The nodes a model starts, in launch order.
pub fn planned_nodes(m: &ros_launch_manifest_model::SystemModel) -> Vec<PlannedNode> {
    m.structure
        .nodes
        .iter()
        .map(|(name, n)| PlannedNode {
            name: name.clone(),
            package: n.pkg.clone(),
            runs: match (&n.exec, &n.plugin, &n.container) {
                (Some(e), _, _) => e.clone(),
                (None, Some(p), Some(c)) => format!("{p} in {c}"),
                (None, Some(p), None) => p.clone(),
                (None, None, _) => String::new(),
            },
        })
        .collect()
}

/// The node an endpoint ref (`<node>/<endpoint>`) belongs to.
fn endpoint_node(r: &str) -> String {
    r.rsplit_once('/')
        .map_or(r, |(n, _)| if n.is_empty() { r } else { n })
        .to_string()
}

fn nodes_of(refs: &[String]) -> Vec<String> {
    let mut v: Vec<String> = refs.iter().map(|r| endpoint_node(r)).collect();
    v.sort();
    v.dedup();
    v
}

/// The topics, then services, then actions a model creates, each sorted.
pub fn planned_endpoints(m: &ros_launch_manifest_model::SystemModel) -> Vec<PlannedEndpoint> {
    let st = &m.structure;
    let topics = st.topics.iter().map(|(name, w)| PlannedEndpoint {
        kind: "topic",
        name: name.clone(),
        ty: w.msg_type.clone(),
        providers: nodes_of(&w.publishers),
        users: nodes_of(&w.subscribers),
        source: FROM_PLAN,
    });
    let wired = |kind: &'static str,
                 map: &std::collections::BTreeMap<
        String,
        ros_launch_manifest_model::ServiceWiring,
    >| {
        map.iter()
            .map(|(name, w)| PlannedEndpoint {
                kind,
                name: name.clone(),
                ty: w.srv_type.clone(),
                providers: nodes_of(&w.server),
                users: nodes_of(&w.client),
                source: FROM_PLAN,
            })
            .collect::<Vec<_>>()
    };
    topics
        .chain(wired("service", &st.services))
        .chain(wired("action", &st.actions))
        .collect()
}

/// The topics, services and actions an image's sizing descriptor records.
/// The configure probe records the image's own endpoints, not which node
/// holds each, so the image stands on its side. A hidden name (a segment
/// that starts with `_`, such as an action's `<action>/_action/feedback`) is
/// left out, as `ros2 topic list` leaves it out.
pub fn descriptor_endpoints(
    image: &str,
    desc: &nros_sizing_descriptor::SizingDescriptor,
) -> Vec<PlannedEndpoint> {
    use nros_sizing_descriptor::EndpointKind as K;
    let side = vec![format!("image {image}")];
    let mut out: Vec<PlannedEndpoint> = Vec::new();
    for e in &desc.endpoints {
        if is_hidden(&e.topic) {
            continue;
        }
        let (kind, provides) = match e.kind {
            K::Publisher => ("topic", true),
            K::Subscription => ("topic", false),
            K::ServiceServer => ("service", true),
            K::ServiceClient => ("service", false),
            K::ActionServer => ("action", true),
            K::ActionClient => ("action", false),
        };
        let at = match out.iter().position(|r| r.kind == kind && r.name == e.topic) {
            Some(at) => at,
            None => {
                out.push(PlannedEndpoint {
                    kind,
                    name: e.topic.clone(),
                    ty: e.type_name.clone(),
                    providers: Vec::new(),
                    users: Vec::new(),
                    source: FROM_DESCRIPTOR,
                });
                out.len() - 1
            }
        };
        let row = &mut out[at];
        let to = if provides {
            &mut row.providers
        } else {
            &mut row.users
        };
        if to.is_empty() {
            *to = side.clone();
        }
    }
    out
}

/// A ROS name ROS 2's tools hide by default: one with a segment that starts
/// with `_`.
fn is_hidden(name: &str) -> bool {
    name.split('/').any(|s| s.starts_with('_'))
}

/// The plan's rows, then each descriptor row the plan does not wire, in
/// topic, service, action order and by name within each.
pub fn merged_endpoints(
    plan: Vec<PlannedEndpoint>,
    descriptor: Vec<PlannedEndpoint>,
) -> Vec<PlannedEndpoint> {
    let mut out = plan;
    for d in descriptor {
        if !out.iter().any(|p| p.kind == d.kind && p.name == d.name) {
            out.push(d);
        }
    }
    let rank = |k: &str| match k {
        "topic" => 0,
        "service" => 1,
        _ => 2,
    };
    out.sort_by(|a, b| (rank(a.kind), &a.name).cmp(&(rank(b.kind), &b.name)));
    out
}

/// What `--topics` prints: the plan's wiring and the endpoints the image's
/// sizing descriptor records. An image that runs its own entry package has
/// no plan, so its descriptor alone answers.
fn planned_topics(d: &ImageItem, descriptor: Option<&Path>) -> Result<Vec<PlannedEndpoint>> {
    let owned = d.entry.is_some() && d.launch.is_none();
    let plan = if owned {
        Vec::new()
    } else {
        planned_endpoints(&planned_model(d)?)
    };
    let recorded = match (descriptor.filter(|p| p.is_file()), owned) {
        (Some(p), _) => descriptor_endpoints(&d.name, &fresh_descriptor(d, p)?),
        (None, false) => Vec::new(),
        (None, true) => bail!(
            "image `{}` runs its own entry package, so only its sizing descriptor records its \
             topics, and it has none yet. Run `nros configure` first",
            d.name
        ),
    };
    Ok(merged_endpoints(plan, recorded))
}

fn joined(v: &[String]) -> String {
    if v.is_empty() {
        "-".to_string()
    } else {
        v.join(", ")
    }
}

pub fn render_planned_nodes(nodes: &[PlannedNode]) -> String {
    let mut t = Table::new(
        &["PLANNED NODE", "PACKAGE", "RUNS"],
        "The plan starts no node.",
    );
    for n in nodes {
        t.row(vec![n.name.clone(), dash(&n.package), n.runs.clone()]);
    }
    t.render()
}

pub fn render_planned_endpoints(eps: &[PlannedEndpoint]) -> String {
    if eps.is_empty() {
        return "Neither the plan nor the image's sizing descriptor records a topic, service or \
                action.\n"
            .to_string();
    }
    let mut t = Table::new(&["PLANNED", "KIND", "TYPE", "FROM", "TO"], "");
    for e in eps {
        t.row(vec![
            e.name.clone(),
            e.kind.to_string(),
            e.ty.clone(),
            joined(&e.providers),
            joined(&e.users),
        ]);
    }
    let mut foot =
        "FROM is the publisher or server side, TO the subscriber or client side.".to_string();
    if eps.iter().any(|e| e.source == FROM_DESCRIPTOR) {
        foot.push_str(
            "\n`image <name>` marks an endpoint the image's sizing descriptor records without \
             naming its node.",
        );
    }
    t.footer(&foot);
    t.render()
}

/// What the last `nros configure` derived for the image, from its sizing
/// descriptor. Never configures: a missing or stale descriptor names `nros
/// configure` instead.
fn derived_of(d: &ImageItem, descriptor: Option<&Path>) -> Result<Derived> {
    let Some(path) = descriptor else {
        bail!(
            "image `{}` is built by a road that writes no sizing descriptor, so there is no \
             derived configuration to print",
            d.name
        );
    };
    if !path.is_file() {
        bail!(
            "image `{}` has no derived configuration yet. Run `nros configure` first: it \
             writes {}",
            d.name,
            path.display()
        );
    }
    Ok(Derived {
        descriptor: path.to_path_buf(),
        values: derived_values(&fresh_descriptor(d, path)?),
    })
}

/// The image's sizing descriptor at `path`, refused when the plan it was
/// derived from is stale.
fn fresh_descriptor(
    d: &ImageItem,
    path: &Path,
) -> Result<nros_sizing_descriptor::SizingDescriptor> {
    // The descriptor is derived from the image's plan, so it is as fresh as
    // the plan: the gate refuses a plan older than its inputs. (Not the
    // descriptor's own mtime: it is written only when it changes.) An owned
    // entry with no launch has no plan.
    if d.launch.is_some() || d.entry.is_none() {
        use nros_orchestration_ir::model_location::launch_to_model_rel;
        let rel = launch_to_model_rel(&d.bringup_dir, d.launch.as_deref(), &d.launch_args)
            .map_err(|e| eyre::eyre!("image `{}`: {e}", d.name))?;
        match crate::model_gate::open(&d.bringup_dir, &rel) {
            Ok(_) => {}
            Err(r) => bail!("image `{}`: {}", d.name, with_configure(&r.to_string())),
        }
    }
    nros_sizing_descriptor::read(path).map_err(|e| eyre::eyre!("image `{}`: {e}", d.name))
}

/// The gate's refusal, with the remedy added unless it already names one.
fn with_configure(refusal: &str) -> String {
    if refusal.contains("nros configure") {
        refusal.to_string()
    } else {
        format!("{refusal}\n  Run `nros configure` to plan it again")
    }
}

/// The descriptor's derived counts, each with its reason.
pub fn derived_values(desc: &nros_sizing_descriptor::SizingDescriptor) -> Vec<DerivedValue> {
    use nros_sizing_descriptor::Fact;
    crate::cmd::sizing_descriptor::derived_facts(desc)
        .into_iter()
        .map(|(section, name, f)| {
            let (value, reason) = match f {
                Fact::Stated(v) => (Some(v), NO_REASON.to_string()),
                Fact::Refused(why) => (None, format!("not derived: {why}")),
                Fact::Absent => (None, "not derived: nothing states it".to_string()),
            };
            DerivedValue {
                section: section.to_string(),
                name: name.to_string(),
                value,
                reason,
            }
        })
        .collect()
}

/// A refusal can run to a list. The table keeps its lead clause and `--json`
/// the whole of it. The descriptor stores a refusal on one line, its newlines
/// turned to spaces, so a list shows there as a colon and a run of spaces.
fn reason_cell(reason: &str) -> String {
    let first = reason.lines().next().unwrap_or_default().trim_end();
    match first.find(":   ") {
        Some(at) => format!("{} (the list: --json)", &first[..at]),
        None => first.to_string(),
    }
}

pub fn render_derived(d: &Derived) -> String {
    let mut t = Table::new(
        &["DERIVED", "VALUE", "REASON"],
        "The descriptor states no derived value.",
    );
    for v in &d.values {
        t.row(vec![
            format!("{}.{}", v.section, v.name),
            v.value.map_or_else(|| "-".to_string(), |n| n.to_string()),
            reason_cell(&v.reason),
        ]);
    }
    t.footer(&format!("Read from {}.", d.descriptor.display()));
    t.render()
}

pub fn render_show(i: &ImageShow) -> String {
    let d = &i.declared;
    let mut out = frame::show_head(
        &d.name,
        &format!("image `{}` of bringup `{}`", d.id, d.bringup),
    );
    let mut f = Fields::new();
    f.field("board", dash(&d.board))
        .field("platform", i.platform.clone())
        .field("rmw", dash(&d.rmw))
        .field("launch", dash(&d.launch))
        .field("entry package", dash(&i.entry_package))
        .field("profile", dash(&i.profile))
        .field("capabilities", i.capabilities.join(", "))
        .field(
            "packages",
            i.packages
                .as_ref()
                .map(|p| p.join(", "))
                .unwrap_or_default(),
        );
    out.push_str(&f.render());
    if let Some(n) = &i.planned_nodes {
        out.push('\n');
        out.push_str(&render_planned_nodes(n));
    }
    if let Some(e) = &i.planned_endpoints {
        out.push('\n');
        out.push_str(&render_planned_endpoints(e));
    }
    if let Some(d) = &i.derived {
        out.push('\n');
        out.push_str(&render_derived(d));
    }
    out.push_str(&frame::book_line(BOOK_PAGE));
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_listed_refusal_keeps_its_lead_clause_in_the_table() {
        let flat = "2 type(s) carry no derived bound, so it is unknown:     a/msg/A (unbounded)     b/msg/B (unbounded)   Bound the member.";
        assert_eq!(
            reason_cell(&format!("not derived: {flat}")),
            "not derived: 2 type(s) carry no derived bound, so it is unknown (the list: --json)"
        );
        assert_eq!(
            reason_cell("not derived: no budget"),
            "not derived: no budget"
        );
        assert_eq!(reason_cell("first\nsecond"), "first");
    }

    use super::*;

    fn item(name: &str, id: &str, bringup: &str) -> ImageItem {
        ImageItem {
            name: name.into(),
            id: id.into(),
            bringup: bringup.into(),
            board: Some("linux".into()),
            rmw: None,
            launch: Some("system.launch.xml".into()),
            bringup_dir: PathBuf::new(),
            entry: None,
            launch_args: Vec::new(),
        }
    }

    fn model() -> ros_launch_manifest_model::SystemModel {
        ros_launch_manifest_model::SystemModel::from_yaml_str(
            "meta:\n  version: 1\nstructure:\n  nodes:\n    /talker:\n      scope: system.launch.xml\n      \
             pkg: demo_nodes\n      exec: talker\n      node_name: talker\n    /listener:\n      \
             scope: system.launch.xml\n      pkg: demo_nodes\n      exec: listener\n      \
             node_name: listener\n  topics:\n    /chatter:\n      type: std_msgs/msg/String\n      \
             pub:\n      - /talker/chatter\n      sub:\n      - /listener/chatter\n",
        )
        .expect("fixture model parses")
    }

    #[test]
    fn the_planned_nodes_are_the_model_s_in_launch_order() {
        let nodes = planned_nodes(&model());
        assert_eq!(
            nodes.iter().map(|n| n.name.as_str()).collect::<Vec<_>>(),
            ["/talker", "/listener"]
        );
        assert_eq!(nodes[0].runs, "talker");
        let text = render_planned_nodes(&nodes);
        assert!(text.starts_with("PLANNED NODE"), "{text}");
    }

    #[test]
    fn a_planned_topic_names_the_nodes_on_each_side() {
        let eps = planned_endpoints(&model());
        assert_eq!(eps.len(), 1);
        assert_eq!(eps[0].kind, "topic");
        assert_eq!(eps[0].ty, "std_msgs/msg/String");
        assert_eq!(eps[0].providers, ["/talker"]);
        assert_eq!(eps[0].users, ["/listener"]);
        assert!(render_planned_endpoints(&eps).contains("/chatter"));
        assert!(render_planned_endpoints(&[]).contains("records a topic"));
    }

    /// phase-569 W7c part 2 — the descriptor's `[[endpoint]]` rows fill in
    /// what the plan does not wire, with the image on their side.
    #[test]
    fn a_topic_only_the_descriptor_records_names_the_image() {
        let desc = nros_sizing_descriptor::parse(
            "schema_version = 3\n\n[meta]\nentry = \"e\"\nstatus = \"derived\"\nbasis = \
             \"contract\"\n\n[[endpoint]]\nkind = \"publisher\"\ntype = \
             \"action_msgs/msg/GoalStatusArray\"\ntopic = \
             \"/fibonacci/_action/status\"\n\n[[endpoint]]\nkind = \"publisher\"\ntype = \
             \"std_msgs/msg/String\"\ntopic = \"/chatter\"\n\n[[endpoint]]\nkind = \
             \"service_server\"\ntype = \"std_srvs/srv/Empty\"\ntopic = \"/reset\"\n",
            Path::new("e.toml"),
        )
        .unwrap();
        // The action's hidden `_action` topic is left out, as ROS 2 hides it.
        let recorded = descriptor_endpoints("linux", &desc);
        assert_eq!(recorded.len(), 2);
        assert_eq!(recorded[0].name, "/chatter");
        assert_eq!(recorded[0].providers, ["image linux"]);
        assert!(recorded[0].users.is_empty());

        // The plan wires /chatter, so its row stays; /reset comes from the
        // descriptor, after the topics.
        let eps = merged_endpoints(planned_endpoints(&model()), recorded.clone());
        let rows: Vec<_> = eps.iter().map(|e| (e.name.as_str(), e.source)).collect();
        assert_eq!(rows, [("/chatter", FROM_PLAN), ("/reset", FROM_DESCRIPTOR)]);
        let text = render_planned_endpoints(&eps);
        assert!(text.contains("image linux"), "{text}");
        assert!(text.contains("without naming its node"), "{text}");

        // With no plan wiring, the descriptor's rows answer alone.
        let eps = merged_endpoints(Vec::new(), recorded);
        assert_eq!(eps[0].name, "/chatter");
        assert_eq!(eps[0].source, FROM_DESCRIPTOR);
    }

    #[test]
    fn an_owned_entry_image_with_no_descriptor_names_nros_configure() {
        let mut i = item("linux", "linux", "b");
        i.entry = Some("talker_entry".into());
        i.launch = None;
        let missing = std::env::temp_dir().join("nros-w7c-no-such/nros/sizing/linux.toml");
        let e = planned_topics(&i, Some(&missing)).unwrap_err().to_string();
        assert!(e.contains("nros configure"), "{e}");
    }

    #[test]
    fn an_image_with_no_plan_names_nros_configure() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("system.toml"), "").unwrap();
        let mut i = item("linux", "linux", "demo_bringup");
        i.bringup_dir = tmp.path().to_path_buf();
        let e = planned_model(&i).unwrap_err().to_string();
        assert!(e.contains("nros configure"), "{e}");
        i.entry = Some("talker_entry".into());
        i.launch = None;
        let e = planned_model(&i).unwrap_err().to_string();
        assert!(
            e.contains("runs its own entry package `talker_entry`"),
            "{e}"
        );
    }

    /// phase-569 W3 — a stated value carries the reason column's placeholder
    /// until phase-570 records one; an unstated one says it was not derived.
    #[test]
    fn a_derived_value_shows_its_reason_or_that_it_was_not_derived() {
        let desc = nros_sizing_descriptor::parse(
            "schema_version = 3\n\n[meta]\nentry = \"e\"\nstatus = \"derived\"\nbasis = \
             \"contract\"\n\n[image]\nnode_count = 2\n",
            Path::new("e.toml"),
        )
        .unwrap();
        let values = derived_values(&desc);
        let nodes = values.iter().find(|v| v.name == "node_count").unwrap();
        assert_eq!((nodes.section.as_str(), nodes.value), ("image", Some(2)));
        assert_eq!(nodes.reason, NO_REASON);
        let heap = values
            .iter()
            .find(|v| v.name == "heap_budget_bytes")
            .unwrap();
        assert_eq!(heap.value, None);
        assert!(heap.reason.starts_with("not derived"), "{}", heap.reason);
        let text = render_derived(&Derived {
            descriptor: "/ws/build/x/nros/sizing/e.toml".into(),
            values,
        });
        assert!(text.contains("image.node_count"), "{text}");
        assert!(text.contains("/ws/build/x/nros/sizing/e.toml"), "{text}");
    }

    /// phase-569 W3 — no descriptor yet names `nros configure` and builds
    /// nothing; a road that writes none says so.
    #[test]
    fn an_image_with_no_derived_configuration_names_nros_configure() {
        let i = item("linux", "linux", "b");
        let missing = std::env::temp_dir().join("nros-w3-no-such/nros/sizing/linux.toml");
        let e = derived_of(&i, Some(&missing)).unwrap_err().to_string();
        assert!(e.contains("nros configure"), "{e}");
        let e = derived_of(&i, None).unwrap_err().to_string();
        assert!(e.contains("writes no sizing descriptor"), "{e}");
    }

    #[test]
    fn an_image_is_found_by_its_name_or_its_qualified_name() {
        let items = [item("linux", "linux", "demo_bringup")];
        assert_eq!(find(&items, "linux").unwrap().id, "linux");
        assert_eq!(find(&items, "demo_bringup:linux").unwrap().id, "linux");
        let e = find(&items, "linx").unwrap_err().to_string();
        assert!(e.contains("did you mean `linux`?"), "{e}");
    }

    #[test]
    fn the_list_names_every_image_with_its_declaration() {
        let text = render_list(&[item("linux", "linux", "demo_bringup")]);
        assert!(text.starts_with("IMAGE"), "{text}");
        assert!(
            text.lines()
                .any(|l| l.split_whitespace().collect::<Vec<_>>()
                    == ["linux", "demo_bringup", "linux", "-", "system.launch.xml"]),
            "{text}"
        );
        assert!(render_list(&[]).starts_with("No images"));
    }

    /// The repo's Rust workspace lists the images its bringups declare.
    #[test]
    fn a_real_workspace_lists_the_images_its_bringups_declare() {
        let ws = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../examples/workspaces/rust")
            .canonicalize()
            .unwrap();
        let names: Vec<String> = items(&ws)
            .unwrap_or_else(|e| panic!("{}: {e:#}", ws.display()))
            .into_iter()
            .map(|i| i.name)
            .collect();
        assert!(names.iter().any(|n| n == "freertos_talker"), "{names:?}");
    }

    #[test]
    fn json_schema_names_are_pinned() {
        assert_eq!(LIST_SCHEMA, "nros.image.list.v1");
        assert_eq!(SHOW_SCHEMA, "nros.image.show.v1");
    }
}
