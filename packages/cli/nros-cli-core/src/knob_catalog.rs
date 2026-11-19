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
//! Scope of phase-581 W1: no package declares rows yet, so the merged catalog
//! equals core's. Owner derivation and collision rules C1, C2 and C4 are W2's;
//! what is enforced here is RFC-0140 D1's one namespace (C3): a spelling used
//! by two rows refuses the catalog, naming both.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use cargo_nano_ros::provider_scan::ProviderPackage;
use eyre::{Result, WrapErr, bail};
use nros_zephyr_build::{
    ENV_ROWS, EnvRow, KNOBS, KnobRow, RetiredRoad, RetiredSpelling, STATEMENTS, StatementRow,
};
use serde::Serialize;

/// The file a package declares its rows in, beside its descriptor (RFC-0140 D2).
pub const PACKAGE_FILE: &str = "nros-knobs.toml";

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
    /// A package's `nros-knobs.toml`.
    Package { package: String, file: PathBuf },
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
            origin: Origin::Core,
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
            origin: Origin::Core,
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

/// The merged catalog: core rows, then each package's, in scan order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Catalog {
    rows: Vec<Row>,
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
        Catalog { rows }
    }

    /// Core plus every scanned provider's `nros-knobs.toml` (RFC-0140 D2: the
    /// file's presence beside the descriptor is the declaration).
    ///
    /// # Errors
    ///
    /// A package file that does not parse, or a spelling two rows use (C3).
    pub fn build(providers: &[ProviderPackage]) -> Result<Self> {
        let mut cat = Catalog::core();
        let mut seen_dirs = std::collections::BTreeSet::new();
        for p in providers {
            let file = p.dir.join(PACKAGE_FILE);
            if !seen_dirs.insert(p.dir.clone()) || !file.is_file() {
                continue;
            }
            let text = std::fs::read_to_string(&file)
                .wrap_err_with(|| format!("reading {}", file.display()))?;
            cat.rows.extend(parse_package(&p.package, &file, &text)?);
        }
        cat.check_one_namespace()?;
        Ok(cat)
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

/// The rows of one package's `nros-knobs.toml`. Phase-581 W1 reads what the
/// catalog needs (keys, today's spellings and retired entries); W2 adds the
/// owner rules and checks every field.
fn parse_package(package: &str, file: &Path, text: &str) -> Result<Vec<Row>> {
    let doc: toml::Table =
        toml::from_str(text).wrap_err_with(|| format!("parsing {}", file.display()))?;
    let origin = Origin::Package {
        package: package.to_string(),
        file: file.to_path_buf(),
    };
    let where_ = |what: &str| format!("{}: {what}", file.display());
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
            let retired = parse_retired(t.get("retired"), &where_)?;
            let row = if kind == "knob" {
                let key = t
                    .get("key")
                    .and_then(toml::Value::as_str)
                    .ok_or_else(|| eyre::eyre!(where_("a knob row states `key`")))?;
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
                    (Some(key), None) => tstr("name")
                        .map(str::to_string)
                        .unwrap_or_else(|| format!("NROS_{}", derive_id(key))),
                    (None, Some(n)) => n
                        .as_str()
                        .ok_or_else(|| eyre::eyre!(where_("`name` is a string")))?
                        .to_string(),
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
    Ok(rows)
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
        ProviderPackage {
            package: name.to_string(),
            dir: dir.to_path_buf(),
            root_index: 0,
            provides: Vec::new(),
            depends: Default::default(),
        }
    }

    #[test]
    fn core_catalog_holds_every_row_kind() {
        let cat = Catalog::build(&[]).expect("core's catalog has one namespace");
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
        // The package retires a name core's ZENOH_MAX_QUERYABLES already retired.
        std::fs::write(
            tmp.path().join(PACKAGE_FILE),
            "owner = \"acme\"\n\n[[knob]]\nkey = \"acme.queryables\"\n\
             retired = [{ spelling = \"env:ZPICO_MAX_QUERYABLES\" }]\n",
        )
        .unwrap();
        let err = Catalog::build(&[provider(tmp.path(), "acme_rmw")]).unwrap_err();
        let msg = format!("{err:#}");
        let old = "ZPICO_MAX_QUERYABLES"; // a retired spelling of core's row
        let want = format!("`{old}` is spelled by knob `zenoh.max_queryables` (core knobs.toml)");
        assert!(msg.contains(&want), "{msg}");
        assert!(msg.contains("package acme_rmw"), "{msg}");
    }

    #[test]
    fn an_entry_with_factor_and_instead_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join(PACKAGE_FILE),
            "[[knob]]\nkey = \"acme.x\"\nretired = [{ spelling = \"env:NROS_ACME_Y\", factor = 2, instead = \"no\" }]\n",
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
