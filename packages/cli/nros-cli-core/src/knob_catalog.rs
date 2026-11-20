//! The merged configuration catalog (RFC-0140 D4/D5, phase-581 W1).
//!
//! One derivation, two consumers. [`Catalog::build`] puts core's rows (the
//! compiled tables of `nros-zephyr-build`, generated from `knobs.toml`)
//! together with every package's `nros-knobs.toml`, found beside the
//! descriptor of a provider the provider scan reports — the scan `nros
//! configure` already runs. `nros fix` and the build preflight call it in
//! memory, before any configure, possibly on a fresh clone; configure writes
//! the same value to `<build>/nros/knobs/catalog.toml` for out-of-process
//! readers (cmake, Kconfig). The file is a cache and never the only source.
//!
//! [`Catalog::retired_spelling`] is the one read path for a retired spelling
//! on any road: nothing parses `knobs.toml` by hand.
//!
//! The compiled tables hold the IN-TREE set: core's rows and the rows of the
//! in-tree packages (zenoh, xrce, cyclonedds, freertos, threadx, zephyr) that
//! moved to their own `nros-knobs.toml` (phase-581 W2), each tagged with its
//! package by `IN_TREE_PACKAGES`. A scanned package file is merged over them
//! under RFC-0140 D2: its owner word is DERIVED from the package (an RMW's or
//! platform's canonical name, a board's key) and checked against the file's
//! `owner` line; C1 one owner word, one package; C2 a row stays under its
//! owner, and no package takes a core word; C3 one spelling, one row; C4 a
//! package declares only its own library's foreign variables. The same
//! package found in two roots is not a collision: the later root wins (the
//! provider scan's precedence) and the other copy is reported as shadowed.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use cargo_nano_ros::provider_scan::ProviderPackage;
use eyre::{Result, WrapErr, bail};
use nros_zephyr_build::{
    CORE_OWNERS, ENV_ROWS, EnvRow, IN_TREE_PACKAGES, InTreePackage, KNOBS, KnobRow, RetiredRoad,
    RetiredSpelling, STATEMENTS, StatementRow,
};
use serde::Serialize;

/// The file a package declares its rows in, beside its descriptor (RFC-0140 D2).
pub const PACKAGE_FILE: &str = "nros-knobs.toml";

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
    ws_root
        .join("build")
        .join("nros")
        .join("knobs")
        .join("catalog.toml")
}

/// A row's kind (RFC-0140 D1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RowKind {
    Knob,
    Statement,
    Env,
}

/// Who declared a row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// Core's `knobs.toml`, compiled into `nros-zephyr-build`.
    Core,
    /// A package's `nros-knobs.toml`. For an in-tree package's compiled rows,
    /// `file` is relative to the checkout.
    Package { package: String, file: PathBuf },
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
            },
            None => Origin::Core,
        }
    }

    fn package(&self) -> Option<&str> {
        match self {
            Origin::Core => None,
            Origin::Package { package, .. } => Some(package),
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
        }
    }

    fn label(&self) -> String {
        let kind = match self.kind {
            RowKind::Knob => "knob",
            RowKind::Statement => "statement",
            RowKind::Env => "env",
        };
        match &self.origin {
            Origin::Core => format!("{kind} `{}` (core knobs.toml)", self.name),
            Origin::Package { package, file } => {
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
}

/// A package file the scan found, with the provider it belongs to.
struct Found<'a> {
    provider: &'a ProviderPackage,
    file: PathBuf,
}

impl Catalog {
    /// Core's rows alone, from the compiled tables.
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
        }
    }

    /// Core plus every scanned provider's `nros-knobs.toml` (RFC-0140 D2: the
    /// file's presence beside the descriptor is the declaration). The one
    /// loader: `nros configure` writes its result, and `nros fix` and the
    /// preflight call it in memory.
    ///
    /// # Errors
    ///
    /// A package file that does not parse or breaks one of RFC-0140 D2's
    /// rules C1 to C4.
    pub fn build(providers: &[ProviderPackage]) -> Result<Self> {
        let mut cat = Catalog::core();

        // Every copy of each package, highest precedence first: a LATER root
        // wins, as in the provider scan (`provider_scan::candidates`).
        let mut seen_dirs = std::collections::BTreeSet::new();
        let mut copies: BTreeMap<&str, Vec<Found<'_>>> = BTreeMap::new();
        for p in providers {
            let file = p.dir.join(PACKAGE_FILE);
            if !seen_dirs.insert(p.dir.clone()) || !file.is_file() {
                continue;
            }
            copies
                .entry(p.package.as_str())
                .or_default()
                .push(Found { provider: p, file });
        }
        for list in copies.values_mut() {
            list.sort_by(|a, b| {
                b.provider
                    .root_index
                    .cmp(&a.provider.root_index)
                    .then(a.provider.dir.cmp(&b.provider.dir))
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
            let text = crate::configure_stamp::read_external(&winner.file)
                .wrap_err_with(|| format!("reading {}", winner.file.display()))?;
            let (owner, rows) = parse_package(winner.provider, &winner.file, &text)?;
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
                winner.provider.root_index
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
                    claims.insert(owner, (package.to_string(), here));
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
            let mut mine = std::collections::BTreeSet::new();
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
    #[must_use]
    pub fn render_toml(&self) -> String {
        #[derive(Serialize)]
        struct FileRow<'a> {
            kind: RowKind,
            name: &'a str,
            origin: String,
            spellings: Vec<String>,
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
                    spellings: r
                        .spellings
                        .iter()
                        .map(|(road, s)| format!("{}:{s}", road.as_str()))
                        .collect(),
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
             # for cmake and Kconfig: `nros` itself builds the same catalog in memory.\n\n{body}"
        )
    }

    /// Write [`Self::render_toml`] to [`catalog_path`], replacing it atomically.
    ///
    /// # Errors
    ///
    /// The file cannot be written.
    pub fn write(&self, ws_root: &Path) -> Result<PathBuf> {
        let path = catalog_path(ws_root);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).wrap_err_with(|| format!("creating {}", dir.display()))?;
        }
        crate::atomic_file::atomic_write(&path, &self.render_toml())
            .wrap_err_with(|| format!("writing {}", path.display()))?;
        Ok(path)
    }
}

/// The owner word RFC-0140 D2 derives for a package: an RMW's or platform's
/// canonical name (the first it announces), a board's key (one of those it
/// announces, the one its `owner` line names). `-` reads as `_`. The kind is
/// the descriptor the file sits beside.
fn derive_owner(p: &ProviderPackage, file: &Path, stated: Option<&str>) -> Result<String> {
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
        .filter(|pr| pr.kind == *kind)
        .map(|pr| pr.name.replace('-', "_"))
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

/// The owner word and rows of one package's `nros-knobs.toml`, with RFC-0140
/// D2's per-file rules: the `owner` line equals the derived word, every key is
/// under it (C2), and a foreign variable is the package's own library's (C4).
fn parse_package(p: &ProviderPackage, file: &Path, text: &str) -> Result<(String, Vec<Row>)> {
    let doc: toml::Table =
        toml::from_str(text).wrap_err_with(|| format!("parsing {}", file.display()))?;
    let where_ = |what: &str| format!("{}: {what}", file.display());
    if let Some(extra) = doc
        .keys()
        .find(|k| !matches!(k.as_str(), "owner" | "knob" | "env"))
    {
        bail!(where_(&format!(
            "`{extra}`: a package file holds `owner`, `[[knob]]` and `[[env]]` rows \
             (statements and [owners] are core's)"
        )));
    }
    let stated = doc.get("owner").and_then(toml::Value::as_str);
    let owner = derive_owner(p, file, stated)?;
    if stated != Some(owner.as_str()) {
        bail!(where_(&format!(
            "`owner = {}`, but package {}'s derived owner word is `{owner}`; state it so the \
             file reads on its own",
            stated.map_or_else(|| "(missing)".to_string(), |s| format!("\"{s}\"")),
            p.package
        )));
    }
    let under = |key: &str| key.split('.').next() == Some(owner.as_str()) && key.contains('.');
    let origin = Origin::Package {
        package: p.package.clone(),
        file: file.to_path_buf(),
    };
    let mut rows = Vec::new();
    for (kind, list) in [("knob", doc.get("knob")), ("env", doc.get("env"))] {
        let Some(list) = list else { continue };
        let list = list
            .as_array()
            .ok_or_else(|| eyre::eyre!(where_(&format!("`{kind}` is an array of tables"))))?;
        for t in list {
            let t = t
                .as_table()
                .ok_or_else(|| eyre::eyre!(where_(&format!("a `{kind}` row is a table"))))?;
            let trans = t.get("transitional").and_then(toml::Value::as_table);
            let tstr = |k: &str| trans.and_then(|tt| tt.get(k)).and_then(toml::Value::as_str);
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
                    .ok_or_else(|| eyre::eyre!(where_("a knob row states `key`")))?;
                if !under(key) {
                    bail!(where_(&format!(
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
                Row {
                    kind: RowKind::Knob,
                    name: key.to_string(),
                    spellings,
                    retired,
                    origin: origin.clone(),
                }
            } else {
                let name = match (t.get("key").and_then(toml::Value::as_str), t.get("name")) {
                    (Some(key), None) => {
                        if !under(key) {
                            bail!(where_(&format!(
                                "key `{key}` is not under this package's owner word \
                                 `{owner}`; a package declares only its own rows"
                            )));
                        }
                        tstr("name")
                            .map(str::to_string)
                            .unwrap_or_else(|| format!("NROS_{}", derive_id(key)))
                    }
                    (None, Some(n)) => {
                        let n = n
                            .as_str()
                            .ok_or_else(|| eyre::eyre!(where_("`name` is a string")))?;
                        let foreign = t.get("foreign").and_then(toml::Value::as_str);
                        if foreign != Some(owner.as_str()) {
                            bail!(where_(&format!(
                                "`{n}`: a package declares only its own library's variables \
                                 (`foreign = \"{owner}\"`); ROS 2's, rcutils' and Rust's \
                                 are core's"
                            )));
                        }
                        n.to_string()
                    }
                    _ => bail!(where_("an env row states `key` or `name`, exactly one")),
                };
                Row {
                    kind: RowKind::Env,
                    spellings: vec![(RetiredRoad::Run, name.clone())],
                    name,
                    retired,
                    origin: origin.clone(),
                }
            };
            rows.push(row);
        }
    }
    Ok((owner, rows))
}

fn parse_retired(
    v: Option<&toml::Value>,
    where_: &dyn Fn(&str) -> String,
) -> Result<Vec<RetiredEntry>> {
    let Some(v) = v else { return Ok(Vec::new()) };
    let list = v.as_array().ok_or_else(|| {
        eyre::eyre!(where_(
            "`retired` is a list of { spelling = \"<road>:<name>\" }"
        ))
    })?;
    let mut out = Vec::new();
    for e in list {
        let t = e.as_table().ok_or_else(|| {
            eyre::eyre!(where_(
                "a retired entry is { spelling = \"<road>:<name>\", ... }"
            ))
        })?;
        let spelling = t
            .get("spelling")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| eyre::eyre!(where_("a retired entry states `spelling`")))?;
        let (road, name) = spelling
            .split_once(':')
            .and_then(|(r, n)| RetiredRoad::parse(r).map(|r| (r, n)))
            .ok_or_else(|| {
                eyre::eyre!(where_(&format!(
                    "`{spelling}` does not start with toml:, env:, kconfig:, cmake: or run:"
                )))
            })?;
        let factor = match t.get("factor") {
            None => 1,
            Some(f) => f
                .as_integer()
                .and_then(|f| u32::try_from(f).ok())
                .filter(|f| *f >= 1)
                .ok_or_else(|| {
                    eyre::eyre!(where_(&format!(
                        "`{spelling}`: `factor` is a positive integer"
                    )))
                })?,
        };
        let instead = t
            .get("instead")
            .and_then(toml::Value::as_str)
            .map(str::to_string);
        if instead.is_some() && t.contains_key("factor") {
            bail!(where_(&format!(
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
fn derive_id(key: &str) -> String {
    key.to_ascii_uppercase().replace('.', "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(dir: &Path, name: &str) -> ProviderPackage {
        rmw_package(dir, name, "acme", 0)
    }

    /// An RMW package announcing `rmw`, with its descriptor, in root `root`.
    fn rmw_package(dir: &Path, name: &str, rmw: &str, root: usize) -> ProviderPackage {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("nros-rmw.toml"), "[rmw]\n").unwrap();
        ProviderPackage {
            package: name.to_string(),
            dir: dir.to_path_buf(),
            root_index: root,
            provides: vec![cargo_nano_ros::package_xml::Provision {
                kind: "rmw".into(),
                name: rmw.into(),
            }],
            depends: Default::default(),
        }
    }

    fn write_rows(dir: &Path, text: &str) -> PathBuf {
        let f = dir.join(PACKAGE_FILE);
        std::fs::write(&f, text).unwrap();
        f
    }

    const ACME: &str = "owner = \"acme\"\n\n[[knob]]\nkey = \"acme.frame_bytes\"\n";

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("repo root")
            .to_path_buf()
    }

    /// RFC-0140 D2 / phase-581 W2: the provider scan of the checkout finds
    /// exactly the in-tree package files the generator rendered, with the
    /// owner word the generator derived, and their rows equal the compiled
    /// ones (so no in-tree package reads as shadowed).
    #[test]
    fn the_scan_finds_the_in_tree_packages_the_generator_rendered() {
        let root = repo_root();
        let roots: Vec<PathBuf> = ["packages/rmw", "packages/platform", "packages/boards"]
            .iter()
            .map(|d| root.join(d))
            .collect();
        let scan = cargo_nano_ros::provider_scan::scan_roots(&roots).unwrap();
        let mut found: Vec<(String, String, String)> = scan
            .providers
            .iter()
            .filter(|p| p.dir.join(PACKAGE_FILE).is_file())
            .map(|p| {
                let file = p.dir.join(PACKAGE_FILE);
                let text = std::fs::read_to_string(&file).unwrap();
                let (owner, _) = parse_package(p, &file, &text).unwrap();
                let rel = file.strip_prefix(&root).unwrap().display().to_string();
                (p.package.clone(), owner, rel)
            })
            .collect();
        found.sort();
        let mut want: Vec<(String, String, String)> = IN_TREE_PACKAGES
            .iter()
            .map(|p| (p.package.into(), p.owner.into(), p.file.into()))
            .collect();
        want.sort();
        assert_eq!(found, want);
        assert_eq!(
            want.len(),
            6,
            "zenoh, xrce, cyclonedds, freertos, threadx, zephyr"
        );
        let cat = Catalog::build(&scan.providers).expect("the in-tree set loads");
        assert!(cat.shadowed().is_empty(), "{:?}", cat.shadowed());
        assert_eq!(cat, Catalog::core());
        // A moved row keeps its spellings and names its package.
        let row = cat
            .rows()
            .iter()
            .find(|r| r.name == "zenoh.max_queryables")
            .unwrap();
        assert!(
            matches!(&row.origin, Origin::Package { package, .. } if package == "nros_rmw_zenoh")
        );
    }

    /// C1: two different packages, one owner word. The message names both files.
    #[test]
    fn two_packages_with_one_owner_word_are_refused_naming_both_files() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("acme_a");
        let b = tmp.path().join("acme_b");
        let pa = rmw_package(&a, "acme_a", "acme", 0);
        let pb = rmw_package(&b, "acme_b", "acme", 1);
        let fa = write_rows(&a, ACME);
        let fb = write_rows(&b, "owner = \"acme\"\n\n[[knob]]\nkey = \"acme.other\"\n");
        let msg = format!("{:#}", Catalog::build(&[pa, pb]).unwrap_err());
        assert!(
            msg.contains("owner word `acme` is claimed by two packages"),
            "{msg}"
        );
        assert!(msg.contains(&fa.display().to_string()), "{msg}");
        assert!(msg.contains(&fb.display().to_string()), "{msg}");
        // An in-tree word is claimed by its package too.
        let z = tmp.path().join("my_zenoh");
        let pz = rmw_package(&z, "my_zenoh", "zenoh", 1);
        write_rows(&z, "owner = \"zenoh\"\n\n[[knob]]\nkey = \"zenoh.mine\"\n");
        let msg = format!("{:#}", Catalog::build(&[pz]).unwrap_err());
        assert!(
            msg.contains("nros_rmw_zenoh (packages/rmw/zenoh/nros-rmw-zenoh/nros-knobs.toml"),
            "{msg}"
        );
    }

    /// C2: a package row under another owner's word, and a package on a core word.
    #[test]
    fn a_package_row_under_another_owner_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let p = provider(tmp.path(), "acme_rmw");
        write_rows(
            tmp.path(),
            "owner = \"acme\"\n\n[[knob]]\nkey = \"zenoh.max_sessions_mine\"\n",
        );
        let msg = format!(
            "{:#}",
            Catalog::build(std::slice::from_ref(&p)).unwrap_err()
        );
        assert!(
            msg.contains(
                "key `zenoh.max_sessions_mine` is not under this package's owner word `acme`"
            ),
            "{msg}"
        );
        write_rows(tmp.path(), "owner = \"acme\"\n\n[[env]]\nkey = \"log.x\"\n");
        let msg = format!(
            "{:#}",
            Catalog::build(std::slice::from_ref(&p)).unwrap_err()
        );
        assert!(msg.contains("key `log.x` is not under"), "{msg}");
        // The `owner` line is checked against the derived word.
        write_rows(tmp.path(), "owner = \"zenoh\"\n");
        let msg = format!("{:#}", Catalog::build(&[p]).unwrap_err());
        assert!(msg.contains("derived owner word is `acme`"), "{msg}");
        // A package whose word is a core owner's.
        let l = tmp.path().join("log_rmw");
        let pl = rmw_package(&l, "log_rmw", "log", 0);
        write_rows(&l, "owner = \"log\"\n");
        let msg = format!("{:#}", Catalog::build(&[pl]).unwrap_err());
        assert!(msg.contains("owner word `log` is a core owner"), "{msg}");
    }

    /// C4: a package declares only its own library's foreign variables.
    #[test]
    fn a_package_declaring_another_projects_variable_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        write_rows(
            tmp.path(),
            "owner = \"acme\"\n\n[[env]]\nname = \"ROS_DOMAIN_ID\"\nforeign = \"ros2\"\n",
        );
        let msg = format!(
            "{:#}",
            Catalog::build(&[provider(tmp.path(), "acme_rmw")]).unwrap_err()
        );
        assert!(msg.contains("its own library's variables"), "{msg}");
    }

    /// C1's exception: the same package in two roots is shadowed, not refused.
    #[test]
    fn the_same_package_in_two_roots_is_shadowed_not_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let under = tmp.path().join("underlay/acme_rmw");
        let over = tmp.path().join("overlay/acme_rmw");
        let pu = rmw_package(&under, "acme_rmw", "acme", 0);
        let po = rmw_package(&over, "acme_rmw", "acme", 1);
        let fu = write_rows(&under, ACME);
        let fo = write_rows(
            &over,
            "owner = \"acme\"\n\n[[knob]]\nkey = \"acme.frame_bytes\"\n\n\
             [[knob]]\nkey = \"acme.overlay_only\"\n",
        );
        let cat = Catalog::build(&[pu, po]).expect("one package in two roots is no collision");
        assert_eq!(
            cat.shadowed(),
            &[Shadowed {
                package: "acme_rmw".into(),
                winner: fo,
                shadowed: vec![fu],
            }]
        );
        // The later root's rows are the ones the catalog holds.
        assert!(cat.rows().iter().any(|r| r.name == "acme.overlay_only"));
        assert_eq!(
            cat.rows()
                .iter()
                .filter(|r| r.name == "acme.frame_bytes")
                .count(),
            1
        );
        // An overlay of an in-tree package shadows its compiled rows.
        let z = tmp.path().join("overlay/nros_rmw_zenoh");
        let pz = rmw_package(&z, "nros_rmw_zenoh", "zenoh", 1);
        let fz = write_rows(
            &z,
            "owner = \"zenoh\"\n\n[[knob]]\nkey = \"zenoh.only_knob\"\n",
        );
        let cat = Catalog::build(&[pz]).expect("an overlay of an in-tree package");
        assert_eq!(cat.shadowed()[0].winner, fz);
        assert_eq!(
            cat.shadowed()[0].shadowed,
            vec![PathBuf::from(
                "packages/rmw/zenoh/nros-rmw-zenoh/nros-knobs.toml"
            )]
        );
        assert!(!cat.rows().iter().any(|r| r.name == "zenoh.max_queryables"));
    }

    #[test]
    fn core_catalog_holds_every_row_kind() {
        let cat = Catalog::build(&[]).expect("the in-tree catalog has one namespace");
        assert_eq!(
            cat.rows().len(),
            KNOBS.len() + STATEMENTS.len() + ENV_ROWS.len()
        );
        assert!(
            cat.rows()
                .iter()
                .any(|r| r.kind == RowKind::Statement && r.name == "domain_id")
        );
        assert!(
            cat.rows()
                .iter()
                .any(|r| r.kind == RowKind::Env && r.name == "ROS_DOMAIN_ID")
        );
        // `NROS_ENTRY_SPIN_MS` is a run-time variable now, not a build knob.
        assert!(
            cat.rows()
                .iter()
                .any(|r| r.kind == RowKind::Env && r.name == "NROS_ENTRY_SPIN_MS")
        );
        assert!(
            !cat.rows()
                .iter()
                .any(|r| r.kind == RowKind::Knob && r.name == "entry.spin_ms")
        );
    }

    #[test]
    fn retired_spelling_answers_every_road_core_holds() {
        let cat = Catalog::core();
        // A build-env rename (one of the 32 phase-484 W8 spellings).
        let (row, e) = cat
            .retired_spelling(RetiredRoad::Env, "ZPICO_MAX_QUERYABLES")
            .expect("a retired build spelling");
        assert_eq!(row.kind, RowKind::Knob);
        assert!(
            row.spellings
                .contains(&(RetiredRoad::Env, "NROS_ZENOH_MAX_QUERYABLES".into()))
        );
        assert_eq!((e.factor, e.is_manual()), (1, false));
        // A run-time alias: the generated `RETIRED_ENV_VARS` view.
        let (row, e) = cat
            .retired_spelling(RetiredRoad::Run, "ZENOH_LOCATOR")
            .expect("a retired run-time spelling");
        assert_eq!(row.name, "NROS_LOCATOR");
        assert!(!e.is_manual());
        // A manual retirement: no replacement name, only text.
        let (row, e) = cat
            .retired_spelling(RetiredRoad::Run, "ZENOH_LISTEN")
            .expect("a manual retirement");
        assert_eq!(row.name, "ZENOH_CONFIG_OVERRIDE");
        assert!(e.is_manual());
        // The retired knob: every spelling refuses, with the row's reason.
        let (_, e) = cat
            .retired_spelling(RetiredRoad::Env, "ZPICO_LEASE_TASK_PRIORITY")
            .expect("the retired lease knob");
        assert!(e.instead.as_deref().is_some_and(|t| t.contains("lease")));
        // The road is part of the question.
        assert!(
            cat.retired_spelling(RetiredRoad::Run, "ZPICO_MAX_QUERYABLES")
                .is_none()
        );
        assert!(
            cat.retired_spelling(RetiredRoad::Env, "NROS_ZENOH_MAX_QUERYABLES")
                .is_none()
        );
    }

    #[test]
    fn retired_env_vars_view_is_the_registry_run_road() {
        let cat = Catalog::core();
        let pairs: Vec<(String, String)> = cat
            .rows()
            .iter()
            .filter(|r| r.kind == RowKind::Env)
            .flat_map(|r| {
                r.retired
                    .iter()
                    .filter(|e| e.road == RetiredRoad::Run && !e.is_manual())
                    .map(move |e| (e.spelling.clone(), r.name.clone()))
            })
            .collect();
        let want: Vec<(String, String)> = [
            ("ZENOH_LOCATOR", "NROS_LOCATOR"),
            ("ZENOH_MODE", "NROS_SESSION_MODE"),
        ]
        .into_iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        assert_eq!(pairs, want);
    }

    #[test]
    fn a_package_row_joins_the_catalog_and_its_retired_spellings() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join(PACKAGE_FILE),
            "owner = \"acme\"\n\n[[knob]]\nkey = \"acme.frame_bytes\"\ntype = \"int\"\n\
             retired = [{ spelling = \"env:NROS_ACME_FRAME_KB\", factor = 1024, from_unit = \"KiB\" }]\n\n\
             [[env]]\nkey = \"acme.trace\"\n",
        )
        .unwrap();
        let cat = Catalog::build(&[provider(tmp.path(), "acme_rmw")]).unwrap();
        let (row, e) = cat
            .retired_spelling(RetiredRoad::Env, "NROS_ACME_FRAME_KB")
            .expect("a package's retired spelling");
        assert_eq!(row.name, "acme.frame_bytes");
        assert!(matches!(&row.origin, Origin::Package { package, .. } if package == "acme_rmw"));
        assert_eq!((e.factor, e.from_unit.as_deref()), (1024, Some("KiB")));
        assert!(
            row.spellings
                .contains(&(RetiredRoad::Kconfig, "CONFIG_NROS_ACME_FRAME_BYTES".into()))
        );
        assert!(
            cat.rows()
                .iter()
                .any(|r| r.kind == RowKind::Env && r.name == "NROS_ACME_TRACE")
        );
        let text = cat.render_toml();
        assert!(text.contains("name = \"acme.frame_bytes\""), "{text}");
        assert!(
            text.contains("spelling = \"env:NROS_ACME_FRAME_KB\""),
            "{text}"
        );
        let back: toml::Table = toml::from_str(&text).expect("catalog.toml parses");
        assert_eq!(back["row"].as_array().unwrap().len(), cat.rows().len());
    }

    #[test]
    fn a_spelling_two_rows_use_refuses_the_catalog_naming_both() {
        let tmp = tempfile::tempdir().unwrap();
        // The package retires a name the zenoh package's max_queryables already retired.
        std::fs::write(
            tmp.path().join(PACKAGE_FILE),
            "owner = \"acme\"\n\n[[knob]]\nkey = \"acme.queryables\"\n\
             retired = [{ spelling = \"env:ZPICO_MAX_QUERYABLES\" }]\n",
        )
        .unwrap();
        let err = Catalog::build(&[provider(tmp.path(), "acme_rmw")]).unwrap_err();
        let msg = format!("{err:#}");
        let old = "ZPICO_MAX_QUERYABLES"; // a retired spelling of core's row
        let want = format!(
            "`{old}` is spelled by knob `zenoh.max_queryables` (package nros_rmw_zenoh, \
             packages/rmw/zenoh/nros-rmw-zenoh/nros-knobs.toml)"
        );
        assert!(msg.contains(&want), "{msg}");
        assert!(msg.contains("package acme_rmw"), "{msg}");
    }

    #[test]
    fn an_entry_with_factor_and_instead_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join(PACKAGE_FILE),
            "owner = \"acme\"\n[[knob]]\nkey = \"acme.x\"\nretired = [{ spelling = \"env:NROS_ACME_Y\", factor = 2, instead = \"no\" }]\n",
        )
        .unwrap();
        let err = Catalog::build(&[provider(tmp.path(), "acme_rmw")]).unwrap_err();
        assert!(
            format!("{err:#}").contains("states both `instead` and `factor`"),
            "{err:#}"
        );
    }

    #[test]
    fn configure_writes_the_catalog_it_builds() {
        let tmp = tempfile::tempdir().unwrap();
        let cat = Catalog::build(&[]).unwrap();
        let path = cat.write(tmp.path()).unwrap();
        assert_eq!(path, catalog_path(tmp.path()));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), cat.render_toml());
    }
}
