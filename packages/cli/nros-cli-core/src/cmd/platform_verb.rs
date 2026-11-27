//! `nros platform list` / `nros platform show <platform>` (RFC-0133,
//! phase-569 W6).
//!
//! The platforms on the descriptor search path: every
//! `<root>/<name>/nros-platform.toml` `PlatformsTree` loads, which is what a
//! board's `platform` and an image's build resolve against. A platform added
//! on `$NROS_PLATFORMS_DIR` is listed with no change to this verb.
//!
//! Each row names the `nros new --platform` word that scaffolds on it, read
//! from the board catalog's `PlatformKind` (its descriptor name and its deploy
//! word), the boards in the catalog that build on it, and the RMWs `nros rmw
//! list` verifies on one of those boards.

use std::path::Path;

use clap::{Args as ClapArgs, Subcommand};
use eyre::{Result, eyre};
use serde::Serialize;

use nros_board_common::platform_config::PlatformsTree;

use crate::{
    cmd::{
        frame::{self, Fields, Table},
        rmw::RmwItem,
    },
    orchestration::board_descriptor::{BoardCatalog, PlatformKind},
};

/// The `--json` schema names (RFC-0133 D1). Adding a key keeps the version;
/// removing or renaming one is a new version.
pub const LIST_SCHEMA: &str = "nros.platform.list.v1";
pub const SHOW_SCHEMA: &str = "nros.platform.show.v1";
/// The Book page for a platform with no guide page of its own.
pub const FALLBACK_PAGE: &str = "reference/supported-boards.html";

#[derive(Debug, Subcommand)]
pub enum Args {
    /// List the platforms on the descriptor search path.
    List(JsonArgs),
    /// Show one platform: its names, parent, boards and capabilities.
    Show(ShowArgs),
}

#[derive(Debug, ClapArgs)]
pub struct JsonArgs {
    /// Print one JSON object instead of text.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, ClapArgs)]
pub struct ShowArgs {
    /// The platform, by its name or an alias (`freertos`, `bare-metal`).
    pub platform: String,
    /// Print one JSON object instead of text.
    #[arg(long)]
    pub json: bool,
}

/// One platform descriptor.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PlatformItem {
    /// The name the platform resolves as (its descriptor's first `names`
    /// entry, else its directory).
    pub name: String,
    /// The other names it answers to.
    pub aliases: Vec<String>,
    /// The platform it builds on, as its descriptor writes it.
    pub inherits: Option<String>,
    /// The `nros new --platform` word that scaffolds on it, if any.
    pub new_platform: Option<&'static str>,
    /// The catalog boards that build on it, by their first name.
    pub boards: Vec<String>,
    /// The RMWs with a runtime test on one of its boards.
    pub rmws: Vec<String>,
    /// The capabilities it has, its parent's included.
    pub capabilities: Vec<String>,
    /// The `[arch.*]` profiles a C build on it admits, in order.
    pub arch_profiles: Vec<String>,
}

pub fn run(args: Args) -> Result<()> {
    match args {
        Args::List(a) => list(a),
        Args::Show(a) => show(a),
    }
}

/// What a platform's row is read from: the platform tree, the board catalog
/// and the RMWs with the boards each is verified on.
pub struct Sources {
    pub tree: PlatformsTree,
    pub catalog: BoardCatalog,
    pub rmws: Vec<RmwItem>,
}

/// The platform tree over the SDK root's search path, the board catalog of
/// the workspace the current directory is in, and `nros rmw list`'s RMWs.
pub(crate) fn load() -> Result<Sources> {
    let root = crate::cmd::board::catalog_root(None)?;
    let tree = load_tree(&root)?;
    let cwd = std::env::current_dir()?;
    let catalog = crate::cmd::board::catalog_for_dir(&root, &cwd)?;
    let rmws = crate::cmd::rmw::collect(None)?;
    Ok(Sources {
        tree,
        catalog,
        rmws,
    })
}

/// The platform tree a build under `root` resolves against:
/// `$NROS_PLATFORMS_DIR` first, then the in-tree homes.
pub fn load_tree(root: &Path) -> Result<PlatformsTree> {
    let path = PlatformsTree::default_search_path(
        root,
        std::env::var("NROS_PLATFORMS_DIR").ok().as_deref(),
    );
    PlatformsTree::load_search_path(&path).map_err(|e| eyre!("platform descriptors: {e}"))
}

/// The `nros new --platform` word for the platform `name` resolves as: the
/// deploy word of a built-in kind whose descriptor name resolves there, when
/// `nros new` offers it.
pub fn new_platform_word(tree: &PlatformsTree, name: &str) -> Option<&'static str> {
    PlatformKind::all_kebabs()
        .into_iter()
        .filter_map(PlatformKind::from_kebab)
        .filter(|k| tree.resolve_alias(k.kebab()) == name)
        .filter_map(PlatformKind::cmake_deploy)
        .find(|w| crate::cmd::new::OFFERED_PLATFORMS.contains(w))
}

/// The platform a board builds on, resolved through the tree's aliases.
fn board_platform<'a>(
    tree: &'a PlatformsTree,
    d: &'a crate::orchestration::board_descriptor::BoardDescriptor,
) -> &'a str {
    tree.resolve_alias(d.platform_name.as_deref().unwrap_or(d.platform.kebab()))
}

pub fn item(src: &Sources, name: &str) -> Result<PlatformItem> {
    let Sources {
        tree,
        catalog,
        rmws,
    } = src;
    let rmws = rmws
        .iter()
        .filter(|r| {
            r.boards.iter().any(|b| {
                b.platform
                    .as_deref()
                    .is_some_and(|p| tree.resolve_alias(p) == name)
            })
        })
        .map(|r| r.name.clone())
        .collect();
    let mut boards: Vec<String> = catalog
        .descriptors()
        .iter()
        .filter(|d| board_platform(tree, d) == name)
        .filter_map(|d| d.names.first().cloned())
        .collect();
    boards.sort();
    boards.dedup();
    let capabilities = tree
        .capabilities(name)
        .map_err(|e| eyre!("platform `{name}`: {e}"))?
        .into_iter()
        .filter_map(|(k, on)| on.then_some(k))
        .collect();
    Ok(PlatformItem {
        name: name.to_string(),
        aliases: tree
            .declared_names(name)
            .iter()
            .filter(|n| n.as_str() != name)
            .cloned()
            .collect(),
        inherits: tree.inherits_of(name).flatten().map(str::to_string),
        new_platform: new_platform_word(tree, name),
        boards,
        rmws,
        capabilities,
        arch_profiles: tree.arch_profiles(name).unwrap_or_default().to_vec(),
    })
}

/// Every platform on the tree, by name.
pub fn items(src: &Sources) -> Result<Vec<PlatformItem>> {
    src.tree.names().map(|n| item(src, n)).collect()
}

fn list(args: JsonArgs) -> Result<()> {
    let src = load()?;
    let items = items(&src)?;
    if args.json {
        return frame::print_json(LIST_SCHEMA, "items", &items, &[]);
    }
    print!("{}", render_list(&items));
    Ok(())
}

/// The name the platform `name` resolves as, or the refusal with the
/// nearest names.
pub fn find<'a>(tree: &'a PlatformsTree, name: &'a str) -> Result<&'a str> {
    let resolved = tree.resolve_alias(name);
    if tree.names().any(|n| n == resolved) {
        return Ok(resolved);
    }
    let known = tree.all_names();
    Err(frame::unknown(
        "platform",
        name,
        known.iter().map(String::as_str),
        "nros platform list",
    ))
}

fn show(args: ShowArgs) -> Result<()> {
    let src = load()?;
    let item = item(&src, find(&src.tree, &args.platform)?)?;
    if args.json {
        return frame::print_json(SHOW_SCHEMA, "item", &item, &[]);
    }
    print!("{}", render_show(&item));
    Ok(())
}

fn or_dash(v: Option<&str>) -> String {
    v.unwrap_or("-").to_string()
}

fn joined(v: &[String]) -> String {
    if v.is_empty() {
        "-".to_string()
    } else {
        v.join(", ")
    }
}

pub fn render_list(items: &[PlatformItem]) -> String {
    let mut t = Table::new(
        &["PLATFORM", "NEW", "BOARDS", "RMWS"],
        "No platforms on the descriptor search path.",
    );
    for i in items {
        t.row(vec![
            i.name.clone(),
            or_dash(i.new_platform),
            i.boards.len().to_string(),
            joined(&i.rmws),
        ]);
    }
    t.footer(
        "NEW is the `nros new --platform` word that scaffolds on it. \
         `nros platform show <platform>` names its boards.",
    );
    t.render()
}

/// The Book page for a platform: its guide page, else the board table.
pub fn book_page(i: &PlatformItem) -> String {
    match i.name.as_str() {
        "posix" => "guide/platforms/linux-host.html".to_string(),
        "bare-metal" | "freertos" | "nuttx" | "threadx" | "zephyr" => {
            format!("guide/platforms/{}.html", i.name)
        }
        _ => FALLBACK_PAGE.to_string(),
    }
}

pub fn render_show(i: &PlatformItem) -> String {
    let summary = match &i.inherits {
        Some(p) => format!("A platform that builds on `{p}`."),
        None => String::new(),
    };
    let mut out = frame::show_head(&i.name, &summary);
    let mut f = Fields::new();
    f.field("also", joined(&i.aliases))
        .field("inherits", or_dash(i.inherits.as_deref()))
        .field(
            "scaffold",
            i.new_platform.map_or("-".to_string(), |w| {
                format!("nros new <name> --platform {w}")
            }),
        )
        .field("boards", joined(&i.boards))
        .field("rmws", joined(&i.rmws))
        .field("capabilities", joined(&i.capabilities))
        .field("arch", joined(&i.arch_profiles));
    out.push_str(&f.render());
    out.push_str(&frame::book_line(&book_page(i)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .unwrap()
    }

    /// The repo's tree and catalog, with `first` searched before the in-tree
    /// homes, as `$NROS_PLATFORMS_DIR` is.
    fn sources(first: Option<&Path>, rmws: Vec<RmwItem>) -> Sources {
        let root = repo();
        let mut path: Vec<std::path::PathBuf> = first.map(Path::to_path_buf).into_iter().collect();
        path.extend(PlatformsTree::default_search_path(&root, None));
        Sources {
            tree: PlatformsTree::load_search_path(&path).unwrap(),
            catalog: BoardCatalog::load(&root).unwrap(),
            rmws,
        }
    }

    fn fixture() -> Sources {
        sources(None, Vec::new())
    }

    #[test]
    fn every_platform_on_the_tree_is_listed() {
        let src = fixture();
        let items = items(&src).unwrap();
        let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, src.tree.names().collect::<Vec<_>>());
        let text = render_list(&items);
        for n in names {
            assert!(text.contains(n), "{n} missing:\n{text}");
        }
    }

    /// RFC-0133 acceptance: a platform added on the search path is listed,
    /// with no CLI edit.
    #[test]
    fn a_platform_added_on_the_search_path_is_listed() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("fixture-rtos");
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(
            dir.join("nros-platform.toml"),
            "names = [\"fixture-rtos\", \"fixture\"]\ninherits = \"freertos\"\n",
        )
        .unwrap();
        let src = sources(Some(tmp.path()), Vec::new());
        let items = items(&src).unwrap();
        let i = items.iter().find(|i| i.name == "fixture-rtos").unwrap();
        assert_eq!(i.aliases, ["fixture"]);
        assert_eq!(i.inherits.as_deref(), Some("freertos"));
        assert_eq!(i.new_platform, None);
        assert!(i.boards.is_empty());
        assert!(i.capabilities.contains(&"ip_stack".to_string()), "{i:?}");
        assert!(render_list(&items).contains("fixture-rtos"));
        assert_eq!(find(&src.tree, "fixture").unwrap(), "fixture-rtos");
    }

    #[test]
    fn every_new_platform_word_scaffolds_on_a_listed_platform() {
        let items = items(&fixture()).unwrap();
        for w in crate::cmd::new::OFFERED_PLATFORMS {
            assert!(
                items.iter().any(|i| i.new_platform == Some(w)),
                "`nros new --platform {w}` names no listed platform"
            );
        }
        let posix = items.iter().find(|i| i.name == "posix").unwrap();
        assert_eq!(posix.new_platform, Some("linux"));
    }

    #[test]
    fn a_board_is_listed_under_the_platform_it_builds_on() {
        let src = fixture();
        // Listed by its key, the first name it declares.
        let key = src
            .catalog
            .descriptors()
            .iter()
            .find(|d| d.answers_to("mps2-an385-freertos"))
            .and_then(|d| d.names.first())
            .unwrap();
        let freertos = item(&src, "freertos").unwrap();
        assert!(freertos.boards.contains(key), "{:?}", freertos.boards);
        let posix = item(&src, "posix").unwrap();
        assert!(!posix.boards.contains(key), "{:?}", posix.boards);
    }

    #[test]
    fn an_rmw_is_listed_under_the_platform_of_a_board_it_is_verified_on() {
        let rmw = |name: &str, platform: &str| RmwItem {
            name: name.to_string(),
            aliases: Vec::new(),
            summary: String::new(),
            host_peer: String::new(),
            host_packages: Vec::new(),
            package: String::new(),
            boards: vec![crate::cmd::rmw::BoardRef {
                board: "b".to_string(),
                platform: Some(platform.to_string()),
            }],
            build_only_boards: Vec::new(),
        };
        // `threadx-linux` is an alias of `threadx`.
        let src = sources(
            None,
            vec![rmw("zenoh", "threadx-linux"), rmw("xrce", "zephyr")],
        );
        assert_eq!(item(&src, "threadx").unwrap().rmws, ["zenoh"]);
        assert_eq!(item(&src, "zephyr").unwrap().rmws, ["xrce"]);
        assert!(item(&src, "posix").unwrap().rmws.is_empty());
    }

    #[test]
    fn an_alias_resolves_and_an_unknown_name_is_refused_with_the_nearest() {
        let src = fixture();
        assert_eq!(find(&src.tree, "freertos-lwip").unwrap(), "freertos");
        let e = find(&src.tree, "zephir").unwrap_err().to_string();
        assert!(e.contains("did you mean `zephyr`?"), "{e}");
        assert!(e.contains("`nros platform list` names every one"), "{e}");
    }

    #[test]
    fn every_book_page_exists() {
        for i in items(&fixture()).unwrap() {
            let page = book_page(&i);
            let md = repo().join("book/src").join(page.replace(".html", ".md"));
            assert!(md.is_file(), "{}: no {}", i.name, md.display());
        }
    }

    #[test]
    fn json_schema_names_are_pinned() {
        assert_eq!(LIST_SCHEMA, "nros.platform.list.v1");
        assert_eq!(SHOW_SCHEMA, "nros.platform.show.v1");
    }
}
