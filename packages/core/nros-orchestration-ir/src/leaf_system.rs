//! RFC-0098 D3/D5/D8 (phase-445 W3) — what ONE package deploys to, and where
//! that is written.
//!
//! A single-package example (a directory holding `Cargo.toml` or
//! `CMakeLists.txt` + `package.xml`, with no enclosing workspace) states its
//! board, RMW, domain, network identity and node declaration in a
//! `system.toml` beside its manifest — the SAME schema a workspace bringup
//! uses (`[system]`, `[[component]]`, `[image.<id>]`). This module is the one
//! reader of that declaration, shared by every consumer that needs it:
//!
//! * `nros::main!()` (the proc-macro cannot depend on `nros-cli-core`, which is
//!   why this lives here — the same reason `model_location` does);
//! * `nros sync` / `nros ws board-facts` / `nros ws leaf-system` and the entity
//!   budget derivation in `nros-cli-core`;
//! * the C/C++ lanes, through `nros ws leaf-system`, which prints this module's
//!   answer as `KEY=VALUE` lines for cmake.
//!
//! It walks `toml::Value` rather than deserialising a mirror struct, on
//! purpose: the typed, `deny_unknown_fields` schema is `nros-cli-core`'s
//! `SystemToml`, and every `nros sync` parses the file through it, so a typo is
//! still refused. A second typed struct here would be a second schema that can
//! drift — exactly what this crate exists to prevent.
//!
//! # An ENTRY inside a workspace — the bringup's image states it
//!
//! A workspace entry package (a hand-written Zephyr west application, a test
//! fixture's entry, and the entry `nros build` GENERATES under
//! `build/<coord>/<id>_entry/`) is not its own image: the bringup's
//! `[image.<id>]` is (RFC-0065 D6, RFC-0098 D5). [`for_entry`] finds the image
//! that claims an entry — `entry = "<pkg>"`, else the `<id>_entry` name the
//! builder gives a generated one ([`entry_package_name`]) — and answers from it,
//! through the same overlay as a leaf's `system.toml`. A `system.toml` beside a
//! workspace entry would make `nros build` read it as a second bringup, so the
//! bringup is the only place such an entry's deployment can live.
//!
//! # The retired manifest keys are REFUSED
//!
//! Before RFC-0098 a Rust leaf spelled all of this in `Cargo.toml`:
//! `[package.metadata.nros.entry] deploy`, `[package.metadata.nros.deploy.<b>]`,
//! `[package.metadata.nros.node]` and `[package.metadata.nros.component]`.
//! phase-445 W3/W3b read them through a deprecated fallback while the leaves
//! were converted; W5 converted the last workspace entries and deleted it. The
//! deployment keys are now an ERROR wherever they appear ([`read`]) — naming
//! the file to write — because a key nothing reads is a fact that silently
//! stopped being true. `[package.metadata.nros.node]` / `component` on a
//! workspace NODE package are node REGISTRATION — the Rust spelling of a C/C++
//! package's `nros_components_register_node` (issue 1289) — not a deployment,
//! and are not this reader's business. Nor is an `[package.metadata.nros.entry]`
//! table what makes a package an entry: an image's claim is
//! ([`claiming_image`]).

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// The file a single-package leaf states its deployment in.
pub const SYSTEM_TOML: &str = "system.toml";

/// Where a [`LeafSystem`] was read from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// `<leaf>/system.toml` — the RFC-0098 home.
    SystemToml(PathBuf),
    /// A workspace bringup's `system.toml`, whose `[image.<id>]` claims the
    /// entry ([`for_entry`]).
    Bringup(PathBuf),
}

/// The package name `nros build` gives the entry it generates for
/// `[image.<id>]` — `linux_robot1` → `linux_robot1_entry`.
///
/// Lives here, not in the CLI's builder, because [`for_entry`] (and so
/// `nros::main!`, which cannot depend on the CLI) must map a generated entry
/// back to its image by the SAME rule the builder named it by. Two copies of
/// this rule would be a generated entry that cannot find its own image.
#[must_use]
pub fn entry_package_name(image_id: &str) -> String {
    format!("{}_entry", image_id.replace(['-', '.', '/'], "_"))
}

/// The generated selection package a package `pkg` depends on —
/// `qemu-bsp-talker` → `qemu_bsp_talker_nros_selection`.
///
/// Issue 1305 / RFC-0098 D6 (amendment 2026-10-11). A workspace entry has had
/// one since phase-315 (`<entry>_nros_selection`); a single-package leaf gets
/// the same crate, so a board switch touches no `Cargo.toml`. The name is the
/// crate IDENT form (hyphens folded), so the manifest key, the package name and
/// the path `nros::main!` emits are one spelling. Lives here for the same
/// reason as [`entry_package_name`]: the macro must compute it by the SAME
/// rule the CLI writes it by.
#[must_use]
pub fn selection_crate_name(pkg: &str) -> String {
    format!("{}_nros_selection", pkg.replace('-', "_"))
}

/// Where a single-package leaf's generated selection package lives, relative
/// to the leaf: `build/nros/selection` (RFC-0103 D7 puts generated artifacts
/// under `<build>/nros/`).
///
/// ONE directory per leaf, independent of the image and its board, because
/// the leaf manifest names it as a path dependency and that line must survive
/// a board switch. A leaf with several images gets the selection of the image
/// being synced or built: each `nros sync` / `nros build` rewrites it for its
/// image (write-if-changed, so a rebuild of the same image touches nothing).
pub const LEAF_SELECTION_DIR: &str = "build/nros/selection";

/// The environment variable a single-package leaf's generated settings file
/// (`build/<image>/nros-cargo.toml`) sets to the image it builds (issue 1305).
///
/// A cargo leaf may declare several images (one per RMW and ROS edition), and
/// cargo is the only thing between `nros build <id>` and `nros::main!`, so the
/// image rides the settings file's `[env]` the way the board facts do. The
/// macro reads that image ([`read_image`]); unset, it applies [`read`]'s own
/// rule. Lives here because the macro and the CLI must spell it the same.
pub const IMAGE_ENV: &str = "NROS_IMAGE";

/// The link kinds `[image.<id>] transport` may name (RFC-0098 D4). This is the
/// one list: the `[knobs.transport]` tenant that once restated it in
/// `nros-platform-config` was retired by issue 2006, since no build read it.
///
/// An unknown value is an
/// ERROR here for RFC-0086 D2's own reason — a typo that silently selected
/// nothing would leave every implication unapplied and the image would build
/// with the wrong links on, which is exactly what
/// `examples/mps2-an385-baremetal/rust/talker-xrce` did: it named the RMW
/// (`transport = "xrce"`) where the link kind goes. (One reader DID read the
/// key — the mps2 board's XRCE registration, comparing it with `"xrce"` — so
/// the W6 rename silently unregistered that image's backend, issue 1601. The
/// value now reaches boards as the typed `nros_platform::LinkKind`, which
/// `nros::main!` maps from this list.)
pub const TRANSPORT_KINDS: &[&str] = &["serial", "tcp", "udp"];

/// Deployment identity (RFC-0098 D5): what the image dials and what it is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Network {
    pub domain_id: Option<u32>,
    pub locator: Option<String>,
    /// `[image.<id>.net.<if>]` over `[image_defaults.net.<if>]` (RFC-0103 D10),
    /// checked against the networks without the board
    /// (`crate::system_net::check_image_interfaces`). The CLI's resolver
    /// combines it with the board and hands the answer to every build as
    /// `NROS_IMAGE_NET`; `nros::main!` only needs to know whether there IS one.
    pub net: BTreeMap<String, crate::system_net::ImageNetIf>,
    /// `[network.<name>]`, the project's networks.
    pub networks: BTreeMap<String, crate::system_net::NetworkDef>,
    /// `[system] name` — part of the key a MAC the image does not state is
    /// derived from (D10b), so two projects' images with the same id differ.
    pub system: Option<String>,
    pub transport: Option<String>,
}

/// One declared node (`[[component]]`, RFC-0098 D8 for `entities`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LeafComponent {
    /// `package.xml` name of the package providing the class. `None` when the
    /// row leaves it out (the leaf's own package, then).
    pub pkg: Option<String>,
    pub class: Option<String>,
    pub name: Option<String>,
    /// Declared entities, in `EntityDecl::parse` grammar, for a board whose
    /// component cannot be host-probed (issues 1556, 1603; issue 1265 retired
    /// the esp32-c3 / mps2 cases). `None` = not declared;
    /// `Some(vec![])` = declared to have none.
    pub entities: Option<Vec<String>>,
    /// Issue 1556 -- `entities = "census"`: nothing is authored, and this
    /// component's entities are what the leaf's own program creates, read from
    /// its census (`nros ws entity-census take --leaf`). `entities` is `None`
    /// when this is set; the two are one key, so a component cannot state both.
    pub entities_from_census: bool,
    /// Dispatch strategy (`"inline"` | `"deferred"` | `"from_isr"`,
    /// phase-216 A.5), replacing `[package.metadata.nros.node] dispatch`
    /// (issue 1278). Read by `nros check`'s framework × dispatch lint; `None`
    /// = the trait default. Validated there, not here, so the lint keeps its
    /// one diagnostic for a typo.
    pub dispatch: Option<String>,
}

/// One `[image.<id>.knobs]` value, as TOML typed it (phase-484 W8).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KnobValue {
    Int(i64),
    Bool(bool),
    Str(String),
}

impl std::fmt::Display for KnobValue {
    /// The value as it is written in TOML (`16`, `true`, `"warn"`).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KnobValue::Int(n) => write!(f, "{n}"),
            KnobValue::Bool(b) => write!(f, "{b}"),
            KnobValue::Str(s) => write!(f, "{s:?}"),
        }
    }
}

/// Flatten a `knobs` table into `<group>.<name>` → value: nested tables join
/// their keys with `.`, so `[knobs.zenoh.tx] batch = true` is `zenoh.tx.batch`.
/// A value directly under `knobs` (no group), an array, a float or a date is an
/// error naming the key; `table` is how the caller spells the table
/// (`[image.talker.knobs]`).
pub fn flatten_knobs(t: &toml::Table, table: &str) -> Result<BTreeMap<String, KnobValue>, String> {
    fn walk(
        t: &toml::Table,
        prefix: &str,
        table: &str,
        out: &mut BTreeMap<String, KnobValue>,
    ) -> Result<(), String> {
        for (k, v) in t {
            let key = if prefix.is_empty() {
                k.clone()
            } else {
                format!("{prefix}.{k}")
            };
            let value = match v {
                toml::Value::Table(inner) => {
                    walk(inner, &key, table, out)?;
                    continue;
                }
                _ if prefix.is_empty() => {
                    return Err(format!(
                        "`{table}` row `{k}` has no group -- a knob is \
                         `[<table>.<group>] <name> = <value>`, e.g. `[image.<id>.knobs.zephyr] \
                         heap_size = 262144`"
                    ));
                }
                toml::Value::Integer(n) => KnobValue::Int(*n),
                toml::Value::Boolean(b) => KnobValue::Bool(*b),
                toml::Value::String(s) => KnobValue::Str(s.clone()),
                other => {
                    return Err(format!(
                        "`{table}` knob `{key}` is a {}, and a knob is an integer, a bool or \
                         a string",
                        other.type_str()
                    ));
                }
            };
            out.insert(key, value);
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    walk(t, "", table, &mut out)?;
    Ok(out)
}

/// A leaf's resolved deployment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeafSystem {
    pub origin: Origin,
    /// The `[image.<id>]` this answer describes.
    pub image: Option<String>,
    /// The board the image is built for, as the image names it.
    pub board: Option<String>,
    /// RMW: `[image] rmw` > `[image_defaults] rmw` > `[system] rmw`.
    pub rmw: Option<String>,
    pub network: Network,
    pub components: Vec<LeafComponent>,
    /// `[image.<id>] env` — the RFC-0049 APP rung, stated per image
    /// (RFC-0098 D4, phase-445 W6).
    ///
    /// A build knob whose value is neither a board fact (it differs between two
    /// images of the same board) nor derivable from the declarations. The KEY is
    /// the knob's env front-end — the spelling `executor_env_key` /
    /// `xrce_env_key` already publish and every build script already reads — so
    /// this rung needs no second name for anything.
    ///
    /// It replaces the `[env]` block a leaf used to hand-write in its own
    /// `.cargo/config.toml`, and it lands in `build/<image>/nros-cargo.toml`'s
    /// `[env]` WITHOUT `force`, so the ladder still holds: a lane that exports
    /// the variable outranks it.
    ///
    /// Reach for a board's `[knobs]` FIRST. This is the rung for a fact
    /// that is genuinely this image's — `examples/mps2-an385-baremetal/rust/
    /// talker-xrce`'s XRCE transport budget, on a board whose other twelve
    /// images want the defaults.
    pub env: BTreeMap<String, String>,
    /// `[image.<id>.knobs.<group>] <name> = <value>` — the same APP rung,
    /// spelled the way a board's `[knobs.<group>]` is (RFC-0103 D6, phase-484
    /// W8), keyed by the dotted `<group>.<name>`. `[image_defaults.knobs]`
    /// underneath, key by key, like [`Self::env`].
    ///
    /// Read as written: which knob a key names, and whether its value fits,
    /// is the registry's answer (`nros-zephyr-build`), which the CLI asks
    /// before rendering each one into the carrier [`Self::env`] rides.
    pub knobs: BTreeMap<String, KnobValue>,
    /// Issue 1142 — `[system] features`, the SAME key a workspace bringup's
    /// `system.toml` carries (`features = ["param_services"]`, which reaches a
    /// resolved model as `execution.features`).
    ///
    /// It answers the half of the queryable budget no entity declaration can:
    /// the runtime's own parameter and lifecycle service families are service
    /// servers nobody writes a `create_service` for. On the workspace road the
    /// resolved model states them; a standalone leaf has no model, so it states
    /// them here, in the file it already states everything else in.
    ///
    /// ABSENT MEANS NONE, exactly as it does for a bringup —
    /// `cmd::entity_facts::declared_infra` reads an empty `execution.features`
    /// as `"none"` for every model in this tree. The claim is worth exactly as
    /// much as the `entities` declaration beside it, which is why it is read
    /// only where that declaration exists.
    pub features: Vec<String>,
    /// `[image.<id>] conf` — the framework config fragments this image needs,
    /// in merge order: `[image_defaults] conf` first, then the image's own,
    /// CONCATENATED (the overlay `ImageBlock::with_base` gives a list).
    ///
    /// phase-481 W3. On Zephyr these are the Zephyr-native fragments an RMW
    /// needs (`prj-xrce.conf`'s `CONFIG_NET_TCP`, heap and POSIX sizing): the
    /// nano-ros half of the image is rendered from this file, the Zephyr half
    /// stays a Kconfig fragment, and the image names which one. Names are
    /// relative to the directory holding `system.toml`; this reader keeps them
    /// as written and leaves resolution to the caller, which knows the road.
    pub conf: Vec<String>,
}

impl LeafSystem {
    /// The file the answer came from.
    pub fn origin_path(&self) -> &Path {
        match &self.origin {
            Origin::SystemToml(p) | Origin::Bringup(p) => p,
        }
    }

    /// Every component's declared entities, concatenated. `None` when no
    /// component declares any (the probe is the only source then).
    pub fn declared_entities(&self) -> Option<Vec<String>> {
        let mut any = false;
        let mut out = Vec::new();
        for c in &self.components {
            if let Some(e) = &c.entities {
                any = true;
                out.extend(e.iter().cloned());
            }
        }
        any.then_some(out)
    }

    /// Issue 1556 -- does any component take its entities from the leaf's
    /// census (`entities = "census"`)?
    pub fn entities_from_census(&self) -> bool {
        self.components.iter().any(|c| c.entities_from_census)
    }
}

/// Is `dir` a PACKAGE (as opposed to a workspace bringup, which carries a
/// `system.toml` and no build file)?
pub fn is_package_dir(dir: &Path) -> bool {
    dir.join("Cargo.toml").is_file() || dir.join("CMakeLists.txt").is_file()
}

/// The deployment `dir` declares, or `None` when it declares none.
///
/// * `<dir>/system.toml` beside a package manifest → read from it.
/// * otherwise `None` — a workspace entry's deployment is its bringup's image
///   ([`for_entry`]).
///
/// Either way a retired deployment key left in `Cargo.toml` is an ERROR naming
/// where the fact lives now: with the fallback gone nothing reads it, and a key
/// nothing reads is a board choice that silently stopped being made.
///
/// A `system.toml` in a directory with no `Cargo.toml`/`CMakeLists.txt` is a
/// workspace BRINGUP, not a leaf, and yields `None` here: its images are
/// chosen by the workspace builder, not by this reader.
pub fn read(dir: &Path) -> Result<Option<LeafSystem>, String> {
    let system = dir.join(SYSTEM_TOML);
    refuse_retired_manifest_keys(dir)?;
    if system.is_file() && is_package_dir(dir) {
        return from_system_toml(&system).map(Some);
    }
    Ok(None)
}

/// phase-481 W1 (RFC-0098 D11) — the deployment `dir`'s `system.toml` states
/// for the NAMED image, whether `dir` is a single-package leaf or a workspace
/// bringup.
///
/// The Zephyr module hook selects an image with `-DNROS_IMAGE=<id>` when a leaf
/// declares several (one per RMW, say), and a generated workspace application
/// names its bringup's image the same way. [`read`]'s own rule (exactly one
/// image, or `[system] default_images`) is what applies when nothing is named.
/// `Ok(None)` when `dir` holds no `system.toml`; naming an image the file does
/// not declare is an error listing the ones it does.
pub fn read_image(dir: &Path, image: &str) -> Result<Option<LeafSystem>, String> {
    let path = dir.join(SYSTEM_TOML);
    if !path.is_file() {
        return Ok(None);
    }
    let package = is_package_dir(dir);
    if package {
        refuse_retired_manifest_keys(dir)?;
    }
    let doc = parse_file(&path)?;
    let images = as_table(doc.get("image")).cloned().unwrap_or_default();
    if !images.contains_key(image) {
        return Err(format!(
            "{}: names no `[image.{image}]` (declared: {})",
            path.display(),
            if images.is_empty() {
                "none".to_string()
            } else {
                images
                    .keys()
                    .map(|k| format!("`{k}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        ));
    }
    let origin = if package {
        Origin::SystemToml(path.clone())
    } else {
        Origin::Bringup(path.clone())
    };
    image_system(&doc, &path, image, origin).map(Some)
}

/// The deployment of the ENTRY package at `entry_dir` (package name
/// `entry_pkg`), as stated by the bringup at `bringup_dir`: the `[image.<id>]`
/// that claims it.
///
/// An image claims an entry by naming it — `entry = "<pkg>"` — or, when it
/// names none, by being the image the builder would GENERATE that entry for
/// ([`entry_package_name`]`(id) == entry_pkg`). The directory name is accepted
/// beside the package name for both, because `entry =` has always accepted
/// either (`west_application_dir`).
///
/// `Ok(None)` when no image claims it. SEVERAL claiming it is an error, not a
/// first match: which image's locator an entry bakes is not a coin toss.
///
/// A leaf `system.toml` beside the entry answers first ([`read`]) — an entry
/// that states its own deployment is a single-package leaf, whatever encloses
/// it.
pub fn for_entry(
    entry_dir: &Path,
    entry_pkg: &str,
    bringup_dir: &Path,
) -> Result<Option<LeafSystem>, String> {
    if let Some(own) = read(entry_dir)? {
        return Ok(Some(own));
    }
    let path = bringup_dir.join(SYSTEM_TOML);
    if !path.is_file() {
        return Ok(None);
    }
    let doc = parse_file(&path)?;
    let dir_name = entry_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let names = |s: &str| s == entry_pkg || (!dir_name.is_empty() && s == dir_name);
    let images = as_table(doc.get("image")).cloned().unwrap_or_default();
    let mut explicit = Vec::new();
    let mut by_name = Vec::new();
    for (id, block) in &images {
        match str_key(block.as_table(), "entry") {
            Some(e) if names(&e) => explicit.push(id.clone()),
            // An image that names a DIFFERENT entry is that entry's, even when
            // its id happens to derive this one's name.
            Some(_) => {}
            None if names(&entry_package_name(id)) => by_name.push(id.clone()),
            None => {}
        }
    }
    let claims = if explicit.is_empty() {
        by_name
    } else {
        explicit
    };
    match claims.as_slice() {
        [] => Ok(None),
        [one] => image_system(&doc, &path, one, Origin::Bringup(path.clone())).map(Some),
        many => Err(format!(
            "{}: {} images claim the entry `{entry_pkg}` ({}) — an entry is ONE image's \
             program; name it from exactly one with `entry = \"{entry_pkg}\"`",
            path.display(),
            many.len(),
            many.join(", ")
        )),
    }
}

/// Issue 1289 — the image that claims the package at `entry_dir` (package name
/// `entry_pkg`), searched over a WORKSPACE's bringups: what makes a package an
/// ENTRY.
///
/// This is the one predicate for "is this package an entry". It used to be the
/// presence of an (empty) `[package.metadata.nros.entry]` table in the
/// manifest, which stated nothing an image did not already state — every
/// in-tree entry the table marked was also claimed by an image, and a marker
/// nobody keeps in step with the bringup is a second spelling of one fact.
///
/// * A `system.toml` beside the package answers first ([`read`]): a
///   single-package leaf is its own image.
/// * Otherwise each of `bringup_dirs` is asked through [`for_entry`]. A
///   directory that is itself a package (a leaf's `system.toml`) is not a
///   bringup and is skipped, so a caller may pass every directory holding a
///   `system.toml`.
///
/// `Ok(None)` when nothing claims it — a node or library package. Images in two
/// bringups claiming one entry is an error, not a first match.
pub fn claiming_image<'a>(
    entry_dir: &Path,
    entry_pkg: &str,
    bringup_dirs: impl IntoIterator<Item = &'a Path>,
) -> Result<Option<LeafSystem>, String> {
    if let Some(own) = read(entry_dir)? {
        return Ok(Some(own));
    }
    let mut hit: Option<(PathBuf, LeafSystem)> = None;
    for dir in bringup_dirs {
        if !dir.join(SYSTEM_TOML).is_file() || is_package_dir(dir) {
            continue; // not a bringup
        }
        let Some(l) = for_entry(entry_dir, entry_pkg, dir)? else {
            continue;
        };
        if let Some((prev, _)) = &hit {
            return Err(format!(
                "entry `{entry_pkg}` is claimed by images in two bringups ({} and {}) — an \
                 entry is ONE image's program",
                prev.display(),
                dir.display()
            ));
        }
        hit = Some((dir.to_path_buf(), l));
    }
    Ok(hit.map(|(_, l)| l))
}

/// The board `dir` deploys to, from [`read`].
pub fn board(dir: &Path) -> Result<Option<String>, String> {
    Ok(read(dir)?.and_then(|l| l.board))
}

fn as_table(v: Option<&toml::Value>) -> Option<&toml::Table> {
    v.and_then(|v| v.as_table())
}

fn str_key(t: Option<&toml::Table>, key: &str) -> Option<String> {
    t?.get(key)?
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn u32_key(t: Option<&toml::Table>, key: &str, file: &Path) -> Result<Option<u32>, String> {
    let Some(v) = t.and_then(|t| t.get(key)) else {
        return Ok(None);
    };
    let i = v
        .as_integer()
        .ok_or_else(|| format!("{}: `{key}` must be an integer, got {v}", file.display()))?;
    u32::try_from(i)
        .map(Some)
        .map_err(|_| format!("{}: `{key}` = {i} is out of range", file.display()))
}

/// Issue 1142 — `[system] features`, an array of capability names.
///
/// Absent is an empty list and NOT an error: a bringup that names no feature
/// carries none, and one schema means one reading.
fn features_key(t: Option<&toml::Table>, file: &Path) -> Result<Vec<String>, String> {
    let Some(v) = t.and_then(|t| t.get("features")) else {
        return Ok(Vec::new());
    };
    let arr = v.as_array().ok_or_else(|| {
        format!(
            "{}: `[system] features` must be an ARRAY of strings, e.g. \
             [\"param_services\", \"lifecycle\"]",
            file.display()
        )
    })?;
    arr.iter()
        .map(|item| {
            item.as_str().map(str::to_string).ok_or_else(|| {
                format!(
                    "{}: every `[system] features` element must be a string; found {item}",
                    file.display()
                )
            })
        })
        .collect()
}

/// The word `entities` takes instead of a list (issue 1556).
pub const ENTITIES_FROM_CENSUS: &str = "census";

/// `entities` as `(declared list, from census)`: an array is a declaration,
/// the string [`ENTITIES_FROM_CENSUS`] is the census opt-in, anything else is
/// refused naming both shapes.
fn entities_key(t: &toml::Table, file: &Path) -> Result<(Option<Vec<String>>, bool), String> {
    let Some(v) = t.get("entities") else {
        return Ok((None, false));
    };
    if v.as_str() == Some(ENTITIES_FROM_CENSUS) {
        return Ok((None, true));
    }
    let arr = v.as_array().ok_or_else(|| {
        format!(
            "{}: `entities` must be an ARRAY of strings, e.g. \
             [\"publisher:std_msgs/msg/String:/chatter\", \"timer\"], or the word \
             \"{ENTITIES_FROM_CENSUS}\" (read from the leaf's census, issue 1556)",
            file.display()
        )
    })?;
    arr.iter()
        .map(|item| {
            item.as_str().map(str::to_string).ok_or_else(|| {
                format!(
                    "{}: every `entities` element must be a string; found {item}",
                    file.display()
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|v| (Some(v), false))
}

fn parse_file(path: &Path) -> Result<toml::Table, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    raw.parse::<toml::Table>()
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Read a leaf `system.toml`. A leaf builds ONE image: exactly one
/// `[image.<id>]`, or `[system] default_images = ["<id>"]` naming one of
/// several.
fn from_system_toml(path: &Path) -> Result<LeafSystem, String> {
    let doc = parse_file(path)?;
    crate::retired_tables::refuse_retired_tables(&doc, path)?;
    let system = as_table(doc.get("system"));
    let images = as_table(doc.get("image")).cloned().unwrap_or_default();

    let ids: Vec<&String> = images.keys().collect();
    let default_images: Vec<String> = system
        .and_then(|s| s.get("default_images"))
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let id: String = match (ids.as_slice(), default_images.as_slice()) {
        ([], _) => {
            return Err(format!(
                "{}: a single-package leaf must declare the image it builds — add \
                 `[image.<id>]` with `board = \"<board>\"` (RFC-0098 D3)",
                path.display()
            ));
        }
        ([one], _) => (*one).clone(),
        (_, [one]) => {
            if !images.contains_key(one) {
                return Err(format!(
                    "{}: `[system] default_images = [\"{one}\"]` names no `[image.{one}]`",
                    path.display()
                ));
            }
            one.clone()
        }
        (many, _) => {
            return Err(format!(
                "{}: a single-package leaf builds ONE image, and {} are declared ({}) — \
                 set `[system] default_images = [\"<id>\"]` to choose",
                path.display(),
                many.len(),
                many.iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    };
    image_system(&doc, path, &id, Origin::SystemToml(path.to_path_buf()))
}

/// `[image.<id>]` of the `system.toml` document `doc` (read from `path`),
/// resolved: the image over `[image_defaults]` over `[system]`. Shared by a
/// leaf's own file ([`from_system_toml`]) and a bringup's image for one of its
/// entries ([`for_entry`]) — one overlay, whichever file states it.
fn image_system(
    doc: &toml::Table,
    path: &Path,
    id: &str,
    origin: Origin,
) -> Result<LeafSystem, String> {
    let system = as_table(doc.get("system"));
    let defaults = as_table(doc.get("image_defaults"));
    let image = as_table(doc.get("image")).and_then(|t| as_table(t.get(id)));
    // `[image.<id>]` over `[image_defaults]` — the one overlay RFC-0065 D5.1
    // defines (`ImageBlock::with_base`), applied key by key.
    let pick = |key: &str| str_key(image, key).or_else(|| str_key(defaults, key));

    let board = pick("board").ok_or_else(|| {
        format!(
            "{}: `[image.{id}]` names no `board` — every image is built for exactly one \
             board (RFC-0098 D3)",
            path.display()
        )
    })?;
    let rmw = pick("rmw").or_else(|| str_key(system, "rmw"));
    let domain_id = match u32_key(image, "domain_id", path)? {
        Some(d) => Some(d),
        None => match u32_key(defaults, "domain_id", path)? {
            Some(d) => Some(d),
            None => u32_key(system, "domain_id", path)?,
        },
    };
    let transport = pick("transport");
    if let Some(t) = &transport
        && !TRANSPORT_KINDS.contains(&t.as_str())
    {
        return Err(format!(
            "{}: `[image.{id}] transport = \"{t}\"` is not a link kind — it is one of {} \
             (RFC-0086 D2). The RMW is `rmw = \"…\"`, which is a different choice.",
            path.display(),
            TRANSPORT_KINDS.join(", ")
        ));
    }
    // RFC-0103 D10 (phase-497 W1) — the address is `[image.<id>.net.<if>]` on a
    // `[network.<name>]`, lowered here to the triple every carrier still takes.
    // The flat `ip` / `gateway` / `netmask` it replaces are refused by name.
    crate::system_net::refuse_flat_image_net(image, &format!("image.{id}"))
        .and_then(|()| crate::system_net::refuse_flat_image_net(defaults, "image_defaults"))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let networks =
        crate::system_net::parse_networks(doc).map_err(|e| format!("{}: {e}", path.display()))?;
    let net = {
        let base = crate::system_net::parse_image_net(defaults, "image_defaults")
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let at = format!("image.{id}");
        let inline = crate::system_net::parse_image_net(image, &at)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        // RFC-0109 D4 (phase-514 W2) — and the board network files the image
        // names in `os_config`, beside this `system.toml`.
        let os_config = crate::system_net::parse_os_config(image, &at)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        let own = crate::system_net::fold_board_net_files(&inline, dir, &os_config, &at)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        crate::system_net::overlay(&base, &own)
    };
    crate::system_net::check_image_interfaces(id, &networks, &net)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    // D10d — the image's own `locator`, else the `router` of the network it
    // sits on, else the project's `[system] locator`. A network more specific
    // than the project outranks the project's default.
    let locator = pick("locator")
        .or_else(|| crate::system_net::network_router(&networks, &net))
        .or_else(|| str_key(system, "locator"));
    let network = Network {
        domain_id,
        locator,
        net,
        networks,
        system: str_key(system, "name"),
        transport,
    };

    let mut components = Vec::new();
    if let Some(rows) = doc.get("component") {
        let rows = rows.as_array().ok_or_else(|| {
            format!(
                "{}: `component` must be an array of tables (`[[component]]`)",
                path.display()
            )
        })?;
        for row in rows {
            let t = row.as_table().ok_or_else(|| {
                format!("{}: every `[[component]]` must be a table", path.display())
            })?;
            let (entities, entities_from_census) = entities_key(t, path)?;
            // phase-481 W3 -- eight Zephyr leaves shipped `name =
            // "${NROS_CYCLONE_IDLC}"`, a build-system variable copied into a
            // file nothing substitutes (a phase-445 W3b slip). TOML has no
            // interpolation, so a `${` in an identifier is never what the
            // author meant; refuse it naming the row rather than registering
            // a node by that literal.
            for key in ["pkg", "class", "name"] {
                if let Some(v) = str_key(Some(t), key).filter(|v| v.contains("${")) {
                    return Err(format!(
                        "{}: `[[component]] {key} = \"{v}\"` holds an unsubstituted `${{…}}` -- \
                         system.toml is not expanded by any build system; write the literal {key}",
                        path.display()
                    ));
                }
            }
            components.push(LeafComponent {
                pkg: str_key(Some(t), "pkg"),
                class: str_key(Some(t), "class"),
                name: str_key(Some(t), "name"),
                entities,
                entities_from_census,
                dispatch: str_key(Some(t), "dispatch"),
            });
        }
    }

    // `[image.<id>] env` over `[image_defaults] env`, key by key — the same
    // overlay `ImageBlock::with_base` gives a map: the base is a default SET,
    // and the image overwrites only what it also names.
    let mut env = env_table(defaults, path, id)?;
    env.extend(env_table(image, path, id)?);
    let mut knobs = knobs_table(defaults, path, "image_defaults.knobs")?;
    knobs.extend(knobs_table(image, path, &format!("image.{id}.knobs"))?);
    let mut conf = conf_list(defaults, path, "image_defaults")?;
    conf.extend(conf_list(image, path, &format!("image.{id}"))?);

    Ok(LeafSystem {
        origin,
        image: Some(id.to_string()),
        board: Some(board),
        rmw,
        network,
        components,
        env,
        knobs,
        features: features_key(system, path)?,
        conf,
    })
}

/// `conf = ["<fragment>", …]` of one table — an array of strings, or absent.
fn conf_list(block: Option<&toml::Table>, path: &Path, table: &str) -> Result<Vec<String>, String> {
    let Some(v) = block.and_then(|b| b.get("conf")) else {
        return Ok(Vec::new());
    };
    let arr = v.as_array().ok_or_else(|| {
        format!(
            "{}: `[{table}] conf` must be an ARRAY of fragment names, e.g. [\"prj-xrce.conf\"]",
            path.display()
        )
    })?;
    arr.iter()
        .map(|item| {
            item.as_str().map(str::to_string).ok_or_else(|| {
                format!(
                    "{}: every `[{table}] conf` element must be a string; found {item}",
                    path.display()
                )
            })
        })
        .collect()
}

/// `[<table>.<group>] <name> = <value>` of one image block, flattened.
fn knobs_table(
    block: Option<&toml::Table>,
    path: &Path,
    table: &str,
) -> Result<BTreeMap<String, KnobValue>, String> {
    let Some(v) = block.and_then(|b| b.get("knobs")) else {
        return Ok(BTreeMap::new());
    };
    let t = v.as_table().ok_or_else(|| {
        format!(
            "{}: `[{table}]` must be a TABLE of `[{table}.<group>] <name> = <value>` rows",
            path.display()
        )
    })?;
    flatten_knobs(t, &format!("[{table}]")).map_err(|e| format!("{}: {e}", path.display()))
}

/// `env = { KEY = "VALUE" }` of one image block.
///
/// Values are strings, always: an environment variable IS a string, and
/// accepting an integer here would make `X = 2` and `X = "2"` two spellings of
/// one row that a later `deny_unknown_fields` schema would have to keep
/// agreeing about. The error says which spelling to write.
fn env_table(
    block: Option<&toml::Table>,
    path: &Path,
    id: &str,
) -> Result<BTreeMap<String, String>, String> {
    let Some(t) = block.and_then(|b| b.get("env")) else {
        return Ok(BTreeMap::new());
    };
    let t = t.as_table().ok_or_else(|| {
        format!(
            "{}: `[image.{id}] env` must be a TABLE of `KEY = \"VALUE\"` rows",
            path.display()
        )
    })?;
    t.iter()
        .map(|(k, v)| {
            v.as_str()
                .map(|s| (k.clone(), s.to_string()))
                .ok_or_else(|| {
                    format!(
                        "{}: `[image.{id}] env` row `{k}` is a {}, not a string — an environment \
                     value is a string, so write `{k} = \"{v}\"`",
                        path.display(),
                        v.type_str()
                    )
                })
        })
        .collect()
}

/// The retired DEPLOYMENT keys a manifest table still carries, by name.
///
/// `[package.metadata.nros.node]` / `component` are not among them: on a
/// single-package leaf they retired with W3b (and `check-leaf-deployment-
/// spelling` refuses them there), but on a workspace node package they are
/// node REGISTRATION (issue 1289), not a deployment, and the deployment reader
/// has no business refusing them.
#[must_use]
pub fn retired_deployment_keys(nros: &toml::Table) -> Vec<String> {
    let mut out = Vec::new();
    if as_table(nros.get("entry")).is_some_and(|e| e.contains_key("deploy")) {
        out.push("[package.metadata.nros.entry] deploy".to_string());
    }
    if let Some(d) = as_table(nros.get("deploy")) {
        for k in d.keys() {
            out.push(format!("[package.metadata.nros.deploy.{k}]"));
        }
    }
    out
}

/// Refuse a manifest still carrying a retired deployment key, naming each one
/// and where the fact lives now. Nothing reads those keys any more
/// (phase-445 W5 deleted the fallback), so leaving one is worse than an error:
/// it READS like the board choice and is not.
fn refuse_retired_manifest_keys(dir: &Path) -> Result<(), String> {
    let manifest = dir.join("Cargo.toml");
    if !manifest.is_file() {
        return Ok(());
    }
    let doc = parse_file(&manifest)?;
    let Some(nros) = doc
        .get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("nros"))
        .and_then(|n| n.as_table())
    else {
        return Ok(());
    };
    let left = retired_deployment_keys(nros);
    if left.is_empty() {
        return Ok(());
    }
    Err(format!(
        "{} carries {} — retired (RFC-0098 D3/D5, phase-445 W5): nothing reads it. State the \
         deployment as `[image.<id>] board = \"<board>\"` (+ `rmw`, `domain_id`, `locator`, …) \
         in {} for a single-package example, or in the workspace bringup's `system.toml` image \
         that builds this entry (`entry = \"<pkg>\"`), and delete the table.",
        manifest.display(),
        left.join(", "),
        dir.join(SYSTEM_TOML).display(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue 1305 — the manifest key, the package name and the path the macro
    /// emits are one spelling: the crate ident form.
    #[test]
    fn the_selection_crate_name_is_the_ident_form_of_the_package() {
        assert_eq!(
            selection_crate_name("qemu-bsp-talker"),
            "qemu_bsp_talker_nros_selection"
        );
        assert_eq!(
            selection_crate_name("linux_entry"),
            "linux_entry_nros_selection"
        );
    }

    fn leaf(files: &[(&str, &str)]) -> tempdir::Dir {
        let d = tempdir::Dir::new();
        for (name, body) in files {
            std::fs::write(d.path().join(name), body).unwrap();
        }
        d
    }

    /// Minimal self-cleaning temp dir; this crate carries no dev-deps.
    mod tempdir {
        use std::path::{Path, PathBuf};
        pub struct Dir(PathBuf);
        impl Dir {
            pub fn new() -> Self {
                use std::sync::atomic::{AtomicUsize, Ordering};
                static N: AtomicUsize = AtomicUsize::new(0);
                let p = std::env::temp_dir().join(format!(
                    "nros-leaf-system-{}-{}",
                    std::process::id(),
                    N.fetch_add(1, Ordering::Relaxed)
                ));
                let _ = std::fs::remove_dir_all(&p);
                std::fs::create_dir_all(&p).unwrap();
                Dir(p)
            }
            pub fn path(&self) -> &Path {
                &self.0
            }
        }
        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    const CARGO: &str = "[package]\nname = \"talker\"\nversion = \"0.1.0\"\n";

    const SYSTEM: &str = r#"
[system]
name = "talker"
rmw = "zenoh"
domain_id = 3
locator = "tcp/10.0.2.2:7447"

[[component]]
pkg = "esp32_talker"
class = "esp32_talker::Talker"
name = "talker"
entities = ["publisher:std_msgs/msg/String:/chatter", "timer"]
dispatch = "deferred"

[image.esp32]
board = "esp32-c3-baremetal"
locator = "tcp/10.0.2.2:9800"
net = { eth0 = { network = "qemu", host = 50 } }
"#;

    /// phase-481 W1 — an image named explicitly is resolved from a leaf with
    /// several (where [`read`] refuses to choose) and from a bringup (where
    /// [`read`] answers nothing); an unknown id is refused naming the declared
    /// ones.
    #[test]
    fn a_named_image_is_read_from_a_leaf_or_a_bringup() {
        let two = "[system]\nname = \"t\"\nrmw = \"zenoh\"\n\n\
                   [image.zephyr_zenoh]\nboard = \"zephyr\"\n\n\
                   [image.zephyr_xrce]\nboard = \"zephyr\"\nrmw = \"xrce\"\n";
        let d = leaf(&[("Cargo.toml", CARGO), ("system.toml", two)]);
        assert!(read(d.path()).is_err(), "two images, nothing chosen");
        let x = read_image(d.path(), "zephyr_xrce")
            .unwrap()
            .expect("declared");
        assert!(matches!(x.origin, Origin::SystemToml(_)));
        assert_eq!(x.image.as_deref(), Some("zephyr_xrce"));
        assert_eq!(x.rmw.as_deref(), Some("xrce"));
        let z = read_image(d.path(), "zephyr_zenoh").unwrap().unwrap();
        assert_eq!(z.rmw.as_deref(), Some("zenoh"), "falls through to [system]");
        let e = read_image(d.path(), "nope").unwrap_err();
        assert!(
            e.contains("zephyr_xrce") && e.contains("zephyr_zenoh"),
            "{e}"
        );

        let bringup = leaf(&[("system.toml", two)]);
        assert_eq!(read(bringup.path()).unwrap(), None, "a bringup is no leaf");
        let b = read_image(bringup.path(), "zephyr_zenoh").unwrap().unwrap();
        assert!(matches!(b.origin, Origin::Bringup(_)));

        let empty = leaf(&[("Cargo.toml", CARGO)]);
        assert_eq!(read_image(empty.path(), "x").unwrap(), None);
    }

    /// phase-481 W3 — `conf` names the image's framework fragments:
    /// `[image_defaults] conf` first, then the image's own (concatenated, the
    /// `ImageBlock::with_base` overlay for a list); absent is empty; a non-array
    /// or a non-string element is refused naming the table.
    #[test]
    fn conf_concatenates_defaults_then_the_image() {
        let t = "[system]\nname = \"t\"\nrmw = \"zenoh\"\ndefault_images = [\"a\"]\n\n\
                 [image_defaults]\nconf = [\"base.conf\"]\n\n\
                 [image.a]\nboard = \"zephyr\"\nconf = [\"prj-zenoh.conf\"]\n\n\
                 [image.b]\nboard = \"zephyr\"\n";
        let d = leaf(&[("Cargo.toml", CARGO), ("system.toml", t)]);
        assert_eq!(
            read(d.path()).unwrap().unwrap().conf,
            vec!["base.conf", "prj-zenoh.conf"]
        );
        assert_eq!(
            read_image(d.path(), "b").unwrap().unwrap().conf,
            vec!["base.conf"]
        );
        let none = leaf(&[("Cargo.toml", CARGO), ("system.toml", SYSTEM)]);
        assert!(read(none.path()).unwrap().unwrap().conf.is_empty());
        let bad = leaf(&[
            ("Cargo.toml", CARGO),
            (
                "system.toml",
                "[image.a]\nboard = \"zephyr\"\nconf = \"prj.conf\"\n",
            ),
        ]);
        let e = read(bad.path()).unwrap_err();
        assert!(e.contains("[image.a] conf") && e.contains("ARRAY"), "{e}");
    }

    /// phase-481 W3 -- a `${VAR}` in a component identifier is a build-system
    /// variable nothing expands; refused naming the key.
    #[test]
    fn an_unsubstituted_variable_in_a_component_is_refused() {
        let t = "[system]\nname = \"t\"\n\n[[component]]\npkg = \"p\"\n\
                 class = \"p::C\"\nname = \"${NROS_CYCLONE_IDLC}\"\n\n\
                 [image.a]\nboard = \"zephyr\"\n";
        let d = leaf(&[("Cargo.toml", CARGO), ("system.toml", t)]);
        let e = read(d.path()).unwrap_err();
        assert!(
            e.contains("[[component]] name") && e.contains("${NROS_CYCLONE_IDLC}"),
            "{e}"
        );
    }

    #[test]
    fn a_single_package_leaf_resolves_from_its_system_toml() {
        let d = leaf(&[("Cargo.toml", CARGO), ("system.toml", SYSTEM)]);
        let l = read(d.path()).unwrap().expect("declared");
        assert!(matches!(l.origin, Origin::SystemToml(_)));
        assert_eq!(l.image.as_deref(), Some("esp32"));
        assert_eq!(l.board.as_deref(), Some("esp32-c3-baremetal"));
        assert_eq!(l.rmw.as_deref(), Some("zenoh"));
        // The image's locator beats the system default; the domain falls
        // through to `[system]`.
        assert_eq!(l.network.locator.as_deref(), Some("tcp/10.0.2.2:9800"));
        assert_eq!(l.network.domain_id, Some(3));
        // RFC-0103 D10 — `host = 50` on the built-in `qemu` network, carried
        // as stated: the CLI's resolver turns it into an address.
        let eth0 = &l.network.net["eth0"];
        assert_eq!(eth0.network.as_deref(), Some("qemu"));
        assert_eq!(eth0.host, Some(50));
        assert_eq!(l.components.len(), 1);
        assert_eq!(l.components[0].pkg.as_deref(), Some("esp32_talker"));
        // Issue 1278 — the dispatch strategy has a home here too.
        assert_eq!(l.components[0].dispatch.as_deref(), Some("deferred"));
        assert_eq!(
            l.declared_entities().unwrap(),
            vec!["publisher:std_msgs/msg/String:/chatter", "timer"]
        );
        // Issue 1142 — a leaf that names no feature carries none, exactly as a
        // bringup with an empty `execution.features` does.
        assert!(l.features.is_empty());
    }

    /// Issue 1142 — `[system] features` is the leaf's channel for the service
    /// families the runtime creates on its behalf. Same key, same spelling and
    /// same meaning as a workspace bringup's.
    #[test]
    fn the_system_features_list_is_read_and_a_non_array_names_the_file() {
        let d = leaf(&[
            ("Cargo.toml", CARGO),
            (
                "system.toml",
                "[system]\nname = \"t\"\nfeatures = [\"param_services\", \"lifecycle\"]\n\
                 \n[image.i]\nboard = \"linux\"\n",
            ),
        ]);
        let l = read(d.path()).unwrap().expect("declared");
        assert_eq!(l.features, vec!["param_services", "lifecycle"]);

        let d = leaf(&[
            ("Cargo.toml", CARGO),
            (
                "system.toml",
                "[system]\nname = \"t\"\nfeatures = \"param_services\"\n\
                 \n[image.i]\nboard = \"linux\"\n",
            ),
        ]);
        let e = read(d.path()).unwrap_err();
        assert!(e.contains("ARRAY") && e.contains("system.toml"), "{e}");
    }

    #[test]
    fn a_cmake_leaf_is_a_package_too() {
        let d = leaf(&[("CMakeLists.txt", "project(x)\n"), ("system.toml", SYSTEM)]);
        assert!(read(d.path()).unwrap().is_some());
    }

    #[test]
    fn a_bringup_system_toml_is_not_a_leaf() {
        // No build file beside it: a workspace bringup, whose images the
        // builder chooses. Not this reader's question.
        let d = leaf(&[("system.toml", SYSTEM)]);
        assert_eq!(read(d.path()).unwrap(), None);
    }

    #[test]
    fn a_retired_deployment_key_is_refused_naming_where_the_fact_lives() {
        // The fallback that read these is gone (phase-445 W5). A manifest still
        // carrying one must not resolve to "no deployment" and then fail far
        // away as "declares no board" — it is refused where it is.
        for (tables, key) in [
            (
                "[package.metadata.nros.entry]\ndeploy = \"freertos\"\n",
                "[package.metadata.nros.entry] deploy",
            ),
            (
                "[package.metadata.nros.deploy.zephyr]\nrmw = \"zenoh\"\n",
                "[package.metadata.nros.deploy.zephyr]",
            ),
        ] {
            let d = leaf(&[("Cargo.toml", &format!("{CARGO}\n{tables}"))]);
            let e = read(d.path()).unwrap_err();
            assert!(e.contains(key), "names the key: {e}");
            assert!(e.contains("system.toml"), "names where to write: {e}");
            assert!(!e.contains('\n'), "one line: {e}");
        }
    }

    #[test]
    fn node_and_component_tables_are_not_a_deployment() {
        // A workspace NODE package declares its class for the metadata
        // pipeline; that is not this reader's refusal to make.
        let d = leaf(&[(
            "Cargo.toml",
            &format!(
                "{CARGO}\n[package.metadata.nros.node]\nclass = \"t::Talker\"\n\
                 [package.metadata.nros.component]\nentities = [\"timer\"]\n"
            ),
        )]);
        assert_eq!(read(d.path()).unwrap(), None);
    }

    #[test]
    fn a_manifest_with_no_nros_metadata_declares_nothing() {
        let d = leaf(&[("Cargo.toml", CARGO)]);
        assert_eq!(read(d.path()).unwrap(), None);
    }

    const BRINGUP: &str = r#"
[system]
name = "demo"
rmw = "zenoh"
domain_id = 0

[image_defaults]
rmw = "zenoh"

[image.esp32]
board = "esp32-c3-baremetal"
locator = "tcp/10.0.2.2:9830"

[image.zephyr]
board = "zephyr"
entry = "zephyr_entry"
locator = "tcp/10.0.2.2:7430"

[image.zephyr_robot1]
board = "zephyr"
entry = "zephyr_entry_robot1"
domain_id = 4
"#;

    /// `<ws>/src/{demo_bringup,<entry>}` with the bringup's `system.toml`.
    fn workspace(entry: &str, entry_manifest: &str) -> (tempdir::Dir, PathBuf, PathBuf) {
        let d = tempdir::Dir::new();
        let bringup = d.path().join("src/demo_bringup");
        let e = d.path().join("src").join(entry);
        std::fs::create_dir_all(&bringup).unwrap();
        std::fs::create_dir_all(&e).unwrap();
        std::fs::write(bringup.join("system.toml"), BRINGUP).unwrap();
        std::fs::write(e.join("Cargo.toml"), entry_manifest).unwrap();
        (d, bringup, e)
    }

    #[test]
    fn a_generated_entry_finds_the_image_it_was_generated_for() {
        // `nros build` names the entry `<id>_entry`; that name is the claim.
        let (_d, bringup, e) = workspace("esp32_entry", CARGO);
        let l = for_entry(&e, "esp32_entry", &bringup)
            .unwrap()
            .expect("claimed");
        assert_eq!(l.image.as_deref(), Some("esp32"));
        assert_eq!(l.board.as_deref(), Some("esp32-c3-baremetal"));
        assert_eq!(l.network.locator.as_deref(), Some("tcp/10.0.2.2:9830"));
        // `[system]` fills what the image leaves out — the leaf overlay.
        assert_eq!(l.network.domain_id, Some(0));
        assert!(matches!(l.origin, Origin::Bringup(_)));
    }

    /// Issue 1289 — an entry is the package an image CLAIMS; a manifest table
    /// says nothing either way.
    #[test]
    fn an_entry_is_what_an_image_claims_not_what_a_manifest_marks() {
        // Claimed by NAME (a generated `<id>_entry`), with no marker at all.
        let (d, bringup, e) = workspace("esp32_entry", CARGO);
        let l = claiming_image(&e, "esp32_entry", [bringup.as_path()])
            .unwrap()
            .expect("an image claims it, so it is an entry");
        assert_eq!(l.image.as_deref(), Some("esp32"));
        // Claimed by `entry = "<pkg>"`.
        let z = d.path().join("src/zephyr_entry");
        std::fs::create_dir_all(&z).unwrap();
        std::fs::write(z.join("Cargo.toml"), CARGO).unwrap();
        let l = claiming_image(&z, "zephyr_entry", [bringup.as_path()])
            .unwrap()
            .expect("claimed by entry =");
        assert_eq!(l.image.as_deref(), Some("zephyr"));
        // A package carrying the old empty marker that NO image claims is not
        // an entry: the marker was never the fact.
        let (_d2, bringup2, marked) = workspace(
            "talker_pkg",
            &format!("{CARGO}\n[package.metadata.nros.entry]\n"),
        );
        assert_eq!(
            claiming_image(&marked, "talker_pkg", [bringup2.as_path()]).unwrap(),
            None
        );
        // A single-package leaf is its own image, whatever bringups are given.
        let leafd = leaf(&[("Cargo.toml", CARGO), ("system.toml", SYSTEM)]);
        assert!(
            claiming_image(leafd.path(), "talker", std::iter::empty::<&Path>())
                .unwrap()
                .is_some()
        );
        // A `system.toml` beside a PACKAGE passed as a bringup is a leaf, not
        // a bringup: it claims nothing for anyone else.
        assert_eq!(
            claiming_image(&marked, "talker_pkg", [leafd.path()]).unwrap(),
            None
        );
    }

    #[test]
    fn an_explicit_entry_key_claims_and_beats_the_derived_name() {
        let (_d, bringup, e) = workspace("zephyr_entry_robot1", CARGO);
        let l = for_entry(&e, "zephyr_entry_robot1", &bringup)
            .unwrap()
            .expect("claimed");
        assert_eq!(l.image.as_deref(), Some("zephyr_robot1"));
        assert_eq!(l.network.domain_id, Some(4));
        // `[image.zephyr] entry = "zephyr_entry"` derives `zephyr_entry` from
        // its id too, and names it — ONE claim, not two.
        let (_d, bringup, e) = workspace("zephyr_entry", CARGO);
        let l = for_entry(&e, "zephyr_entry", &bringup).unwrap().unwrap();
        assert_eq!(l.image.as_deref(), Some("zephyr"));
        assert_eq!(l.network.locator.as_deref(), Some("tcp/10.0.2.2:7430"));
    }

    #[test]
    fn an_entry_no_image_claims_has_no_deployment() {
        let (_d, bringup, e) = workspace("unrelated_entry", CARGO);
        assert_eq!(for_entry(&e, "unrelated_entry", &bringup).unwrap(), None);
    }

    #[test]
    fn two_images_claiming_one_entry_is_refused() {
        let (_d, bringup, e) = workspace("zephyr_entry", CARGO);
        let two =
            format!("{BRINGUP}\n[image.zephyr_b]\nboard = \"zephyr\"\nentry = \"zephyr_entry\"\n");
        std::fs::write(bringup.join("system.toml"), two).unwrap();
        let err = for_entry(&e, "zephyr_entry", &bringup).unwrap_err();
        assert!(err.contains("zephyr, zephyr_b"), "{err}");
    }

    #[test]
    fn an_entry_still_on_a_retired_key_is_refused_even_when_an_image_claims_it() {
        let (_d, bringup, e) = workspace(
            "zephyr_entry",
            &format!("{CARGO}\n[package.metadata.nros.entry]\ndeploy = \"zephyr\"\n"),
        );
        let err = for_entry(&e, "zephyr_entry", &bringup).unwrap_err();
        assert!(
            err.contains("[package.metadata.nros.entry] deploy"),
            "{err}"
        );
    }

    #[test]
    fn the_entry_name_rule_is_the_builders() {
        assert_eq!(entry_package_name("linux_robot1"), "linux_robot1_entry");
        assert_eq!(entry_package_name("esp32-qemu"), "esp32_qemu_entry");
        assert_eq!(entry_package_name("a.b/c"), "a_b_c_entry");
    }

    #[test]
    fn both_spellings_present_is_refused_naming_each_leftover() {
        let d = leaf(&[
            (
                "Cargo.toml",
                &format!(
                    "{CARGO}\n[package.metadata.nros.entry]\ndeploy = \"native\"\n\n\
                     [package.metadata.nros.deploy.native]\nrmw = \"zenoh\"\n"
                ),
            ),
            ("system.toml", SYSTEM),
        ]);
        let e = read(d.path()).unwrap_err();
        assert!(e.contains("[package.metadata.nros.entry] deploy"), "{e}");
        assert!(e.contains("[package.metadata.nros.deploy.native]"), "{e}");
        assert!(e.contains("system.toml"), "{e}");
    }

    #[test]
    fn an_entry_table_without_deploy_is_not_a_conflict() {
        // `max_callbacks` / `node_pkgs` are not deployment facts and do not
        // retire with RFC-0098.
        let d = leaf(&[
            (
                "Cargo.toml",
                &format!("{CARGO}\n[package.metadata.nros.entry]\nmax_callbacks = 8\n"),
            ),
            ("system.toml", SYSTEM),
        ]);
        assert!(read(d.path()).unwrap().is_some());
    }

    #[test]
    fn several_images_need_a_default() {
        let two = format!("{SYSTEM}\n[image.linux]\nboard = \"linux\"\n");
        let d = leaf(&[("Cargo.toml", CARGO), ("system.toml", &two)]);
        let e = read(d.path()).unwrap_err();
        assert!(e.contains("default_images"), "{e}");

        let chosen = two.replace("[system]\n", "[system]\ndefault_images = [\"linux\"]\n");
        let d = leaf(&[("Cargo.toml", CARGO), ("system.toml", &chosen)]);
        let l = read(d.path()).unwrap().unwrap();
        assert_eq!(l.board.as_deref(), Some("linux"));
        // `linux` sets no locator of its own: the system default applies.
        assert_eq!(l.network.locator.as_deref(), Some("tcp/10.0.2.2:7447"));
    }

    #[test]
    fn an_image_without_a_board_is_refused() {
        let bad = SYSTEM.replace("board = \"esp32-c3-baremetal\"\n", "");
        let d = leaf(&[("Cargo.toml", CARGO), ("system.toml", &bad)]);
        assert!(read(d.path()).unwrap_err().contains("names no `board`"));
    }

    #[test]
    fn image_defaults_fill_what_the_image_leaves_out() {
        let with_defaults = format!(
            "{}\n[image_defaults]\nrmw = \"xrce\"\nlocator = \"tcp/10.0.2.2:1\"\n",
            SYSTEM
        );
        let d = leaf(&[("Cargo.toml", CARGO), ("system.toml", &with_defaults)]);
        let l = read(d.path()).unwrap().unwrap();
        assert_eq!(l.rmw.as_deref(), Some("xrce"));
        // The image states its own locator, so the default does not reach it.
        assert_eq!(l.network.locator.as_deref(), Some("tcp/10.0.2.2:9800"));
    }

    /// RFC-0103 D10g (phase-497 W1) — the single-interface spellings are
    /// refused by name on the image and on the defaults, and the refusal names
    /// the `[image.<id>.net.<if>]` that replaces them.
    #[test]
    fn a_flat_ip_gateway_or_netmask_is_refused_naming_the_net_table() {
        for (extra, at) in [
            ("[image.a]\nboard = \"x\"\nip = \"10.0.2.50\"\n", "image.a"),
            (
                "[image.a]\nboard = \"x\"\n[image_defaults]\ngateway = \"10.0.2.2\"\n",
                "image_defaults",
            ),
        ] {
            let t = format!("[system]\nname = \"t\"\n{extra}");
            let d = leaf(&[("Cargo.toml", CARGO), ("system.toml", &t)]);
            let e = read(d.path()).unwrap_err();
            assert!(e.contains(&format!("`{at}`")) && e.contains("D10g"), "{e}");
        }
    }

    /// RFC-0103 D10d (phase-497 W2) — an image on a network that states a
    /// `router` dials it, unless the image states its own `locator`; the
    /// project's `[system] locator` is the fallback below both.
    #[test]
    fn the_networks_router_is_the_default_locator_below_the_images_own() {
        let net = "[network.lab]\nsubnet = \"192.168.50.0/24\"\n\
                   router = \"tcp/192.168.50.1:7447\"\n";
        let base = format!(
            "[system]\nname = \"t\"\nlocator = \"tcp/10.0.2.2:7447\"\n{net}\
             [image.a]\nboard = \"x\"\n[image.a.net.eth0]\nnetwork = \"lab\"\nhost = 20\n"
        );
        let d = leaf(&[("Cargo.toml", CARGO), ("system.toml", &base)]);
        let l = read(d.path()).unwrap().unwrap();
        assert_eq!(l.network.locator.as_deref(), Some("tcp/192.168.50.1:7447"));
        let own = base.replace(
            "board = \"x\"\n",
            "board = \"x\"\nlocator = \"tcp/192.168.50.9:7447\"\n",
        );
        let d = leaf(&[("Cargo.toml", CARGO), ("system.toml", &own)]);
        let l = read(d.path()).unwrap().unwrap();
        assert_eq!(l.network.locator.as_deref(), Some("tcp/192.168.50.9:7447"));
    }

    /// phase-445 W6 — the APP rung. `[image.<id>] env` is what replaced the
    /// `[env]` block a leaf hand-wrote in its own `.cargo/config.toml`.
    #[test]
    fn an_image_states_its_own_build_knobs() {
        let with_env = format!(
            "{SYSTEM}\nenv = {{ NROS_XRCE_BUFFER_SIZE = \"256\", \
             NROS_XRCE_CUSTOM_TRANSPORT_MTU = \"512\" }}\n"
        );
        let d = leaf(&[("Cargo.toml", CARGO), ("system.toml", &with_env)]);
        let l = read(d.path()).unwrap().unwrap();
        assert_eq!(l.env["NROS_XRCE_BUFFER_SIZE"], "256");
        assert_eq!(l.env["NROS_XRCE_CUSTOM_TRANSPORT_MTU"], "512");
        // A leaf that states none has an empty table, never an absent one.
        let plain = leaf(&[("Cargo.toml", CARGO), ("system.toml", SYSTEM)]);
        assert!(read(plain.path()).unwrap().unwrap().env.is_empty());
    }

    #[test]
    fn image_defaults_env_is_a_default_set_the_image_overrides_key_by_key() {
        let both = format!(
            "{SYSTEM}\nenv = {{ A = \"image\" }}\n\n\
             [image_defaults]\nenv = {{ A = \"base\", B = \"base\" }}\n"
        );
        let d = leaf(&[("Cargo.toml", CARGO), ("system.toml", &both)]);
        let l = read(d.path()).unwrap().unwrap();
        assert_eq!(l.env["A"], "image", "the image wins the key it also names");
        assert_eq!(l.env["B"], "base", "and inherits the one it does not");
    }

    /// phase-484 W8 (RFC-0103 D6) — `[image.<id>.knobs.<group>] <name>`,
    /// flattened to `<group>.<name>` and typed as written; nested groups join
    /// with `.`, and `[image_defaults.knobs]` sits underneath key by key.
    #[test]
    fn image_knobs_are_read_flat_and_typed_over_the_defaults() {
        let src = format!(
            "{SYSTEM}\n[image.esp32.knobs.zephyr]\nheap_size = 98304\n\n\
             [image.esp32.knobs.zenoh.tx]\nbatch = true\n\n\
             [image.esp32.knobs.log]\nmax_level = \"warn\"\n\n\
             [image_defaults.knobs.zephyr]\nheap_size = 1\n\n\
             [image_defaults.knobs.executor]\nmax_cbs = 16\n"
        );
        let d = leaf(&[("Cargo.toml", CARGO), ("system.toml", &src)]);
        let l = read(d.path()).unwrap().unwrap();
        assert_eq!(
            l.knobs["zephyr.heap_size"],
            KnobValue::Int(98304),
            "the image wins"
        );
        assert_eq!(
            l.knobs["executor.max_cbs"],
            KnobValue::Int(16),
            "the default fills in"
        );
        assert_eq!(l.knobs["zenoh.tx.batch"], KnobValue::Bool(true));
        assert_eq!(l.knobs["log.max_level"], KnobValue::Str("warn".into()));
        assert!(
            l.env.is_empty(),
            "knobs are not env rows until the CLI renders them"
        );
    }

    #[test]
    fn a_knob_with_no_group_or_an_unusable_value_is_refused() {
        for (body, want) in [
            ("[image.esp32.knobs]\nheap_size = 1\n", "has no group"),
            (
                "[image.esp32.knobs.zephyr]\nheap_size = 1.5\n",
                "is a float",
            ),
            (
                "[image.esp32.knobs.zephyr]\nheap_size = [1]\n",
                "is a array",
            ),
        ] {
            let d = leaf(&[
                ("Cargo.toml", CARGO),
                ("system.toml", &format!("{SYSTEM}\n{body}")),
            ]);
            let e = read(d.path()).unwrap_err();
            assert!(e.contains(want) && e.contains("[image.esp32.knobs]"), "{e}");
        }
    }

    #[test]
    fn a_non_string_env_value_names_the_spelling_to_write() {
        let bad = format!("{SYSTEM}\nenv = {{ N = 256 }}\n");
        let d = leaf(&[("Cargo.toml", CARGO), ("system.toml", &bad)]);
        let e = read(d.path()).unwrap_err();
        assert!(e.contains("N = \"256\""), "{e}");
    }

    /// RFC-0086 D2 — a transport that is not a link kind is an ERROR, not a
    /// pass-through. `talker-xrce` named its RMW here for four phases.
    #[test]
    fn a_transport_that_is_not_a_link_kind_is_refused() {
        let bad = SYSTEM.replace("[image.esp32]", "[image.esp32]\ntransport = \"xrce\"\n#");
        let d = leaf(&[("Cargo.toml", CARGO), ("system.toml", &bad)]);
        let e = read(d.path()).unwrap_err();
        assert!(e.contains("serial, tcp, udp"), "{e}");

        let good = bad.replace("transport = \"xrce\"", "transport = \"serial\"");
        let d = leaf(&[("Cargo.toml", CARGO), ("system.toml", &good)]);
        assert_eq!(
            read(d.path())
                .unwrap()
                .unwrap()
                .network
                .transport
                .as_deref(),
            Some("serial")
        );
    }

    #[test]
    fn a_malformed_entities_list_names_the_file() {
        let bad = SYSTEM.replace(
            "entities = [\"publisher:std_msgs/msg/String:/chatter\", \"timer\"]",
            "entities = \"timer\"",
        );
        let d = leaf(&[("Cargo.toml", CARGO), ("system.toml", &bad)]);
        let e = read(d.path()).unwrap_err();
        assert!(e.contains("system.toml") && e.contains("ARRAY"), "{e}");
    }

    /// Issue 1556 -- `entities = "census"` is the opt-in for a leaf whose
    /// entities are read from its own census: no list, and the flag set. Only
    /// that one word; any other string is still the malformed-list refusal.
    #[test]
    fn entities_census_is_the_census_opt_in_and_nothing_else_is() {
        let census = SYSTEM.replace(
            "entities = [\"publisher:std_msgs/msg/String:/chatter\", \"timer\"]",
            "entities = \"census\"",
        );
        let d = leaf(&[("Cargo.toml", CARGO), ("system.toml", &census)]);
        let l = read(d.path()).unwrap().unwrap();
        assert!(l.entities_from_census());
        assert_eq!(l.components[0].entities, None);
        assert_eq!(
            l.declared_entities(),
            None,
            "the opt-in is not a declaration"
        );

        let typo = SYSTEM.replace(
            "entities = [\"publisher:std_msgs/msg/String:/chatter\", \"timer\"]",
            "entities = \"Census\"",
        );
        let d = leaf(&[("Cargo.toml", CARGO), ("system.toml", &typo)]);
        let e = read(d.path()).unwrap_err();
        assert!(e.contains("ARRAY") && e.contains("\"census\""), "{e}");
    }
}
