//! The naming rule (RFC-0140 D3 N1-N8), enforced when the catalog loads
//! (phase-581 W4, RFC-0140 D13 G1: "the lint is the loader").
//!
//! The rule's data and its baseline are `config-names.toml`, beside this
//! crate's manifest, compiled in. The gate `just check config-names`
//! (`scripts/lib/config_names.py`) reads the same file and runs the same
//! rules, buildless, over every in-tree contributor; the file's `[[case]]`
//! corpus is run by both implementations (`naming_cases_*` in this crate's
//! tests, the gate's self-test), which is what holds the two together.
//!
//! A violation fails the load unless the baseline holds that row and rule: the
//! in-tree rows RFC-0140 Appendix A lists, which the rename stages remove. A
//! downstream package's row is never in it, so a bad name there fails in its
//! user's build.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
};

use nros_zephyr_build::{CORE_OWNERS, RESOURCE_ENV_VARS, RETIRING_OWNERS};

use crate::{Row, RowKind, derive_id};

/// The data file, compiled in.
pub const DATA: &str = include_str!("../config-names.toml");

/// The rules, in order.
pub const RULES: [&str; 8] = ["N1", "N2", "N3", "N4", "N5", "N6", "N7", "N8"];

/// `config-names.toml`, parsed.
#[derive(Debug, Clone, Default)]
pub struct Data {
    /// Last words that measure something, so a unit must follow them (N4).
    pub measure: BTreeSet<String>,
    /// Banned last words and why (N4).
    pub banned: BTreeMap<String, String>,
    /// Terms whose unit the term fixes, and why (N4).
    pub allow: BTreeMap<String, String>,
    /// Hand Kconfig symbols that are framework, not settings, and why.
    pub framework: BTreeMap<String, String>,
    /// `(kind, subject)` -> the rules the baseline holds for it. Only the
    /// entries the lint can see: a probe entry exempts nothing.
    pub baseline: BTreeMap<(String, String), BTreeSet<String>>,
}

fn strings(v: Option<&toml::Value>) -> impl Iterator<Item = String> + '_ {
    v.and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_str)
        .map(str::to_string)
}

fn table_rows<'a>(
    v: Option<&'a toml::Value>,
    k: &'a str,
) -> impl Iterator<Item = (String, String)> + 'a {
    v.and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(move |t| {
            let s = |f: &str| t.get(f).and_then(toml::Value::as_str).map(str::to_string);
            Some((s(k)?, s("reason")?))
        })
}

impl Data {
    /// Parse a data file's text.
    ///
    /// # Panics
    ///
    /// On text that is not TOML: the compiled-in file is held parseable by
    /// this crate's tests and the gate.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let doc: toml::Table = toml::from_str(text).expect("config-names.toml parses");
        let units = doc.get("units");
        let mut baseline: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
        for e in doc
            .get("baseline")
            .and_then(toml::Value::as_array)
            .into_iter()
            .flatten()
        {
            let s = |f: &str| e.get(f).and_then(toml::Value::as_str).unwrap_or_default();
            if e.get("probe").is_some() || !matches!(s("kind"), "knob" | "statement" | "env") {
                continue;
            }
            baseline
                .entry((s("kind").to_string(), s("subject").to_string()))
                .or_default()
                .extend(strings(e.get("rules")));
        }
        Data {
            measure: strings(units.and_then(|u| u.get("measure"))).collect(),
            banned: table_rows(units.and_then(|u| u.get("banned")), "ending").collect(),
            allow: table_rows(units.and_then(|u| u.get("allow")), "term").collect(),
            framework: table_rows(doc.get("framework"), "symbol").collect(),
            baseline,
        }
    }

    /// The compiled-in data.
    pub fn get() -> &'static Data {
        static DATA_PARSED: OnceLock<Data> = OnceLock::new();
        DATA_PARSED.get_or_init(|| Data::parse(DATA))
    }

    /// Does the baseline hold `rule` for this subject?
    #[must_use]
    pub fn holds(&self, kind: &str, subject: &str, rule: &str) -> bool {
        self.baseline
            .get(&(kind.to_string(), subject.to_string()))
            .is_some_and(|r| r.contains(rule))
    }

    fn unit_problem(&self, name: &str) -> Option<String> {
        let lower = name.to_ascii_lowercase();
        let last = lower.rsplit('_').next().unwrap_or(&lower);
        if self.allow.contains_key(last) {
            return None;
        }
        if let Some(why) = self.banned.get(last) {
            return Some(format!("ends `_{last}`: {why}"));
        }
        self.measure
            .contains(last)
            .then(|| format!("ends `_{last}`, which measures something, with no unit after it"))
    }
}

/// What a subject is judged against.
#[derive(Debug, Clone, Default)]
pub struct Context {
    /// Every owner word: core's and each package's.
    pub owners: BTreeSet<String>,
    /// Every env row's variable (N5).
    pub env_names: BTreeSet<String>,
    /// Every retired spelling (N7).
    pub retired: BTreeSet<String>,
    /// Every resource variable (N8).
    pub resources: BTreeSet<String>,
}

impl Context {
    /// The context of a set of rows: core's owner words (less the ones a
    /// rename retires), the package words in `package_owners`, and the rows'
    /// env names and retired spellings.
    pub fn of<'a>(
        rows: impl IntoIterator<Item = &'a Row>,
        package_owners: impl IntoIterator<Item = String>,
    ) -> Self {
        let mut ctx = Context {
            owners: CORE_OWNERS
                .iter()
                .filter(|w| !RETIRING_OWNERS.contains(w))
                .map(|w| (*w).to_string())
                .chain(package_owners)
                .collect(),
            resources: RESOURCE_ENV_VARS.iter().map(|s| (*s).to_string()).collect(),
            ..Context::default()
        };
        for r in rows {
            if r.kind == RowKind::Env {
                ctx.env_names.insert(r.name.clone());
            }
            ctx.retired
                .extend(r.retired.iter().map(|e| e.spelling.clone()));
        }
        ctx
    }
}

/// One thing the rule judges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subject {
    /// `knob`, `statement`, `env` or `kconfig` (a hand Kconfig symbol).
    pub kind: String,
    /// A knob's or statement's key, an env row's variable, a Kconfig symbol.
    pub subject: String,
    /// A knob's or keyed env row's key.
    pub key: Option<String>,
    /// Every live spelling, by road (`toml`, `env`, `kconfig`, `cmake`, `run`).
    pub spellings: Vec<(String, String)>,
    /// The variable is another project's.
    pub foreign: bool,
}

impl Subject {
    /// A catalog row as a subject.
    #[must_use]
    pub fn of_row(r: &Row) -> Self {
        Subject {
            kind: r.kind.as_str().to_string(),
            subject: r.name.clone(),
            key: match r.kind {
                RowKind::Env => r.meta.key.clone(),
                _ => Some(r.name.clone()),
            },
            spellings: r
                .spellings
                .iter()
                .map(|(road, s)| (road.as_str().to_string(), s.clone()))
                .collect(),
            foreign: r.meta.foreign.is_some(),
        }
    }
}

fn is_word(s: &str) -> bool {
    let mut c = s.chars();
    c.next().is_some_and(|f| f.is_ascii_lowercase())
        && c.all(|x| x.is_ascii_lowercase() || x.is_ascii_digit() || x == '_')
}

/// `[(rule, why)]` for one subject, at most one per rule, in rule order.
#[must_use]
pub fn violations(s: &Subject, ctx: &Context, data: &Data) -> Vec<(&'static str, String)> {
    let mut out: BTreeMap<&'static str, String> = BTreeMap::new();
    let mut add = |rule: &'static str, why: String| {
        out.entry(rule).or_insert(why);
    };
    let mut spellings = s.spellings.clone();
    let want_mismatch =
        |want: &BTreeMap<&str, String>, key: &str, add: &mut dyn FnMut(&'static str, String)| {
            for (road, v) in &s.spellings {
                if let Some(w) = want.get(road.as_str())
                    && v != w
                {
                    add(
                        "N2",
                        format!("{road} spelling `{v}` is not `{w}`, the one `{key}` derives"),
                    );
                }
            }
        };
    match (s.kind.as_str(), s.key.as_deref()) {
        ("kconfig", _) => {
            let sym = s.subject.as_str();
            if data.framework.contains_key(sym) {
                return Vec::new();
            }
            match sym.strip_prefix("CONFIG_NROS_") {
                None => add("N6", format!("`{sym}` does not start CONFIG_NROS_")),
                Some(rest) => {
                    let owned = ctx.owners.iter().any(|w| {
                        let u = w.to_ascii_uppercase();
                        rest == u || rest.starts_with(&format!("{u}_"))
                    });
                    if !owned {
                        add(
                            "N3",
                            format!("`{sym}` does not start with an owner word after CONFIG_NROS_"),
                        );
                    }
                    if let Some(p) = data.unit_problem(rest) {
                        add("N4", format!("`{sym}` {p}"));
                    }
                }
            }
            spellings = vec![("kconfig".into(), sym.to_string())];
        }
        ("statement", Some(key)) => {
            if !is_word(key) {
                add(
                    "N1",
                    format!("statement key `{key}` is one lower-case word"),
                );
            }
            let up = key.to_ascii_uppercase();
            let want = BTreeMap::from([
                ("toml", key.to_string()),
                ("env", format!("NROS_ENTRY_{up}")),
                ("kconfig", format!("CONFIG_NROS_{up}")),
            ]);
            want_mismatch(&want, key, &mut add);
        }
        (kind, Some(key)) if kind == "knob" || key.contains('.') => {
            let segs: Vec<&str> = key.split('.').collect();
            if segs.len() != 2 || !segs.iter().all(|x| is_word(x)) {
                add(
                    "N1",
                    format!("key `{key}` is not `<owner>.<name>` (two lower snake case segments)"),
                );
            }
            if !ctx.owners.contains(segs[0]) {
                add(
                    "N3",
                    format!("key `{key}`: `{}` is not an owner word", segs[0]),
                );
            }
            if let Some(p) = data.unit_problem(segs.last().unwrap_or(&key)) {
                add("N4", format!("key `{key}` {p}"));
            }
            let id = derive_id(key);
            let want = BTreeMap::from([
                ("toml", key.to_string()),
                ("env", format!("NROS_{id}")),
                ("kconfig", format!("CONFIG_NROS_{id}")),
                ("cmake", format!("NROS_OVERRIDE_{id}")),
                ("run", format!("NROS_{id}")),
            ]);
            want_mismatch(&want, key, &mut add);
        }
        ("env", Some(key)) => {
            let want = format!("NROS_{}", key.to_ascii_uppercase());
            if s.subject != want {
                add(
                    "N2",
                    format!(
                        "`{}` is not `{want}`, statement `{key}`'s run-time override",
                        s.subject
                    ),
                );
            }
        }
        _ => {}
    }

    let names: Vec<&str> = spellings
        .iter()
        .filter(|(road, _)| road != "toml")
        .map(|(_, v)| v.as_str())
        .collect();
    if matches!(s.kind.as_str(), "knob" | "statement") {
        for v in &names {
            if ctx.env_names.contains(*v) {
                add(
                    "N5",
                    format!("build spelling `{v}` is also the name of an env row"),
                );
            }
        }
    }
    if s.foreign {
        if s.subject.starts_with("NROS_") {
            add(
                "N6",
                format!(
                    "`{}` is foreign but starts NROS_, which is nano-ros's prefix",
                    s.subject
                ),
            );
        }
    } else {
        for v in &names {
            if !(v.starts_with("NROS_") || v.starts_with("CONFIG_NROS_")) {
                add(
                    "N6",
                    format!("`{v}` does not start NROS_ (CONFIG_NROS_ in Kconfig)"),
                );
            }
        }
    }
    for (_, v) in &spellings {
        if ctx.retired.contains(v) {
            add(
                "N7",
                format!("`{v}` is a retired spelling, kept for good, so never reused"),
            );
        }
        if ctx.resources.contains(v) {
            add(
                "N8",
                format!("`{v}` is a resource location's variable (nros-sdk-index.toml)"),
            );
        }
    }
    RULES
        .iter()
        .filter_map(|r| out.remove(r).map(|why| (*r, why)))
        .collect()
}

/// Whether the rule judges `r`'s names. A `class = "retired"` row names a
/// knob that no longer exists: its key is kept only as the error table
/// (RFC-0140 D4), so it is no live name and is not judged (its spellings stay
/// reserved through N7).
#[must_use]
pub fn judged(r: &Row) -> bool {
    r.meta.class.as_deref() != Some("retired")
}

/// Every violation of `rows` the baseline does not hold, as one message line
/// each (`<row>: <rule> - <why>`).
pub fn unheld<'a>(rows: impl IntoIterator<Item = &'a Row>, ctx: &Context) -> Vec<String> {
    let data = Data::get();
    let mut out = Vec::new();
    for r in rows.into_iter().filter(|r| judged(r)) {
        let s = Subject::of_row(r);
        for (rule, why) in violations(&s, ctx, data) {
            if !data.holds(&s.kind, &s.subject, rule) {
                out.push(format!("{}: {rule} - {why}", r.label()));
            }
        }
    }
    out
}

/// The refusal for [`unheld`]'s lines (the rule is RFC-0140 D3; each line
/// names its rule, N1 to N8, and says what is wrong).
#[must_use]
pub fn refusal(lines: &[String]) -> String {
    format!(
        "a configuration name breaks the naming rule (a key is `<owner>.<name>`, every other \
         spelling derives from it, and the name carries its unit); rename it:\n  {}",
        lines.join("\n  ")
    )
}
