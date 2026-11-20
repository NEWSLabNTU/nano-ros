//! The per-build views of an out-of-tree package's rows (RFC-0140 D5).
//!
//! The committed views (`knob_registry.rs`, `NanoRosKnobRegistry.cmake`, the
//! `zephyr/Kconfig` knob section) hold the in-tree set and are rendered by
//! `scripts/gen-knob-registry.py`. A package outside the tree cannot edit
//! them, so `nros configure` and the Zephyr module hook render its rows into
//! the build instead:
//!
//! - [`registry_cmake`]: `<build>/nros/knobs/registry.cmake`, included right
//!   after the committed file;
//! - [`kconfig_packages`]: `<build>/nros/Kconfig.packages`, `osource`d by the
//!   module's `Kconfig`;
//! - [`env_header`][]: `<build>/nros/knobs/<owner>_env.h`;
//! - [`kconfig_rows`]: a package's own `Kconfig`, for a package that is its
//!   own Zephyr module (`nros knob render --kconfig`, the guest case).

use std::collections::BTreeMap;

use crate::{Catalog, Row, RowKind};

/// The typed knob rows of `owners` that the compiled tables do not hold, in
/// catalog order.
fn package_knobs<'c>(cat: &'c Catalog, owners: &[&str]) -> Vec<&'c Row> {
    cat.package_rows()
        .filter(|r| r.kind == RowKind::Knob && r.meta.ty.is_some())
        .filter(|r| owners.contains(&r.owner()))
        .collect()
}

fn symbol(r: &Row) -> String {
    let s = r
        .spelling(nros_zephyr_build::RetiredRoad::Kconfig)
        .unwrap_or_default();
    s.strip_prefix("CONFIG_").unwrap_or(s).to_string()
}

fn id(r: &Row) -> String {
    let env = r
        .spelling(nros_zephyr_build::RetiredRoad::Env)
        .unwrap_or_default();
    env.strip_prefix("NROS_").unwrap_or(env).to_string()
}

/// Kconfig definitions of `rows`, grouped under each row's `kconfig_if`.
///
/// `stated` maps a symbol (without `CONFIG_`) to the image's value; it is
/// written as the first `default`, so it wins over the row's own. With
/// `prompts`, each definition carries its prompt (a guest's own module, so its
/// `prj.conf` sets the knob); without, it is prompt-less like every module knob,
/// so `system.toml` is the one place a value comes from.
#[must_use]
pub fn kconfig_rows(rows: &[&Row], stated: &BTreeMap<String, String>, prompts: bool) -> String {
    let mut conds: Vec<Option<&str>> = vec![None];
    let mut named: Vec<&str> = rows
        .iter()
        .filter_map(|r| r.meta.kconfig_if.as_deref())
        .collect();
    named.sort_unstable();
    named.dedup();
    conds.extend(named.into_iter().map(Some));
    let mut s = String::new();
    for cond in conds {
        let group: Vec<_> = rows
            .iter()
            .filter(|r| r.meta.kconfig_if.as_deref() == cond)
            .collect();
        if group.is_empty() {
            continue;
        }
        if let Some(c) = cond {
            s.push_str(&format!("\nif {c}\n"));
        }
        for r in group {
            let sym = symbol(r);
            let ty = r.meta.kconfig_type();
            s.push('\n');
            if let Some(p) = &r.meta.prompt
                && !prompts
            {
                s.push_str(&format!("# prompt: {p}\n"));
            }
            s.push_str(&format!("# [image.<id>.knobs] {}\n", r.name));
            s.push_str(&format!("config {sym}\n"));
            if prompts {
                let prompt = r
                    .meta
                    .prompt
                    .clone()
                    .unwrap_or_else(|| r.name.replace(['.', '_'], " "));
                s.push_str(&format!("    {ty} \"{}\"\n", prompt.replace('"', "\\\"")));
            } else {
                s.push_str(&format!("    {ty}\n"));
            }
            if let Some(v) = stated.get(&sym) {
                s.push_str(&format!("    default {v}\n"));
            }
            for a in &r.meta.kconfig_attrs {
                s.push_str(&format!("    {a}\n"));
            }
            if let Some(h) = &r.meta.help {
                s.push_str("    help\n");
                for l in h.lines() {
                    if l.is_empty() {
                        s.push('\n');
                    } else {
                        s.push_str(&format!("      {l}\n"));
                    }
                }
            }
            // A choice knob's member bools: Kconfig cannot test a string for a
            // member, so a member that must select options gets its own bool,
            // which the module refuses when it disagrees with the string.
            for (m, depends, select) in &r.meta.member_kconfig {
                let msym = member_symbol(&sym, m);
                s.push_str(&format!("\nconfig {msym}\n"));
                if prompts {
                    s.push_str(&format!(
                        "    bool \"{sym} names {m} (must agree with it)\"\n"
                    ));
                } else {
                    s.push_str("    bool\n");
                }
                if let Some(v) = stated.get(&sym)
                    && choice_names(v, m)
                {
                    s.push_str("    default y\n");
                }
                for d in depends {
                    s.push_str(&format!("    depends on {d}\n"));
                }
                for x in select {
                    s.push_str(&format!("    select {x}\n"));
                }
            }
        }
        if let Some(c) = cond {
            s.push_str(&format!("\nendif # {c}\n"));
        }
    }
    s
}

/// A choice knob member's derived bool: `<SYMBOL>_HAS_<MEMBER>` (`-` read as
/// `_`), the generator's spelling for core rows too.
fn member_symbol(sym: &str, member: &str) -> String {
    format!(
        "{sym}_HAS_{}",
        member.to_ascii_uppercase().replace('-', "_")
    )
}

/// Whether a stated choice value (a Kconfig string, quotes allowed; members
/// comma-joined) names `member`.
fn choice_names(value: &str, member: &str) -> bool {
    value
        .trim()
        .trim_matches('"')
        .split(',')
        .any(|x| x.trim() == member)
}

/// `<build>/nros/Kconfig.packages` (RFC-0140 D5): the prompt-less definition
/// of every typed knob of the packages this image links (`owners`), with the
/// image's value as a `default` (`stated`, keyed by symbol without
/// `CONFIG_`). Sourced by the module's `Kconfig`, so `<build>/zephyr/.config`
/// carries the value and the build-script ladder's `$DOTCONFIG` rung reads it.
#[must_use]
pub fn kconfig_packages(
    cat: &Catalog,
    owners: &[&str],
    stated: &BTreeMap<String, String>,
) -> String {
    let rows = package_knobs(cat, owners);
    let mut s = String::from(
        "# GENERATED by nros (`dev image leaf-system --kconfig-packages-out`) -- do not\n\
         # edit. Re-rendered on every configure.\n\
         #\n\
         # The knobs of the out-of-tree packages this image links. Like every module\n\
         # knob they have no prompt: state one as `[image.<id>.knobs.<owner>] <name>`.\n",
    );
    if rows.is_empty() {
        s.push_str("#\n# This image links no out-of-tree package that declares a knob.\n");
    }
    s.push_str(&kconfig_rows(&rows, stated, false));
    s
}

/// `<build>/nros/knobs/registry.cmake` (RFC-0140 D5): the out-of-tree typed
/// knobs, in the committed `NanoRosKnobRegistry.cmake`'s variables, so cmake's
/// override and retired-name handling reaches them. Included right after it.
#[must_use]
pub fn registry_cmake(cat: &Catalog) -> String {
    let mut out = String::from(
        "# @generated by `nros configure` from the configuration catalog -- do not edit.\n\
         # The knobs of out-of-tree packages, in the variables of the\n\
         # committed cmake/NanoRosKnobRegistry.cmake, which this file extends.\n\n",
    );
    let typed: Vec<&Row> = cat
        .package_rows()
        .filter(|r| r.kind == RowKind::Knob && r.meta.ty.is_some())
        .collect();
    for r in &typed {
        let id = id(r);
        let (table, name) = r.name.rsplit_once('.').unwrap_or(("", &r.name));
        out.push_str(&format!("list(APPEND NROS_KNOB_KCONFIG_IDS {id})\n"));
        out.push_str(&format!(
            "set(NROS_KNOB_KCONFIG_SYMBOL_{id} {})\n",
            symbol(r)
        ));
        out.push_str(&format!("set(NROS_KNOB_IMAGE_TABLE_{id} {table})\n"));
        out.push_str(&format!("set(NROS_KNOB_IMAGE_NAME_{id} {name})\n"));
        if r.meta.is_choice() {
            // The committed registry's CHOICE tables, so the cmake road refuses
            // a package knob's out-of-set value as it does a core one's.
            let kind = r.meta.ty.as_deref().unwrap_or_default();
            let members = r.meta.values.join(", ");
            let want = if kind == "enum" {
                format!("one of: {members}")
            } else {
                format!(
                    "an ordered list of distinct members, comma-separated, at least one, of: \
                     {members}"
                )
            };
            let sym = symbol(r);
            let member_syms: Vec<String> = r
                .meta
                .member_kconfig
                .iter()
                .map(|(m, _, _)| format!("{m}={}", member_symbol(&sym, m)))
                .collect();
            out.push_str(&format!("list(APPEND NROS_KNOB_CHOICE_IDS {id})\n"));
            out.push_str(&format!("set(NROS_KNOB_CHOICE_KIND_{id} {kind})\n"));
            out.push_str(&format!(
                "set(NROS_KNOB_CHOICES_{id} \"{}\")\n",
                r.meta.values.join(";")
            ));
            out.push_str(&format!("set(NROS_KNOB_KEY_{id} {})\n", r.name));
            out.push_str(&format!("set(NROS_KNOB_CHOICE_SYMBOL_{id} {sym})\n"));
            out.push_str(&format!("set(NROS_KNOB_CHOICE_WANT_{id} \"{want}\")\n"));
            out.push_str(&format!(
                "set(NROS_KNOB_MEMBER_SYMBOLS_{id} \"{}\")\n",
                member_syms.join(";")
            ));
        }
    }
    for r in cat.package_rows().filter(|r| r.kind == RowKind::Knob) {
        let env = format!("NROS_{}", id(r));
        let legacy: Vec<&str> = r
            .retired
            .iter()
            .filter(|e| e.road == nros_zephyr_build::RetiredRoad::Env)
            .map(|e| e.spelling.as_str())
            .collect();
        if legacy.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "set(NROS_KNOB_LEGACY_{env} \"{}\")\n",
            legacy.join(";")
        ));
        for l in legacy {
            out.push_str(&format!("set(NROS_KNOB_CANONICAL_{l} {env})\n"));
        }
    }
    out
}

/// `<build>/nros/knobs/<owner>_env.h` (RFC-0140 D5): the names of `owner`'s
/// env rows, for a package's C code to read through rather than spell.
#[must_use]
pub fn env_header(cat: &Catalog, owner: &str) -> String {
    let guard = format!("NROS_KNOBS_{}_ENV_H", owner.to_ascii_uppercase());
    let mut s = format!(
        "/* @generated by `nros configure` from the configuration catalog -- do not edit.\n \
         * The run-time variables package owner `{owner}` declares. */\n\
         #ifndef {guard}\n#define {guard}\n\n"
    );
    for r in cat
        .package_rows()
        .filter(|r| r.kind == RowKind::Env && r.owner() == owner)
    {
        let macro_ = format!(
            "NROS_ENV_{}",
            r.name.to_ascii_uppercase().replace(['<', '>'], "")
        );
        if let Some(d) = &r.meta.disposition {
            s.push_str(&format!("/* disposition: {d} */\n"));
        }
        s.push_str(&format!("#define {macro_} \"{}\"\n", r.name));
    }
    s.push_str(&format!("\n#endif /* {guard} */\n"));
    s
}
