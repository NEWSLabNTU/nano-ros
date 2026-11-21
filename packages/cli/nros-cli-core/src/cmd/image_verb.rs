//! `nros image list` / `nros image show <image>` (RFC-0133, phase-569 W7).
//!
//! `list` prints the workspace's images as its bringups declare them: board,
//! RMW and launch file, read from each `system.toml` and never planned, so it
//! is cheap and works before anything is configured. `show` adds what the
//! build resolves for one image (its platform, entry package, profile and the
//! packages it builds) through `plan_query`, which writes nothing. `check
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
}

pub fn run(args: Args) -> Result<()> {
    match args {
        Args::List(a) => list(a),
        Args::Show(a) => show(a),
        Args::Check(a) => check(a),
    }
}

/// The workspace `workspace` names, or the one the current directory is in.
fn root(workspace: Option<PathBuf>) -> Result<PathBuf> {
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
        .map(|(bringup, _, id, block)| ImageItem {
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
    let item = ImageShow {
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
    out.push_str(&frame::book_line(BOOK_PAGE));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str, id: &str, bringup: &str) -> ImageItem {
        ImageItem {
            name: name.into(),
            id: id.into(),
            bringup: bringup.into(),
            board: Some("linux".into()),
            rmw: None,
            launch: Some("system.launch.xml".into()),
        }
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
