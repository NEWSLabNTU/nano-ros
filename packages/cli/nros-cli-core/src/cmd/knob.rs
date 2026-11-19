//! `nros knob list|show` — the build-time knobs a project can state, and how
//! (RFC-0133, phase-569 W2).
//!
//! Every fact comes from the knob registry (`knobs.toml`, compiled into
//! `nros_zephyr_build::KNOBS`), never from a table here:
//!
//! - which knobs a user states, and where: the row's `class` and its derived
//!   `surface` (`[image.<id>.knobs.<group>] <name>`);
//! - the default: the Kconfig definition's `default` lines, kept with their
//!   conditions, or the platform/board ladder's builtin, which only a
//!   resolved platform can answer (`--platform`);
//! - the values: the Kconfig type, range and names;
//! - the words: the row's `prompt` and `help`.
//!
//! Neither verb prints an env spelling, a Kconfig symbol or a `-D` define:
//! those carry a decision between build layers and are never where a user
//! makes it (RFC-0133, "one field says which spellings are user surface").
//!
//! phase-581 W3 (RFC-0140 D5): both verbs read the MERGED catalog of the
//! workspace (`--workspace`, default the current directory): core's rows and
//! the in-tree packages' from the compiled table, and every out-of-tree
//! package's `nros-knobs.toml` the provider scan finds, each with the package
//! that declared it and the root it was found in. `nros knob render --kconfig
//! <package>` writes a package's own Zephyr module `Kconfig` from its rows, for
//! a package that is its own Zephyr module (a guest has no `nros configure`).

use std::path::PathBuf;

use clap::{Args as ClapArgs, Subcommand};
use eyre::{Result, eyre};
use nros_zephyr_build::{KNOBS, KnobClass, KnobDefault, KnobRow, KnobScope, KnobValues};

use crate::knob_catalog::{Catalog, Origin, Row, RowKind};
use serde::Serialize;

use crate::cmd::config::{ExplainArgs, Ladder, resolve_ladder};

/// The `--json` schema names (RFC-0133 D1). Adding a key keeps the version;
/// removing or renaming one is a new version.
pub const LIST_SCHEMA: &str = "nros.knob.list.v1";
pub const SHOW_SCHEMA: &str = "nros.knob.show.v1";

#[derive(Debug, Subcommand)]
pub enum Args {
    /// List every knob a project can state: its name, scope, default and
    /// what it is for
    List(ListArgs),
    /// Show one knob: where to state it, the values it takes, its default and
    /// its help
    Show(ShowArgs),
    /// Render a package's rows as a view it commits: `--kconfig`, the Kconfig
    /// of a package that is its own Zephyr module
    Render(RenderArgs),
}

#[derive(Debug, ClapArgs)]
pub struct RenderArgs {
    /// The package: a directory holding its `nros-knobs.toml`, or its
    /// `package.xml` name in the workspace's provider scan
    #[arg(long, value_name = "PACKAGE")]
    pub kconfig: String,
    /// The workspace whose packages are searched (default: the current directory)
    #[arg(long, value_name = "DIR")]
    pub workspace: Option<PathBuf>,
    /// Write here instead of stdout
    #[arg(long, value_name = "FILE")]
    pub out: Option<PathBuf>,
}

#[derive(Debug, ClapArgs)]
pub struct ListArgs {
    /// Print one JSON object instead of a table
    #[arg(long)]
    pub json: bool,
    /// Resolve each knob's value for this platform, and say which rung of the
    /// platform/board ladder set it (`builtin`, `platform`, `board`, `env`)
    #[arg(long, value_name = "PLATFORM")]
    pub platform: Option<String>,
    /// With `--platform`: the board's `nros-board.toml`, for its rung
    #[arg(long, value_name = "FILE", requires = "platform")]
    pub board_toml: Option<PathBuf>,
    /// The workspace whose packages are searched for their knobs (and, with
    /// `--platform`, its platform packages). Default: the current directory
    #[arg(long, value_name = "DIR")]
    pub workspace: Option<PathBuf>,
}

#[derive(Debug, ClapArgs)]
pub struct ShowArgs {
    /// The knob, as `nros knob list` names it (e.g. `net.max_sockets`)
    pub name: String,
    /// Print one JSON object instead of text
    #[arg(long)]
    pub json: bool,
    /// The workspace whose packages are searched for their knobs (default: the
    /// current directory)
    #[arg(long, value_name = "DIR")]
    pub workspace: Option<PathBuf>,
}

/// One knob, as `list` and `show` report it.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct KnobItem {
    /// `<group>.<name>`, the key under `[image.<id>.knobs]`.
    pub name: String,
    /// `image`, `board` or `platform`: what the value describes.
    pub scope: &'static str,
    /// `public`, `carrier`, `internal` or `retired`.
    pub class: &'static str,
    /// Where a project states it; `None` unless public.
    pub surface: Option<String>,
    /// Where a board's or platform's `[knobs]` table states it, if it can.
    pub board_key: Option<&'static str>,
    /// The values it takes, in words.
    pub values: String,
    /// The default.
    pub default: DefaultItem,
    /// One line on what it is for.
    pub summary: String,
    /// The full help text, where the registry has one, with every knob's
    /// carrier spelling replaced by its name.
    pub help: Option<String>,
    /// For a non-public knob: why it is not set directly.
    pub note: Option<String>,
    /// With `--platform`: the resolved value and the rung that set it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved: Option<Resolved>,
    /// The package that declared it (`package.xml` name); `None` for core.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    /// Where that package was found: `built in` for an in-tree package, else
    /// the provider scan's root.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
}

/// Where a knob's default comes from.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "from", rename_all = "snake_case")]
pub enum DefaultItem {
    /// Fixed values, the first whose condition holds; the last has none.
    Stated { choices: Vec<Choice> },
    /// The platform/board ladder's builtin: `nros knob list --platform P`.
    Ladder,
    /// Chosen by the code that reads it.
    Build,
}

/// One default, and the image it applies to.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Choice {
    pub value: String,
    /// In words; `None` for the fallback.
    pub when: Option<String>,
}

/// A ladder value for one platform.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Resolved {
    pub value: String,
    /// `builtin`, `platform`, `board` or `env`.
    pub set_by: &'static str,
}

pub fn run(args: Args) -> Result<()> {
    match args {
        Args::List(a) => list(a),
        Args::Show(a) => show(a),
        Args::Render(a) => render(a),
    }
}

/// The merged catalog of `workspace` (default: the current directory): the
/// provider scan `nros configure` runs, memoised in `build/nros/providers.json`.
pub fn workspace_catalog(workspace: Option<&std::path::Path>) -> Result<Catalog> {
    let ws = match workspace {
        Some(w) => w.to_path_buf(),
        None => std::env::current_dir()?,
    };
    let ws = ws.canonicalize().unwrap_or(ws);
    let scan = crate::cmd::ws::sync_provider_scan(&ws, &[], None)?;
    crate::knob_catalog::build_workspace(&scan.providers, Some(&scan))
}

/// The knobs of out-of-tree packages a user states, as items, sorted by name.
pub fn package_items(cat: &Catalog) -> Vec<KnobItem> {
    let mut items: Vec<KnobItem> = cat
        .package_rows()
        .filter(|r| r.kind == RowKind::Knob)
        .filter(|r| matches!(r.meta.class.as_deref(), None | Some("public")))
        .map(package_item)
        .collect();
    items.sort_by(|a, b| a.name.cmp(&b.name));
    items
}

/// An out-of-tree package's knob as an item: everything comes from its row.
pub fn package_item(r: &Row) -> KnobItem {
    let (group, name) = r.name.rsplit_once('.').unwrap_or(("", &r.name));
    let values = match (r.meta.ty.as_deref(), r.meta.range) {
        (Some("bool"), _) => "true or false".to_string(),
        (Some("int"), Some((lo, hi))) => format!("an integer from {lo} to {hi}"),
        (Some("int"), None) => "an integer".to_string(),
        (Some("hex"), _) => "a hex integer".to_string(),
        (Some("string"), _) => "a string".to_string(),
        _ => "an integer (not checked before the build)".to_string(),
    };
    let default = match &r.meta.default {
        Some(v) => DefaultItem::Stated {
            choices: vec![Choice {
                value: match (r.meta.ty.as_deref(), v.as_str()) {
                    (Some("bool"), "y") => "true".to_string(),
                    (Some("bool"), "n") => "false".to_string(),
                    _ => v.clone(),
                },
                when: None,
            }],
        },
        None => DefaultItem::Build,
    };
    let (package, root) = match &r.origin {
        Origin::Package { package, root, .. } => (
            Some(package.clone()),
            Some(root.clone().unwrap_or_else(|| "built in".to_string())),
        ),
        Origin::Core => (None, None),
    };
    KnobItem {
        name: r.name.clone(),
        scope: match r.meta.scope.as_deref() {
            Some("board") => "board",
            Some("platform") => "platform",
            _ => "image",
        },
        class: "public",
        surface: Some(format!("[image.<id>.knobs.{group}] {name}")),
        board_key: None,
        values,
        default,
        summary: r
            .meta
            .prompt
            .clone()
            .or_else(|| {
                r.meta
                    .help
                    .as_deref()
                    .and_then(|h| h.lines().map(str::trim).find(|l| !l.is_empty()))
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "(no description yet)".to_string()),
        help: r.meta.help.clone(),
        note: None,
        resolved: None,
        package,
        root,
    }
}

/// `nros knob render --kconfig <package>` (RFC-0140 D5): the package's own
/// Zephyr module `Kconfig`, every typed knob WITH its prompt, for a guest
/// project (no `system.toml`, so no `nros configure` and no
/// `Kconfig.packages`) to set from `prj.conf`. The package commits it.
fn render(args: RenderArgs) -> Result<()> {
    let dir = PathBuf::from(&args.kconfig);
    let file = dir.join(crate::knob_catalog::PACKAGE_FILE);
    let (label, rows): (String, Vec<Row>) = if file.is_file() {
        let text =
            std::fs::read_to_string(&file).map_err(|e| eyre!("reading {}: {e}", file.display()))?;
        let (_, rows) = crate::knob_catalog::parse_own_file(&args.kconfig, &file, &text)?;
        (file.display().to_string(), rows)
    } else {
        let cat = workspace_catalog(args.workspace.as_deref())?;
        let rows: Vec<Row> = cat
            .rows()
            .iter()
            .filter(|r| r.origin.package() == Some(args.kconfig.as_str()))
            .cloned()
            .collect();
        if rows.is_empty() {
            return Err(eyre!(
                "`{}` is neither a directory holding {} nor a package with configuration \
                 rows in this workspace",
                args.kconfig,
                crate::knob_catalog::PACKAGE_FILE
            ));
        }
        (format!("package {}", args.kconfig), rows)
    };
    let knobs: Vec<&Row> = rows
        .iter()
        .filter(|r| r.kind == RowKind::Knob && r.meta.ty.is_some())
        .collect();
    let body = format!(
        "# @generated by `nros knob render --kconfig` from {label} -- do not\n\
         # edit; re-render after changing the rows. This package's knobs, with their\n\
         # prompts, for an application that builds it as its own Zephyr module.\n{}",
        crate::knob_catalog::kconfig_rows(&knobs, &Default::default(), true)
    );
    match &args.out {
        Some(out) => crate::atomic_file::atomic_write(out, &body)
            .map_err(|e| eyre!("writing {}: {e}", out.display()))?,
        None => print!("{body}"),
    }
    Ok(())
}

fn list(args: ListArgs) -> Result<()> {
    let ladder = match &args.platform {
        Some(p) => Some(resolve_ladder(&ExplainArgs {
            platform: p.clone(),
            board_toml: args.board_toml.clone(),
            platforms_dir: None,
            workspace: args.workspace.clone(),
        })?),
        None => None,
    };
    let mut items = public_items(ladder.as_ref());
    let cat = workspace_catalog(args.workspace.as_deref())?;
    items.extend(package_items(&cat));
    items.sort_by(|a, b| a.name.cmp(&b.name));
    if args.json {
        let doc = serde_json::json!({
            "schema": LIST_SCHEMA,
            "platform": args.platform,
            "items": items,
        });
        println!("{}", serde_json::to_string_pretty(&doc)?);
        return Ok(());
    }
    print!("{}", render_list(&items, ladder.is_some()));
    Ok(())
}

fn show(args: ShowArgs) -> Result<()> {
    let item = match find(&args.name) {
        Ok(row) => item(row, None),
        Err(e) => {
            // phase-581 W3 — an out-of-tree package's knob is in the merged
            // catalog only.
            let cat = workspace_catalog(args.workspace.as_deref())?;
            match cat
                .find(&args.name)
                .filter(|r| r.kind == RowKind::Knob && !r.origin.is_compiled())
            {
                Some(r) => package_item(r),
                None => return Err(e),
            }
        }
    };
    if args.json {
        let doc = serde_json::json!({ "schema": SHOW_SCHEMA, "item": item });
        println!("{}", serde_json::to_string_pretty(&doc)?);
        return Ok(());
    }
    print!("{}", render_show(&item));
    Ok(())
}

/// Every public knob, sorted by name, with its ladder value where resolved.
pub fn public_items(ladder: Option<&Ladder>) -> Vec<KnobItem> {
    let mut items: Vec<KnobItem> = nros_zephyr_build::public_knobs()
        .map(|r| item(r, ladder))
        .collect();
    items.sort_by(|a, b| a.name.cmp(&b.name));
    items
}

/// The row a user's name for a knob names: its `[image.<id>.knobs]` key, its
/// board `[knobs]` key, its environment spelling (`NROS_…`, or a retired one),
/// its Kconfig symbol (`CONFIG_NROS_…`), or the bare registry id. A user who
/// meets a knob in a build log or a Zephyr config can look it up as written.
/// A refusal lists the closest public names.
pub fn find(name: &str) -> Result<&'static KnobRow> {
    let bare = name.strip_prefix("CONFIG_").unwrap_or(name);
    if let Some(r) = KNOBS
        .iter()
        .find(|r| r.image_key == name || r.toml == Some(name))
        .or_else(|| nros_zephyr_build::knob_row(bare))
        .or_else(|| {
            KNOBS.iter().find(|r| {
                r.kconfig == name || r.kconfig.strip_prefix("CONFIG_") == Some(bare) || r.id == bare
            })
        })
    {
        return Ok(r);
    }
    let mut near: Vec<&str> = nros_zephyr_build::public_knobs()
        .map(|r| r.image_key)
        .filter(|k| {
            let (g, _) = k.rsplit_once('.').unwrap_or(("", k));
            name.starts_with(g)
                || k.contains(name)
                || name.contains(k.rsplit('.').next().unwrap_or(k))
        })
        .collect();
    near.sort_unstable();
    near.truncate(8);
    let hint = if near.is_empty() {
        String::new()
    } else {
        format!("; the closest are: {}", near.join(", "))
    };
    Err(eyre!(
        "no knob named `{name}`{hint}\n  `nros knob list` names every knob a project can state"
    ))
}

pub fn item(r: &KnobRow, ladder: Option<&Ladder>) -> KnobItem {
    let resolved = ladder.and_then(|l| {
        let key = r.toml?;
        l.values.iter().find(|v| v.key == key).map(|v| Resolved {
            value: v.value.clone(),
            set_by: v.source,
        })
    });
    KnobItem {
        name: r.image_key.to_string(),
        scope: scope_word(r.scope),
        class: class_word(r.class),
        surface: r.surface.map(str::to_string),
        board_key: r.toml.filter(|_| r.class == KnobClass::Public),
        values: values_text(r),
        default: default_item(r),
        summary: summary(r),
        help: r.help.map(user_words),
        note: note(r),
        resolved,
        package: nros_zephyr_build::IN_TREE_PACKAGES
            .iter()
            .find(|p| p.knobs.contains(&r.image_key))
            .map(|p| p.package.to_string()),
        root: nros_zephyr_build::IN_TREE_PACKAGES
            .iter()
            .any(|p| p.knobs.contains(&r.image_key))
            .then(|| "built in".to_string()),
    }
}

fn scope_word(s: KnobScope) -> &'static str {
    match s {
        KnobScope::Image => "image",
        KnobScope::Board => "board",
        KnobScope::Platform => "platform",
    }
}

fn class_word(c: KnobClass) -> &'static str {
    match c {
        KnobClass::Public => "public",
        KnobClass::Carrier => "carrier",
        KnobClass::Internal => "internal",
        KnobClass::Retired => "retired",
    }
}

/// Why a non-public knob is not set directly, in user words.
fn note(r: &KnobRow) -> Option<String> {
    match r.class {
        KnobClass::Public => None,
        KnobClass::Carrier => Some(match r.carries {
            Some(c) => match KNOBS.iter().find(|k| k.id == c) {
                Some(k) => format!(
                    "not set directly: it carries `{}`; set that instead",
                    k.image_key
                ),
                None if c.contains('.') && !c.contains(' ') => {
                    format!(
                        "not set directly: it carries the board's `[knobs] {c}`; set that instead"
                    )
                }
                None => format!("not set directly: it carries {c}"),
            },
            None => "not set directly".to_string(),
        }),
        KnobClass::Internal => Some("not set in a project: a test or tooling hook".to_string()),
        KnobClass::Retired => Some(format!(
            "retired: {}",
            r.retired.unwrap_or("it no longer exists")
        )),
    }
}

/// `text` with each knob's env, retired or Kconfig spelling replaced by the
/// name a project states it by (`NROS_SMOLTCP_MAX_SOCKETS` and
/// `CONFIG_NROS_MAX_PUBLISHERS` become `net.max_sockets` and
/// `zenoh.max_publishers`). The registry's help was written for menuconfig,
/// where those are the names; here the project's name is.
pub fn user_words(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        if !word.is_empty() {
            let bare = word.strip_prefix("CONFIG_").unwrap_or(word);
            let row = nros_zephyr_build::knob_row(bare).or_else(|| {
                KNOBS
                    .iter()
                    .find(|r| r.kconfig.strip_prefix("CONFIG_") == Some(bare))
            });
            match row {
                Some(r) => out.push_str(&format!("`{}`", r.image_key)),
                None => out.push_str(word),
            }
            word.clear();
        }
    };
    for c in text.chars() {
        if c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_' {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    // A spelling that was already in backticks is now doubled.
    out.replace("``", "`")
}

/// The prompt, else the first line of the help, else nothing stated.
fn summary(r: &KnobRow) -> String {
    if let Some(p) = r.prompt {
        return user_words(p);
    }
    match r
        .help
        .and_then(|h| h.lines().map(str::trim).find(|l| !l.is_empty()))
    {
        Some(l) => user_words(l),
        None => "(no description yet)".to_string(),
    }
}

fn values_text(r: &KnobRow) -> String {
    match r.values {
        KnobValues::Bool => "true or false".to_string(),
        KnobValues::Int => {
            let mut t = match r.range {
                Some((lo, hi)) => format!("an integer from {lo} to {hi}"),
                None => "an integer".to_string(),
            };
            if !r.names.is_empty() {
                t.push_str(&format!(", or one of: {}", r.names.join(", ")));
            }
            t
        }
        KnobValues::Hex => "a hex integer".to_string(),
        KnobValues::String => "a string".to_string(),
        KnobValues::UInt => "an integer of 0 or more".to_string(),
        KnobValues::Unstated => "an integer (not checked before the build)".to_string(),
    }
}

fn default_item(r: &KnobRow) -> DefaultItem {
    match r.default {
        KnobDefault::Kconfig(defaults) => DefaultItem::Stated {
            choices: defaults
                .iter()
                .map(|d| Choice {
                    value: value_word(r, d.value),
                    when: d.when.map(|w| condition_text(w).to_string()),
                })
                .collect(),
        },
        KnobDefault::Ladder => DefaultItem::Ladder,
        KnobDefault::Unstated => DefaultItem::Build,
    }
}

/// A Kconfig default value as `system.toml` spells it.
fn value_word(r: &KnobRow, v: &str) -> String {
    match (r.values, v) {
        (KnobValues::Bool, "y") => "true".to_string(),
        (KnobValues::Bool, "n") => "false".to_string(),
        (KnobValues::String, v) => v.trim_matches('"').to_string(),
        _ => v.to_string(),
    }
}

/// The condition of a conditional default, in user words. A condition with no
/// entry here is shown verbatim and fails `every_condition_has_words`, so a
/// new conditional default arrives with its sentence.
pub fn condition_text(cond: &str) -> &str {
    CONDITIONS
        .iter()
        .find(|(c, _)| *c == cond)
        .map_or(cond, |(_, t)| t)
}

const CONDITIONS: &[(&str, &str)] = &[
    ("NROS_RMW_CYCLONEDDS", "the image uses Cyclone DDS"),
    (
        "NROS_CAPABILITY_PARAM_SERVICES || NROS_PARAM_STORE || NROS_CAPABILITY_LIFECYCLE",
        "the image has parameter services, a parameter store or lifecycle nodes",
    ),
    ("RUST", "the image has Rust code"),
    (
        "(NROS_ZENOH_LINK_SERIAL || NROS_ZENOH_LINK_CAN || NROS_ZENOH_LINK_ISOTP) && \
         !NROS_ZENOH_LINK_TCP && !NROS_ZENOH_LINK_UDP_UNICAST && \
         !NROS_ZENOH_LINK_UDP_MULTICAST && !NROS_ZENOH_LINK_WS && \
         !NROS_ZENOH_RAWETH_TRANSPORT",
        "every zenoh link the image builds is serial or CAN",
    ),
];

/// The default on one line.
pub fn default_text(d: &DefaultItem) -> String {
    match d {
        DefaultItem::Stated { choices } if choices.len() == 1 => choices[0].value.clone(),
        DefaultItem::Stated { choices } => choices
            .iter()
            .map(|c| match &c.when {
                Some(w) => format!("{} if {w}", c.value),
                None => format!("else {}", c.value),
            })
            .collect::<Vec<_>>()
            .join("; "),
        DefaultItem::Ladder => "per platform".to_string(),
        DefaultItem::Build => "chosen by the build".to_string(),
    }
}

/// A short form for the table: a conditional default shows its fallback.
fn default_cell(d: &DefaultItem) -> String {
    match d {
        DefaultItem::Stated { choices } if choices.len() > 1 => format!(
            "{} (varies)",
            choices.last().map_or("", |c| c.value.as_str())
        ),
        _ => default_text(d),
    }
}

pub fn render_list(items: &[KnobItem], resolved: bool) -> String {
    let w = items.iter().map(|i| i.name.len()).max().unwrap_or(4).max(4);
    let p = items
        .iter()
        .map(|i| i.package.as_deref().unwrap_or("core").len())
        .max()
        .unwrap_or(7)
        .max(7);
    let r = items
        .iter()
        .map(|i| i.root.as_deref().unwrap_or("-").len())
        .max()
        .unwrap_or(4)
        .max(4);
    let mut out = String::new();
    if resolved {
        out.push_str(&format!(
            "{:<w$}  {:<8}  {:<12}  {:<9}  {}\n",
            "KNOB", "SCOPE", "VALUE", "SET BY", "WHAT IT IS"
        ));
    } else {
        out.push_str(&format!(
            "{:<w$}  {:<8}  {:<20}  {:<p$}  {:<r$}  {}\n",
            "KNOB", "SCOPE", "DEFAULT", "PACKAGE", "ROOT", "WHAT IT IS"
        ));
    }
    for i in items {
        if resolved {
            let (v, by) = match &i.resolved {
                Some(r) => (r.value.clone(), r.set_by),
                None => (default_cell(&i.default), "-"),
            };
            out.push_str(&format!(
                "{:<w$}  {:<8}  {:<12}  {:<9}  {}\n",
                i.name, i.scope, v, by, i.summary
            ));
        } else {
            out.push_str(&format!(
                "{:<w$}  {:<8}  {:<20}  {:<p$}  {:<r$}  {}\n",
                i.name,
                i.scope,
                default_cell(&i.default),
                i.package.as_deref().unwrap_or("core"),
                i.root.as_deref().unwrap_or("-"),
                i.summary
            ));
        }
    }
    out.push_str(&format!(
        "\n{} knobs. `nros knob show <knob>` says where to state one and what it takes.\n",
        items.len()
    ));
    out
}

pub fn render_show(i: &KnobItem) -> String {
    let mut out = format!("{}\n  {}\n\n", i.name, i.summary);
    if let Some(n) = &i.note {
        out.push_str(&format!("  {n}\n"));
        return out;
    }
    if let Some(s) = &i.surface {
        out.push_str(&format!("  set it in:  system.toml, {s}\n"));
    }
    if let Some(k) = i.board_key {
        out.push_str(&format!(
            "  or for a board:  nros-board.toml, [knobs] {k}\n"
        ));
    }
    out.push_str(&format!("  scope:      {}\n", i.scope));
    out.push_str(&format!(
        "  package:    {}\n",
        i.package.as_deref().unwrap_or("core")
    ));
    if let Some(r) = &i.root {
        out.push_str(&format!("  root:       {r}\n"));
    }
    out.push_str(&format!("  values:     {}\n", i.values));
    match &i.default {
        DefaultItem::Stated { choices } if choices.len() > 1 => {
            out.push_str("  default:    the first that applies:\n");
            for c in choices {
                match &c.when {
                    Some(w) => out.push_str(&format!("                {} if {w}\n", c.value)),
                    None => out.push_str(&format!("                {} otherwise\n", c.value)),
                }
            }
        }
        DefaultItem::Ladder => out.push_str(
            "  default:    set per platform and board; `nros knob list --platform <p>` shows it\n",
        ),
        d => out.push_str(&format!("  default:    {}\n", default_text(d))),
    }
    if let Some(h) = &i.help {
        out.push('\n');
        for l in h.lines() {
            if l.trim().is_empty() {
                out.push('\n');
            } else {
                out.push_str(&format!("  {l}\n"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every conditional default in the registry has its sentence.
    #[test]
    fn every_condition_has_words() {
        for r in KNOBS {
            if let KnobDefault::Kconfig(ds) = r.default {
                for d in ds {
                    if let Some(w) = d.when {
                        assert_ne!(condition_text(w), w, "{}: no words for `{w}`", r.id);
                    }
                }
            }
        }
    }

    /// The acceptance case: `show net.max_sockets` names its system.toml
    /// spelling and no carrier (no env, no Kconfig, no `-D`).
    #[test]
    fn show_names_the_surface_and_no_carrier() {
        let i = item(find("net.max_sockets").unwrap(), None);
        let text = render_show(&i);
        assert!(
            text.contains("system.toml, [image.<id>.knobs.net] max_sockets"),
            "{text}"
        );
        assert!(
            text.contains("nros-board.toml, [knobs] net.max_sockets"),
            "{text}"
        );
        for bad in ["NROS_", "CONFIG_", "-D", "ZPICO_"] {
            assert!(!text.contains(bad), "`{bad}` in:\n{text}");
        }
    }

    /// No knob's text, in any form, leaks a carrier spelling of ITSELF.
    #[test]
    fn no_item_prints_its_own_carrier_spelling() {
        for r in nros_zephyr_build::public_knobs() {
            let i = item(r, None);
            let text = render_show(&i) + &serde_json::to_string(&i).unwrap();
            assert!(!text.contains(r.env), "{}: env spelling shown", r.id);
            assert!(!text.contains(r.kconfig), "{}: Kconfig symbol shown", r.id);
        }
    }

    /// A conditional default is shown whole, never as one value.
    #[test]
    fn conditional_default_is_shown_whole() {
        let i = item(find("zephyr.heap_size").unwrap(), None);
        let text = render_show(&i);
        for v in [
            "1048576 if the image uses Cyclone DDS",
            "131072 if the image has Rust code",
            "65536 otherwise",
        ] {
            assert!(text.contains(v), "`{v}` missing:\n{text}");
        }
        assert_eq!(default_cell(&i.default), "65536 (varies)");
    }

    #[test]
    fn unknown_name_is_refused_with_near_matches() {
        let e = find("net.max_socket").unwrap_err().to_string();
        assert!(
            e.contains("no knob named `net.max_socket`") && e.contains("net.max_sockets"),
            "{e}"
        );
    }

    #[test]
    fn a_knob_is_found_by_every_spelling_a_user_meets() {
        let r = nros_zephyr_build::knob_row("NROS_GRAPH_MAX_ENTITIES").expect("row");
        for spelling in [
            r.image_key,
            r.env,
            r.kconfig,
            r.kconfig.strip_prefix("CONFIG_").unwrap_or(r.kconfig),
            r.id,
        ] {
            assert_eq!(find(spelling).expect(spelling).id, r.id, "{spelling}");
        }
    }

    #[test]
    fn carrier_points_at_what_it_carries() {
        let i = item(find("freertos.heap_kb").unwrap(), None);
        assert_eq!(i.class, "carrier");
        assert!(i.surface.is_none());
        assert!(
            render_show(&i).contains("[knobs] memory.heap_bytes"),
            "{}",
            render_show(&i)
        );
    }
}
