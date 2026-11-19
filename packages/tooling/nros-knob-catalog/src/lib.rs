//! The merged configuration catalog (RFC-0140 D2/D4/D5, phase-581 W1-W3).
//!
//! One derivation, three consumers. [`Catalog::build`] puts core's rows (the
//! compiled tables of `nros-zephyr-build`, generated from `knobs.toml`)
//! together with every package's `nros-knobs.toml`, found beside the
//! descriptor of a package the provider scan reports. The `nros` CLI calls it
//! in memory (`nros fix`, the preflight, `nros knob list`) and `nros
//! configure` writes the same value to `<build>/nros/knobs/catalog.toml`
//! ([`catalog_path`]), named to every road as [`CATALOG_ENV`]; a package's
//! build script reads its own file through [`parse_own_file`] (via
//! `nros-knob-build`). The file is a cache and never the only source.
//!
//! This crate is the ONE loader of `nros-knobs.toml`: it used to live inside
//! the CLI, which a package's build script cannot reach (phase-581 W3).
//!
//! The compiled tables hold the IN-TREE set: core's rows and the rows of the
//! in-tree packages (zenoh, xrce, cyclonedds, freertos, threadx, zephyr),
//! each tagged with its package by `IN_TREE_PACKAGES`. A scanned package file
//! is merged over them under RFC-0140 D2: its owner word is DERIVED from the
//! package (an RMW's or platform's canonical name, a board's key) and checked
//! against the file's `owner` line; C1 one owner word, one package; C2 a row
//! stays under its owner, and no package takes a core word; C3 one spelling,
//! one row; C4 a package declares only its own library's foreign variables.
//! The same package found in two roots is not a collision: the later root
//! wins (the provider scan's precedence) and the other copy is reported as
//! shadowed.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::{Path, PathBuf},
};

use nros_zephyr_build::{
    CORE_OWNERS, ENV_ROWS, EnvDisposition, EnvRow, IN_TREE_PACKAGES, InTreePackage, KNOBS,
    KnobClass, KnobDefault, KnobRow, KnobScope, RetiredRoad, RetiredSpelling, STATEMENTS,
    StatementRow,
};
use serde::Serialize;

/// The file a package declares its rows in, beside its descriptor (RFC-0140 D2).
pub const PACKAGE_FILE: &str = "nros-knobs.toml";

/// The variable that names the merged catalog to a build (RFC-0140 D5): a
/// `relative = true` cargo `[env]` row and a cmake `-D`. The knob ladder
/// (`nros_zephyr_build::knob`) answers a name its compiled table lacks from it.
pub const CATALOG_ENV: &str = nros_zephyr_build::KNOB_CATALOG_ENV;

/// The descriptors a package file may sit beside, and the provider kind each
/// one names (RFC-0140 D2). The descriptor says which kind of package it is,
/// and so which of its announced names is its owner word.
pub const DESCRIPTORS: &[(&str, &str)] = &[
    ("nros-rmw.toml", "rmw"),
    ("nros-platform.toml", "platform"),
    ("nros-board.toml", "board"),
];

/// Where configure writes the merged catalog, under a workspace root.
#[must_use]
pub fn catalog_path(ws_root: &Path) -> PathBuf {
    knobs_dir(ws_root).join("catalog.toml")
}

/// `<build>/nros/knobs`, where configure writes every per-build view of the
/// catalog (RFC-0140 D5): `catalog.toml`, `registry.cmake`, `<owner>_env.h`.
#[must_use]
pub fn knobs_dir(ws_root: &Path) -> PathBuf {
    ws_root.join("build").join("nros").join("knobs")
}

/// A loader error: the text names the file and the rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

macro_rules! bail {
    ($($t:tt)*) => { return Err(Error(format!($($t)*))) };
}

fn err(s: impl Into<String>) -> Error {
    Error(s.into())
}

/// One package a scan found: what [`Catalog::build`] needs of it. The CLI
/// builds these from its provider scan; nothing here scans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageDir {
    /// `<name>` from its `package.xml`.
    pub package: String,
    /// The directory holding `package.xml` and its descriptor.
    pub dir: PathBuf,
    /// Its root's position on the search path; a LATER root wins.
    pub root_index: usize,
    /// Its root, as `nros knob list` prints it (the root's directory).
    pub root: Option<String>,
    /// Every `<nano_ros_provides kind=… name=…/>`, as `(kind, name)`.
    pub provides: Vec<(String, String)>,
}

/// A row's kind (RFC-0140 D1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RowKind {
    Knob,
    Statement,
    Env,
}

impl RowKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            RowKind::Knob => "knob",
            RowKind::Statement => "statement",
            RowKind::Env => "env",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        [RowKind::Knob, RowKind::Statement, RowKind::Env]
            .into_iter()
            .find(|k| k.as_str() == s)
    }
}

/// Who declared a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// Core's `knobs.toml`, compiled into `nros-zephyr-build`.
    Core,
    /// A package's `nros-knobs.toml`. For an in-tree package's compiled rows,
    /// `file` is relative to the checkout and `root` is `None`.
    Package {
        package: String,
        file: PathBuf,
        root: Option<String>,
    },
}

impl Origin {
    fn of_compiled(kind: RowKind, name: &str) -> Self {
        let owner = |p: &&InTreePackage| match kind {
            RowKind::Knob => p.knobs.contains(&name),
            RowKind::Env => p.env.contains(&name),
            RowKind::Statement => false,
        };
        match IN_TREE_PACKAGES.iter().find(owner) {
            Some(p) => Origin::Package {
                package: p.package.to_string(),
                file: PathBuf::from(p.file),
                root: None,
            },
            None => Origin::Core,
        }
    }

    /// The package that declared the row; `None` for core.
    #[must_use]
    pub fn package(&self) -> Option<&str> {
        match self {
            Origin::Core => None,
            Origin::Package { package, .. } => Some(package),
        }
    }

    /// Whether the row is compiled into `nros-zephyr-build` (core, or an
    /// in-tree package's): an out-of-tree row is only in the per-build views.
    #[must_use]
    pub fn is_compiled(&self) -> bool {
        match self {
            Origin::Core => true,
            Origin::Package { root, .. } => root.is_none(),
        }
    }
}

/// One package found in more than one root (RFC-0140 D2 C1's exception): the
/// copy the catalog uses and the ones it does not, nearest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shadowed {
    pub package: String,
    /// The `nros-knobs.toml` whose rows the catalog holds.
    pub winner: PathBuf,
    /// The other copies, in precedence order. An in-tree package's compiled
    /// rows appear as their checkout-relative file.
    pub shadowed: Vec<PathBuf>,
}

/// One spelling a row replaced (RFC-0140 D4), owned so a package row can hold one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetiredEntry {
    pub road: RetiredRoad,
    pub spelling: String,
    /// new value = old value x factor; 1 for a pure rename.
    pub factor: u32,
    pub from_unit: Option<String>,
    /// Set on a manual retirement: what to do instead. No edit is offered.
    pub instead: Option<String>,
}

impl RetiredEntry {
    /// A manual retirement: `nros fix` reports [`Self::instead`] and edits nothing.
    #[must_use]
    pub fn is_manual(&self) -> bool {
        self.instead.is_some()
    }

    fn from_compiled(s: &RetiredSpelling) -> Self {
        RetiredEntry {
            road: s.road,
            spelling: s.spelling.to_string(),
            factor: s.factor,
            from_unit: s.from_unit.map(str::to_string),
            instead: s.instead.map(str::to_string),
        }
    }
}

/// What a row says beyond its names: what the per-build views render
/// (`Kconfig.packages`, `registry.cmake`, a package's `nros_knobs.rs`) and
/// what `nros knob show` prints. Empty fields are fields the row does not state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Meta {
    /// A knob's type: `bool`, `int`, `hex` or `string`.
    pub ty: Option<String>,
    /// A knob's unconditional default, verbatim.
    pub default: Option<String>,
    /// A knob's `range <lo> <hi>`, inclusive.
    pub range: Option<(i64, i64)>,
    pub prompt: Option<String>,
    pub help: Option<String>,
    /// The Kconfig `if` a knob's definition sits under.
    pub kconfig_if: Option<String>,
    /// A knob's Kconfig attribute lines, verbatim (`default 4`, `range 1 64`).
    pub kconfig_attrs: Vec<String>,
    /// RFC-0133: `image`, `board` or `platform`.
    pub scope: Option<String>,
    /// RFC-0133: `public`, `carrier`, `internal` or `retired`.
    pub class: Option<String>,
    /// An env row's RFC-0140 D9 disposition: `respect`, `ignore`, `refuse`, `listed`.
    pub disposition: Option<String>,
    /// The project that defines a foreign variable.
    pub foreign: Option<String>,
    /// An env row's registry key (`acme.trace`), where it has one.
    pub key: Option<String>,
}

/// One row of the merged catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub kind: RowKind,
    /// A knob's or statement's key, or an env row's variable name.
    pub name: String,
    /// Every live spelling, by road. A retired spelling is in [`Self::retired`].
    pub spellings: Vec<(RetiredRoad, String)>,
    pub retired: Vec<RetiredEntry>,
    pub origin: Origin,
    pub meta: Meta,
}

impl Row {
    fn from_knob(k: &KnobRow) -> Self {
        let mut spellings = vec![
            (RetiredRoad::Toml, k.image_key.to_string()),
            (RetiredRoad::Env, k.env.to_string()),
            (RetiredRoad::Kconfig, k.kconfig.to_string()),
        ];
        if k.ty.is_some() {
            spellings.push((RetiredRoad::Cmake, format!("NROS_OVERRIDE_{}", k.id)));
        }
        let default = match k.default {
            KnobDefault::Kconfig(ds) => ds
                .iter()
                .find(|d| d.when.is_none())
                .map(|d| d.value.to_string()),
            _ => None,
        };
        let symbol = k.kconfig.strip_prefix("CONFIG_").unwrap_or(k.kconfig);
        Row {
            kind: RowKind::Knob,
            name: k.image_key.to_string(),
            spellings,
            retired: k
                .retired_spellings
                .iter()
                .map(RetiredEntry::from_compiled)
                .collect(),
            origin: Origin::of_compiled(RowKind::Knob, k.image_key),
            meta: Meta {
                ty: k.ty.map(str::to_string),
                default,
                range: k.range,
                prompt: k.prompt.map(str::to_string),
                help: k.help.map(str::to_string),
                kconfig_if: nros_zephyr_build::kconfig_knob(symbol)
                    .and_then(|kk| kk.kconfig_if)
                    .map(str::to_string),
                kconfig_attrs: Vec::new(),
                scope: Some(
                    match k.scope {
                        KnobScope::Image => "image",
                        KnobScope::Board => "board",
                        KnobScope::Platform => "platform",
                    }
                    .to_string(),
                ),
                class: Some(
                    match k.class {
                        KnobClass::Public => "public",
                        KnobClass::Carrier => "carrier",
                        KnobClass::Internal => "internal",
                        KnobClass::Retired => "retired",
                    }
                    .to_string(),
                ),
                disposition: None,
                foreign: None,
                key: None,
            },
        }
    }

    fn from_statement(s: &StatementRow) -> Self {
        let mut spellings = vec![(RetiredRoad::Toml, s.key.to_string())];
        if let Some(c) = s.carrier {
            spellings.push((RetiredRoad::Env, c.to_string()));
        }
        if let Some(k) = s.kconfig {
            spellings.push((RetiredRoad::Kconfig, k.to_string()));
        }
        Row {
            kind: RowKind::Statement,
            name: s.key.to_string(),
            spellings,
            retired: s
                .retired_spellings
                .iter()
                .map(RetiredEntry::from_compiled)
                .collect(),
            origin: Origin::Core,
            meta: Meta {
                help: s.help.map(str::to_string),
                ..Meta::default()
            },
        }
    }

    fn from_env(e: &EnvRow) -> Self {
        Row {
            kind: RowKind::Env,
            name: e.name.to_string(),
            spellings: vec![(RetiredRoad::Run, e.name.to_string())],
            retired: e
                .retired_spellings
                .iter()
                .map(RetiredEntry::from_compiled)
                .collect(),
            origin: Origin::of_compiled(RowKind::Env, e.name),
            meta: Meta {
                help: e.help.map(str::to_string),
                disposition: Some(
                    match e.disposition {
                        EnvDisposition::Respect => "respect",
                        EnvDisposition::Ignore => "ignore",
                        EnvDisposition::Refuse => "refuse",
                        EnvDisposition::Listed => "listed",
                    }
                    .to_string(),
                ),
                foreign: e.foreign.map(str::to_string),
                key: e.key.map(str::to_string),
                ..Meta::default()
            },
        }
    }

    /// The live spelling on `road`, if the row has one.
    #[must_use]
    pub fn spelling(&self, road: RetiredRoad) -> Option<&str> {
        self.spellings
            .iter()
            .find(|(r, _)| *r == road)
            .map(|(_, s)| s.as_str())
    }

    /// The owner word: a key's first segment; for an env row its key's, else
    /// its foreign project's, else `core`.
    #[must_use]
    pub fn owner(&self) -> &str {
        match self.kind {
            RowKind::Env => self
                .meta
                .key
                .as_deref()
                .and_then(|k| k.split('.').next())
                .or(self.meta.foreign.as_deref())
                .unwrap_or("core"),
            _ => self.name.split('.').next().unwrap_or(&self.name),
        }
    }

    fn label(&self) -> String {
        let kind = self.kind.as_str();
        match &self.origin {
            Origin::Core => format!("{kind} `{}` (core knobs.toml)", self.name),
            Origin::Package { package, file, .. } => {
                format!(
                    "{kind} `{}` (package {package}, {})",
                    self.name,
                    file.display()
                )
            }
        }
    }
}

/// The merged catalog: the compiled in-tree rows, then each scanned
/// package's, in scan order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Catalog {
    rows: Vec<Row>,
    shadowed: Vec<Shadowed>,
    /// Owner word -> package, for every package that contributed rows.
    owners: BTreeMap<String, String>,
}

/// A package file the scan found, with the package it belongs to.
struct Found<'a> {
    package: &'a PackageDir,
    file: PathBuf,
}

impl Catalog {
    /// Core's rows alone, from the compiled tables (in-tree packages included).
    #[must_use]
    pub fn core() -> Self {
        let rows = KNOBS
            .iter()
            .map(Row::from_knob)
            .chain(STATEMENTS.iter().map(Row::from_statement))
            .chain(ENV_ROWS.iter().map(Row::from_env))
            .collect();
        Catalog {
            rows,
            shadowed: Vec::new(),
            owners: IN_TREE_PACKAGES
                .iter()
                .map(|p| (p.owner.to_string(), p.package.to_string()))
                .collect(),
        }
    }

    /// Core plus every scanned package's `nros-knobs.toml` (RFC-0140 D2: the
    /// file's presence beside the descriptor is the declaration). `read`
    /// reads a package file; the CLI passes one that records it as a
    /// configure input.
    ///
    /// # Errors
    ///
    /// A package file that does not parse or breaks one of RFC-0140 D2's
    /// rules C1 to C4.
    pub fn build(
        packages: &[PackageDir],
        read: &dyn Fn(&Path) -> std::io::Result<String>,
    ) -> Result<Self> {
        let mut cat = Catalog::core();

        // Every copy of each package, highest precedence first: a LATER root
        // wins, as in the provider scan.
        let mut seen_dirs = BTreeSet::new();
        let mut copies: BTreeMap<&str, Vec<Found<'_>>> = BTreeMap::new();
        for p in packages {
            let file = p.dir.join(PACKAGE_FILE);
            if !seen_dirs.insert(p.dir.clone()) || !file.is_file() {
                continue;
            }
            copies
                .entry(p.package.as_str())
                .or_default()
                .push(Found { package: p, file });
        }
        for list in copies.values_mut() {
            list.sort_by(|a, b| {
                b.package
                    .root_index
                    .cmp(&a.package.root_index)
                    .then(a.package.dir.cmp(&b.package.dir))
            });
        }

        // C1's claims: owner word -> (package, where it was found).
        let mut claims: BTreeMap<String, (String, String)> = IN_TREE_PACKAGES
            .iter()
            .map(|p| {
                (
                    p.owner.to_string(),
                    (p.package.to_string(), format!("{} (built in)", p.file)),
                )
            })
            .collect();
        let mut problems = Vec::new();
        for (package, list) in &copies {
            let Some((winner, losers)) = list.split_first() else {
                continue;
            };
            let text = read(&winner.file)
                .map_err(|e| err(format!("reading {}: {e}", winner.file.display())))?;
            let (owner, rows) = parse_package(winner.package, &winner.file, &text)?;
            let mut shadowed: Vec<PathBuf> = losers.iter().map(|f| f.file.clone()).collect();
            if let Some(intree) = IN_TREE_PACKAGES.iter().find(|p| p.package == *package) {
                // The compiled rows ARE this package's in-tree copy. A scanned
                // copy that says the same is that copy; one that differs
                // overlays it.
                let compiled: Vec<&Row> = cat
                    .rows
                    .iter()
                    .filter(|r| r.origin.package() == Some(package))
                    .collect();
                if !same_rows(&compiled, &rows) {
                    cat.rows.retain(|r| r.origin.package() != Some(package));
                    cat.rows.extend(rows);
                    shadowed.push(PathBuf::from(intree.file));
                }
            } else {
                cat.rows.extend(rows);
            }
            let here = format!(
                "{} (root {})",
                winner.file.display(),
                winner.package.root_index
            );
            if CORE_OWNERS.contains(&owner.as_str()) {
                // RFC-0140 D2 C2: core cannot take a package's word, nor a
                // package a core one.
                problems.push(format!(
                    "{here}: owner word `{owner}` is a core owner; a package cannot declare \
                     a core owner's rows"
                ));
            }
            match claims.get(&owner) {
                Some((other, at)) if other != package => problems.push(format!(
                    "owner word `{owner}` is claimed by two packages: {other} ({at}) and \
                     {package} ({here}); one owner word belongs to one package"
                )),
                _ => {
                    claims.insert(owner.clone(), (package.to_string(), here));
                    cat.owners.insert(owner, package.to_string());
                }
            }
            if !shadowed.is_empty() {
                cat.shadowed.push(Shadowed {
                    package: package.to_string(),
                    winner: winner.file.clone(),
                    shadowed,
                });
            }
        }
        if !problems.is_empty() {
            // RFC-0140 D2 rules C1 and C2.
            bail!(
                "the configuration catalog has an owner word problem:\n  {}",
                problems.join("\n  ")
            );
        }
        cat.check_one_namespace()?;
        Ok(cat)
    }

    /// Packages found in more than one root, and which copy the catalog uses
    /// (reported, never refused: RFC-0140 D2 C1).
    #[must_use]
    pub fn shadowed(&self) -> &[Shadowed] {
        &self.shadowed
    }

    /// Every row, core first.
    #[must_use]
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// The knob a project states as `[image.<id>.knobs.<group>] <name>`, by
    /// its dotted key.
    #[must_use]
    pub fn knob(&self, key: &str) -> Option<&Row> {
        self.rows
            .iter()
            .find(|r| r.kind == RowKind::Knob && r.name == key)
    }

    /// A row by any of its names: a key, an env variable, a build env or
    /// Kconfig spelling (with or without `CONFIG_`).
    #[must_use]
    pub fn find(&self, name: &str) -> Option<&Row> {
        let config = format!("CONFIG_{name}");
        self.rows
            .iter()
            .find(|r| r.name == name || r.spellings.iter().any(|(_, s)| *s == name || *s == config))
    }

    /// The package that owns `owner`, when a package does (RFC-0140 D2).
    #[must_use]
    pub fn owner_package(&self, owner: &str) -> Option<&str> {
        self.owners.get(owner).map(String::as_str)
    }

    /// The rows of packages outside the compiled set (out-of-tree rows): what
    /// the per-build views carry that the committed views cannot.
    pub fn package_rows(&self) -> impl Iterator<Item = &Row> {
        self.rows.iter().filter(|r| !r.origin.is_compiled())
    }

    /// The row that retired `spelling` on `road`, and its entry (RFC-0140 D4).
    /// A Kconfig spelling is matched with or without `CONFIG_`.
    #[must_use]
    pub fn retired_spelling(
        &self,
        road: RetiredRoad,
        spelling: &str,
    ) -> Option<(&Row, &RetiredEntry)> {
        let want = |s: &str| {
            s == spelling
                || (road == RetiredRoad::Kconfig && s.strip_prefix("CONFIG_") == Some(spelling))
        };
        self.rows.iter().find_map(|r| {
            r.retired
                .iter()
                .find(|e| e.road == road && want(&e.spelling))
                .map(|e| (r, e))
        })
    }

    /// RFC-0140 D1/D2 C3: every spelled name, live or retired, names one row.
    fn check_one_namespace(&self) -> Result<()> {
        let mut seen: BTreeMap<&str, &Row> = BTreeMap::new();
        let mut clashes = Vec::new();
        for row in &self.rows {
            let names = row
                .spellings
                .iter()
                .map(|(_, s)| s.as_str())
                .chain(row.retired.iter().map(|e| e.spelling.as_str()));
            let mut mine = BTreeSet::new();
            for s in names {
                if !mine.insert(s) {
                    continue;
                }
                match seen.get(s) {
                    Some(other) if !std::ptr::eq(*other, row) => clashes.push(format!(
                        "`{s}` is spelled by {} and by {}",
                        other.label(),
                        row.label()
                    )),
                    _ => {
                        seen.insert(s, row);
                    }
                }
            }
        }
        if !clashes.is_empty() {
            // RFC-0140 D2 rule C3.
            bail!(
                "the configuration catalog uses a name twice; each name belongs to one \
                 row, so rename one of them:\n  {}",
                clashes.join("\n  ")
            );
        }
        Ok(())
    }

    /// The catalog as `catalog.toml`'s text.
    ///
    /// The shape is read back by two readers: [`Catalog::parse_toml`] and the
    /// knob ladder's dependency-free reader in `nros-zephyr-build`
    /// (`catalog_knob`), which reads only `[[row]]`, `name`, `spellings` and
    /// `[[row.retired]] spelling`. `the_ladder_reads_the_catalog_this_writes`
    /// holds the two together.
    #[must_use]
    pub fn render_toml(&self) -> String {
        #[derive(Serialize)]
        struct FileRow<'a> {
            kind: RowKind,
            name: &'a str,
            origin: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            root: Option<&'a str>,
            spellings: Vec<String>,
            #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
            ty: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            default: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            range: Option<[i64; 2]>,
            #[serde(skip_serializing_if = "Option::is_none")]
            disposition: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            prompt: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            kconfig_if: Option<&'a str>,
            #[serde(skip_serializing_if = "<[String]>::is_empty")]
            kconfig_attrs: &'a [String],
            #[serde(skip_serializing_if = "Option::is_none")]
            scope: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            class: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            foreign: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            key: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            help: Option<&'a str>,
            #[serde(skip_serializing_if = "Vec::is_empty")]
            retired: Vec<FileRetired<'a>>,
        }
        #[derive(Serialize)]
        struct FileRetired<'a> {
            spelling: String,
            factor: u32,
            #[serde(skip_serializing_if = "Option::is_none")]
            from_unit: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            instead: Option<&'a str>,
        }
        #[derive(Serialize)]
        struct File<'a> {
            row: Vec<FileRow<'a>>,
        }
        let file = File {
            row: self
                .rows
                .iter()
                .map(|r| FileRow {
                    kind: r.kind,
                    name: &r.name,
                    origin: match &r.origin {
                        Origin::Core => "core".to_string(),
                        Origin::Package { package, .. } => package.clone(),
                    },
                    root: match &r.origin {
                        Origin::Package {
                            root: Some(root), ..
                        } => Some(root),
                        _ => None,
                    },
                    spellings: r
                        .spellings
                        .iter()
                        .map(|(road, s)| format!("{}:{s}", road.as_str()))
                        .collect(),
                    ty: r.meta.ty.as_deref(),
                    default: r.meta.default.as_deref(),
                    range: r.meta.range.map(|(lo, hi)| [lo, hi]),
                    disposition: r.meta.disposition.as_deref(),
                    prompt: r.meta.prompt.as_deref(),
                    kconfig_if: r.meta.kconfig_if.as_deref(),
                    kconfig_attrs: &r.meta.kconfig_attrs,
                    scope: r.meta.scope.as_deref(),
                    class: r.meta.class.as_deref(),
                    foreign: r.meta.foreign.as_deref(),
                    key: r.meta.key.as_deref(),
                    help: r.meta.help.as_deref(),
                    retired: r
                        .retired
                        .iter()
                        .map(|e| FileRetired {
                            spelling: format!("{}:{}", e.road.as_str(), e.spelling),
                            factor: e.factor,
                            from_unit: e.from_unit.as_deref(),
                            instead: e.instead.as_deref(),
                        })
                        .collect(),
                })
                .collect(),
        };
        let body = toml::to_string(&file).expect("the catalog serialises");
        format!(
            "# @generated by `nros configure` from the configuration registry (core\n\
             # knobs.toml plus each package's nros-knobs.toml) -- do not edit. A cache\n\
             # for the build: `nros` itself builds the same catalog in memory.\n\n{body}"
        )
    }

    /// `catalog.toml`'s rows read back: names, origin, spellings, retired
    /// spellings and every [`Meta`] field, so a per-build view rendered from
    /// the file equals one rendered in memory. What a build-side reader
    /// that cannot run the scan sees (`nros-platform-config`'s validation of a
    /// descriptor's `[knobs]`).
    ///
    /// # Errors
    ///
    /// Text that is not a catalog.
    pub fn parse_toml(text: &str) -> Result<Self> {
        let doc: toml::Table =
            toml::from_str(text).map_err(|e| err(format!("parsing catalog.toml: {e}")))?;
        let mut rows = Vec::new();
        let mut owners = BTreeMap::new();
        let list = doc
            .get("row")
            .and_then(toml::Value::as_array)
            .cloned()
            .unwrap_or_default();
        for t in &list {
            let s = |k: &str| t.get(k).and_then(toml::Value::as_str).map(str::to_string);
            let kind = s("kind")
                .as_deref()
                .and_then(RowKind::parse)
                .ok_or_else(|| err("catalog.toml: a row without a kind"))?;
            let name = s("name").ok_or_else(|| err("catalog.toml: a row without a name"))?;
            let road_name = |v: &str| -> Result<(RetiredRoad, String)> {
                v.split_once(':')
                    .and_then(|(r, n)| RetiredRoad::parse(r).map(|r| (r, n.to_string())))
                    .ok_or_else(|| err(format!("catalog.toml: `{v}` names no road")))
            };
            let spellings = t
                .get("spellings")
                .and_then(toml::Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(toml::Value::as_str)
                        .map(road_name)
                        .collect::<Result<Vec<_>>>()
                })
                .transpose()?
                .unwrap_or_default();
            let mut retired = Vec::new();
            for e in t
                .get("retired")
                .and_then(toml::Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default()
            {
                let es = |k: &str| e.get(k).and_then(toml::Value::as_str).map(str::to_string);
                let (road, spelling) = road_name(&es("spelling").unwrap_or_default())?;
                retired.push(RetiredEntry {
                    road,
                    spelling,
                    factor: e
                        .get("factor")
                        .and_then(toml::Value::as_integer)
                        .and_then(|f| u32::try_from(f).ok())
                        .unwrap_or(1),
                    from_unit: es("from_unit"),
                    instead: es("instead"),
                });
            }
            let origin = match s("origin").as_deref() {
                None | Some("core") => Origin::Core,
                Some(p) => Origin::Package {
                    package: p.to_string(),
                    file: PathBuf::new(),
                    root: s("root"),
                },
            };
            if let (Origin::Package { package, .. }, Some(owner)) =
                (&origin, name.split_once('.').map(|(o, _)| o))
            {
                owners.insert(owner.to_string(), package.clone());
            }
            rows.push(Row {
                kind,
                name,
                spellings,
                retired,
                origin,
                meta: Meta {
                    ty: s("type"),
                    default: s("default"),
                    range: t
                        .get("range")
                        .and_then(toml::Value::as_array)
                        .and_then(
                            |a| match (a.first()?.as_integer(), a.get(1)?.as_integer()) {
                                (Some(lo), Some(hi)) => Some((lo, hi)),
                                _ => None,
                            },
                        ),
                    disposition: s("disposition"),
                    prompt: s("prompt"),
                    kconfig_if: s("kconfig_if"),
                    kconfig_attrs: t
                        .get("kconfig_attrs")
                        .and_then(toml::Value::as_array)
                        .map(|a| {
                            a.iter()
                                .filter_map(toml::Value::as_str)
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default(),
                    scope: s("scope"),
                    class: s("class"),
                    foreign: s("foreign"),
                    key: s("key"),
                    help: s("help"),
                },
            });
        }
        Ok(Catalog {
            rows,
            shadowed: Vec::new(),
            owners,
        })
    }
}

/// The owner word RFC-0140 D2 derives for a package: an RMW's or platform's
/// canonical name (the first it announces), a board's key (one of those it
/// announces, the one its `owner` line names). `-` reads as `_`. The kind is
/// the descriptor the file sits beside.
fn derive_owner(p: &PackageDir, file: &Path, stated: Option<&str>) -> Result<String> {
    let kinds: Vec<&str> = DESCRIPTORS
        .iter()
        .filter(|(d, _)| p.dir.join(d).is_file())
        .map(|(_, k)| *k)
        .collect();
    let [kind] = kinds.as_slice() else {
        bail!(
            "{}: an {PACKAGE_FILE} sits beside exactly one descriptor ({}), found {}",
            file.display(),
            DESCRIPTORS
                .iter()
                .map(|(d, _)| *d)
                .collect::<Vec<_>>()
                .join(", "),
            kinds.len()
        );
    };
    let names: Vec<String> = p
        .provides
        .iter()
        .filter(|(k, _)| k == kind)
        .map(|(_, n)| n.replace('-', "_"))
        .collect();
    let Some(first) = names.first() else {
        bail!(
            "{}: package {} announces no {kind} (`<nano_ros_provides kind=\"{kind}\">`), \
             so it has no owner word",
            file.display(),
            p.package
        );
    };
    if *kind != "board" {
        return Ok(first.clone());
    }
    match stated {
        Some(s) if names.iter().any(|n| n == s) => Ok(s.to_string()),
        _ => bail!(
            "{}: a board package's owner word is one of its board keys ({}); its `owner` \
             line names which",
            file.display(),
            names.join(", ")
        ),
    }
}

/// Do two row sets say the same thing (kind, name, spellings, retired),
/// whatever their origin and order?
fn same_rows(a: &[&Row], b: &[Row]) -> bool {
    fn sorted<'r>(rows: impl Iterator<Item = &'r Row>) -> Vec<(u8, &'r Row)> {
        let mut v: Vec<_> = rows.map(|r| (r.kind as u8, r)).collect();
        v.sort_by(|p, q| (&p.1.name, p.0).cmp(&(&q.1.name, q.0)));
        v
    }
    let (x, y) = (sorted(a.iter().copied()), sorted(b.iter()));
    x.len() == y.len()
        && x.iter().zip(&y).all(|((_, p), (_, q))| {
            (p.kind, &p.name, &p.spellings, &p.retired)
                == (q.kind, &q.name, &q.spellings, &q.retired)
        })
}

/// The owner word and rows of one scanned package's `nros-knobs.toml`, with
/// RFC-0140 D2's per-file rules: the `owner` line equals the DERIVED word,
/// every key is under it (C2), and a foreign variable is the package's own
/// library's (C4).
///
/// # Errors
///
/// The file breaks one of those rules or does not parse.
pub fn parse_package(p: &PackageDir, file: &Path, text: &str) -> Result<(String, Vec<Row>)> {
    let doc = parse_doc(file, text)?;
    let stated = doc.get("owner").and_then(toml::Value::as_str);
    let owner = derive_owner(p, file, stated)?;
    if stated != Some(owner.as_str()) {
        bail!(
            "{}: `owner = {}`, but package {}'s derived owner word is `{owner}`; state it so \
             the file reads on its own",
            file.display(),
            stated.map_or_else(|| "(missing)".to_string(), |s| format!("\"{s}\"")),
            p.package
        );
    }
    let origin = Origin::Package {
        package: p.package.clone(),
        file: file.to_path_buf(),
        root: Some(p.root.clone().unwrap_or_default()),
    };
    let rows = parse_rows(&doc, &owner, &origin, file)?;
    Ok((owner, rows))
}

/// A package's OWN `nros-knobs.toml`, as its build script reads it
/// (`nros_knob_build::emit`): the owner its `owner` line states and its rows,
/// with C2 and C4 checked against that word. Deriving the word needs the
/// package scan, which a build script does not run; `nros configure` checks
/// the derivation for every package the image links.
///
/// # Errors
///
/// The file breaks C2 or C4, states no `owner`, or does not parse.
pub fn parse_own_file(package: &str, file: &Path, text: &str) -> Result<(String, Vec<Row>)> {
    let doc = parse_doc(file, text)?;
    let owner = doc
        .get("owner")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| err(format!("{}: states no `owner`", file.display())))?
        .to_string();
    let origin = Origin::Package {
        package: package.to_string(),
        file: file.to_path_buf(),
        root: Some(String::new()),
    };
    let rows = parse_rows(&doc, &owner, &origin, file)?;
    Ok((owner, rows))
}

fn parse_doc(file: &Path, text: &str) -> Result<toml::Table> {
    let doc: toml::Table =
        toml::from_str(text).map_err(|e| err(format!("parsing {}: {e}", file.display())))?;
    if let Some(extra) = doc
        .keys()
        .find(|k| !matches!(k.as_str(), "owner" | "knob" | "env"))
    {
        bail!(
            "{}: `{extra}`: a package file holds `owner`, `[[knob]]` and `[[env]]` rows \
             (statements and [owners] are core's)",
            file.display()
        );
    }
    Ok(doc)
}

/// The `default`, `range` of a row's Kconfig attribute lines: the last
/// unconditional `default`, and the `range`.
fn attrs_default_range(attrs: &[String]) -> (Option<String>, Option<(i64, i64)>) {
    let mut default = None;
    let mut range = None;
    for a in attrs {
        let a = a.trim();
        if let Some(v) = a.strip_prefix("default ")
            && !v.contains(" if ")
        {
            default = Some(v.trim().trim_matches('"').to_string());
        }
        if let Some(v) = a.strip_prefix("range ") {
            let mut it = v.split_whitespace().map(str::parse::<i64>);
            if let (Some(Ok(lo)), Some(Ok(hi))) = (it.next(), it.next()) {
                range = Some((lo, hi));
            }
        }
    }
    (default, range)
}

fn parse_rows(doc: &toml::Table, owner: &str, origin: &Origin, file: &Path) -> Result<Vec<Row>> {
    let where_ = |what: &str| err(format!("{}: {what}", file.display()));
    let under = |key: &str| key.split('.').next() == Some(owner) && key.contains('.');
    let mut rows = Vec::new();
    for (kind, list) in [("knob", doc.get("knob")), ("env", doc.get("env"))] {
        let Some(list) = list else { continue };
        let list = list
            .as_array()
            .ok_or_else(|| where_(&format!("`{kind}` is an array of tables")))?;
        for t in list {
            let t = t
                .as_table()
                .ok_or_else(|| where_(&format!("a `{kind}` row is a table")))?;
            let trans = t.get("transitional").and_then(toml::Value::as_table);
            let tstr = |k: &str| trans.and_then(|tt| tt.get(k)).and_then(toml::Value::as_str);
            let s = |k: &str| t.get(k).and_then(toml::Value::as_str).map(str::to_string);
            let mut retired = parse_retired(t.get("retired"), &where_)?;
            // A retired knob's entries all refuse with the row's reason.
            if t.get("class").and_then(toml::Value::as_str) == Some("retired") {
                let why = t.get("instead").and_then(toml::Value::as_str);
                for e in retired.iter_mut().filter(|e| e.instead.is_none()) {
                    e.instead = why.map(str::to_string);
                }
            }
            let row = if kind == "knob" {
                let key = t
                    .get("key")
                    .and_then(toml::Value::as_str)
                    .ok_or_else(|| where_("a knob row states `key`"))?;
                if !under(key) {
                    return Err(where_(&format!(
                        "key `{key}` is not under this package's owner word `{owner}`; a \
                         package declares only its own rows"
                    )));
                }
                let derived = derive_id(key);
                let id = tstr("id").unwrap_or(&derived).to_string();
                let kconfig = tstr("kconfig")
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("NROS_{derived}"));
                let mut spellings = vec![
                    (RetiredRoad::Toml, key.to_string()),
                    (RetiredRoad::Env, format!("NROS_{id}")),
                    (RetiredRoad::Kconfig, format!("CONFIG_{kconfig}")),
                ];
                if t.contains_key("type") {
                    spellings.push((RetiredRoad::Cmake, format!("NROS_OVERRIDE_{id}")));
                }
                let kconfig_attrs: Vec<String> = t
                    .get("kconfig_attrs")
                    .and_then(toml::Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(toml::Value::as_str)
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                let (default, range) = attrs_default_range(&kconfig_attrs);
                Row {
                    kind: RowKind::Knob,
                    name: key.to_string(),
                    spellings,
                    retired,
                    origin: origin.clone(),
                    meta: Meta {
                        ty: s("type"),
                        default,
                        range,
                        prompt: s("prompt"),
                        help: s("help").map(|h| h.trim_end().to_string()),
                        kconfig_if: s("kconfig_if"),
                        kconfig_attrs,
                        scope: s("scope"),
                        class: s("class"),
                        disposition: None,
                        foreign: None,
                        key: None,
                    },
                }
            } else {
                let (name, key) = match (t.get("key").and_then(toml::Value::as_str), t.get("name"))
                {
                    (Some(key), None) => {
                        if !under(key) {
                            return Err(where_(&format!(
                                "key `{key}` is not under this package's owner word \
                                 `{owner}`; a package declares only its own rows"
                            )));
                        }
                        (
                            tstr("name")
                                .map(str::to_string)
                                .unwrap_or_else(|| format!("NROS_{}", derive_id(key))),
                            Some(key.to_string()),
                        )
                    }
                    (None, Some(n)) => {
                        let n = n.as_str().ok_or_else(|| where_("`name` is a string"))?;
                        let foreign = t.get("foreign").and_then(toml::Value::as_str);
                        if foreign != Some(owner) {
                            return Err(where_(&format!(
                                "`{n}`: a package declares only its own library's variables \
                                 (`foreign = \"{owner}\"`); ROS 2's, rcutils' and Rust's \
                                 are core's"
                            )));
                        }
                        (n.to_string(), None)
                    }
                    _ => return Err(where_("an env row states `key` or `name`, exactly one")),
                };
                Row {
                    kind: RowKind::Env,
                    spellings: vec![(RetiredRoad::Run, name.clone())],
                    name,
                    retired,
                    origin: origin.clone(),
                    meta: Meta {
                        key,
                        help: s("help").map(|h| h.trim_end().to_string()),
                        disposition: Some(s("disposition").unwrap_or_else(|| "listed".into())),
                        foreign: s("foreign"),
                        ..Meta::default()
                    },
                }
            };
            rows.push(row);
        }
    }
    Ok(rows)
}

fn parse_retired(
    v: Option<&toml::Value>,
    where_: &dyn Fn(&str) -> Error,
) -> Result<Vec<RetiredEntry>> {
    let Some(v) = v else { return Ok(Vec::new()) };
    let list = v
        .as_array()
        .ok_or_else(|| where_("`retired` is a list of { spelling = \"<road>:<name>\" }"))?;
    let mut out = Vec::new();
    for e in list {
        let t = e
            .as_table()
            .ok_or_else(|| where_("a retired entry is { spelling = \"<road>:<name>\", ... }"))?;
        let spelling = t
            .get("spelling")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| where_("a retired entry states `spelling`"))?;
        let (road, name) = spelling
            .split_once(':')
            .and_then(|(r, n)| RetiredRoad::parse(r).map(|r| (r, n)))
            .ok_or_else(|| {
                where_(&format!(
                    "`{spelling}` does not start with toml:, env:, kconfig:, cmake: or run:"
                ))
            })?;
        let factor = match t.get("factor") {
            None => 1,
            Some(f) => f
                .as_integer()
                .and_then(|f| u32::try_from(f).ok())
                .filter(|f| *f >= 1)
                .ok_or_else(|| where_(&format!("`{spelling}`: `factor` is a positive integer")))?,
        };
        let instead = t
            .get("instead")
            .and_then(toml::Value::as_str)
            .map(str::to_string);
        if instead.is_some() && t.contains_key("factor") {
            return Err(where_(&format!(
                "`{spelling}` states both `instead` and `factor`: an entry is a mechanical \
                 rename or a manual one, never both"
            )));
        }
        out.push(RetiredEntry {
            road,
            spelling: name.to_string(),
            factor,
            from_unit: t
                .get("from_unit")
                .and_then(toml::Value::as_str)
                .map(str::to_string),
            instead,
        });
    }
    Ok(out)
}

/// `zenoh.tx_batch` -> `ZENOH_TX_BATCH` (RFC-0140 D3 N2).
#[must_use]
pub fn derive_id(key: &str) -> String {
    key.to_ascii_uppercase().replace('.', "_")
}

mod views;
pub use views::{env_header, kconfig_packages, kconfig_rows, registry_cmake};

#[cfg(test)]
mod tests;
