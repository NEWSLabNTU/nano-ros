//! `nros dev image entity-facts` — the entity facts an image's archive build
//! hands cargo, as `KEY=VALUE` lines (phase-550 W6b, RFC-0126 D2).
//!
//! ONE producer for every road. The CMake roads (`nros_entity_facts_env`,
//! cmake/NanoRosEntityFacts.cmake) and the NuttX make road both call this,
//! so the rows a build script sees are composed once. The composition lived
//! only in CMake, where a road with no CMake configure could not reach it.
//!
//! Inputs, both READ and never re-derived:
//!
//! * `--inventory`: the image's entity inventory, a CMake fragment the CLI
//!   wrote: a CMake configure's `<build>/nros/entity_inventory.cmake`, or
//!   `nros configure`'s `resolved.cmake`, which is that fragment's seed and
//!   carries the same variables.
//! * `--sizing-descriptor`: the descriptor to name to cargo, chosen by the
//!   caller. On a CMake road the choice depends on which entries registered
//!   during the configure, which only that configure knows; the make road
//!   builds one image with one descriptor.
//!
//! Rows, in the order the CMake composition emitted them:
//!
//! 1. `NROS_ENTITY_DECLARED_DEPTHS=<a,b,…>`, the per-endpoint depth table, when
//!    the inventory resolved it and it is non-empty;
//! 2. `NROS_SIZING_DESCRIPTOR=<path>`;
//! 3. `NROS_DECLARED_MAX_{STRING_VALUE_LEN,ARRAY_LEN,BYTE_ARRAY_LEN}`, the
//!    parameter store, when the contract declared `params:`. A REFUSED
//!    declaration stops the build rather than size the store from defaults.

use std::path::{Path, PathBuf};

use clap::Args as ClapArgs;
use eyre::Result;

#[derive(Debug, ClapArgs)]
pub struct EntityFactsArgs {
    /// The image's entity inventory: a CMake fragment the CLI wrote (a CMake
    /// configure's `nros/entity_inventory.cmake`, or `nros configure`'s
    /// `resolved.cmake`). A path that is missing or unreadable is refused.
    #[arg(long)]
    pub inventory: Option<PathBuf>,

    /// The sizing descriptor to name to cargo.
    #[arg(long)]
    pub sizing_descriptor: Option<PathBuf>,
}

/// The parameter-store rows: the cargo name each derived value travels as.
const PARAM_ROWS: [(&str, &str); 3] = [
    (
        "NROS_DECLARED_MAX_STRING_VALUE_LEN",
        "NROS_DERIVED_MAX_STRING_VALUE_LEN",
    ),
    ("NROS_DECLARED_MAX_ARRAY_LEN", "NROS_DERIVED_MAX_ARRAY_LEN"),
    (
        "NROS_DECLARED_MAX_BYTE_ARRAY_LEN",
        "NROS_DERIVED_MAX_BYTE_ARRAY_LEN",
    ),
];

/// The rows, or why there are none to give.
pub fn rows(inventory: Option<&Path>, sizing: Option<&Path>) -> Result<Vec<String>, String> {
    let vars = match inventory {
        Some(p) => {
            let text = std::fs::read_to_string(p).map_err(|e| {
                format!(
                    "the entity inventory {} cannot be read ({e}). Run `nros configure` for \
                     the image, or name the inventory its configure wrote.",
                    p.display()
                )
            })?;
            Some((p, crate::message_bound_knobs::cmake_set_values(&text)))
        }
        None => None,
    };
    let get = |k: &str| {
        vars.as_ref()
            .and_then(|(_, v)| v.get(k).map(String::as_str))
    };
    let mut out = Vec::new();

    if get("NROS_ENTITY_DECLARED_DEPTH_STATUS") == Some("resolved")
        && let Some(table) = get("NROS_ENTITY_DECLARED_DEPTHS")
    {
        let table = table.replace(';', ",");
        if !table.is_empty() {
            out.push(format!("NROS_ENTITY_DECLARED_DEPTHS={table}"));
        }
    }

    if let Some(d) = sizing {
        out.push(format!("NROS_SIZING_DESCRIPTOR={}", d.display()));
    }

    match get("NROS_PARAM_DECLARATION_STATUS") {
        Some("refused") => {
            let path = vars
                .as_ref()
                .map(|(p, _)| p.display().to_string())
                .unwrap_or_default();
            return Err(format!(
                "the parameter store cannot be sized: the entity inventory REFUSED the \
                 contract's `params:` declaration (NROS_PARAM_DECLARATION_STATUS \"refused\" in \
                 {path}):\n  {}\nDeclare `params:` on every node in the image, or on none. The \
                 build stops here rather than size the store from the crate defaults.",
                get("NROS_PARAM_DECLARATION_REASON").unwrap_or("")
            ));
        }
        Some("declared") => {
            for (name, src) in PARAM_ROWS {
                if let Some(v) = get(src) {
                    out.push(format!("{name}={v}"));
                }
            }
        }
        _ => {}
    }
    Ok(out)
}

pub fn run(args: EntityFactsArgs) -> Result<()> {
    let rows = rows(args.inventory.as_deref(), args.sizing_descriptor.as_deref())
        .map_err(|e| eyre::eyre!(e))?;
    for r in rows {
        println!("{r}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::rows;
    use std::path::Path;

    fn inv(body: &str) -> tempfile::NamedTempFile {
        let f = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(f.path(), body).unwrap();
        f
    }

    #[test]
    fn rows_come_in_the_cmake_order() {
        let f = inv("set(NROS_ENTITY_DECLARED_DEPTH_STATUS \"resolved\")\n\
             set(NROS_ENTITY_DECLARED_DEPTHS \"a=1;b=2\")\n\
             set(NROS_PARAM_DECLARATION_STATUS \"declared\")\n\
             set(NROS_DERIVED_MAX_STRING_VALUE_LEN 64)\n\
             set(NROS_DERIVED_MAX_BYTE_ARRAY_LEN 8)\n");
        let got = rows(Some(f.path()), Some(Path::new("/d/x.toml"))).unwrap();
        assert_eq!(
            got,
            [
                "NROS_ENTITY_DECLARED_DEPTHS=a=1,b=2",
                "NROS_SIZING_DESCRIPTOR=/d/x.toml",
                "NROS_DECLARED_MAX_STRING_VALUE_LEN=64",
                "NROS_DECLARED_MAX_BYTE_ARRAY_LEN=8",
            ]
        );
    }

    #[test]
    fn an_unresolved_or_empty_table_and_absent_params_give_no_rows() {
        let f = inv("set(NROS_ENTITY_DECLARED_DEPTH_STATUS \"resolved\")\n\
             set(NROS_ENTITY_DECLARED_DEPTHS \"\")\n\
             set(NROS_PARAM_DECLARATION_STATUS \"absent\")\n");
        assert!(rows(Some(f.path()), None).unwrap().is_empty());
        let g = inv(
            "set(NROS_ENTITY_DECLARED_DEPTH_STATUS \"refused\")\nset(NROS_ENTITY_DECLARED_DEPTHS \"a=1\")\n",
        );
        assert!(rows(Some(g.path()), None).unwrap().is_empty());
    }

    #[test]
    fn a_missing_inventory_is_refused_never_empty() {
        let e = rows(Some(Path::new("/nonexistent/entity_inventory.cmake")), None).unwrap_err();
        assert!(e.contains("/nonexistent/entity_inventory.cmake"), "{e}");
    }

    #[test]
    fn a_refused_param_declaration_stops_the_build() {
        let f = inv("set(NROS_PARAM_DECLARATION_STATUS \"refused\")\n\
             set(NROS_PARAM_DECLARATION_REASON \"node b declares none\")\n");
        let e = rows(Some(f.path()), None).unwrap_err();
        assert!(
            e.contains("node b declares none") && e.contains("params:"),
            "{e}"
        );
        let id = regex::Regex::new(r"(?i)\b(issue|phase|rfc)[ -]?\d").unwrap();
        assert!(!id.is_match(&e), "{e}");
    }
}
