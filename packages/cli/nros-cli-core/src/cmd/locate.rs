//! `nros locate` — phase-484 W1 (RFC-0103 D4): where a resource is, and why.
//!
//! The front door for every road that cannot link `nros_build_paths` —
//! cmake, `just`, shell. The ladder itself is `nros_build_paths::locate`
//! (one implementation; build scripts call it directly); this verb builds the
//! row from the index model, gathers the context, and prints.
//!
//! `--why` prints every rung and what it saw, so "which FreeRTOS did this
//! use?" is answered by reading, not by inferring.

use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use eyre::{Result, bail};
use nros_build_paths::locate::{self, Ctx, Row};

use crate::orchestration::sdk_index::{SdkIndex, SourcePackage};

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum Format {
    /// The path alone (one name) or `name<TAB>path` lines.
    #[default]
    Path,
    /// `NROS_LOCATE_<NAME>=<path>` lines, shell-quoted.
    Sh,
    /// `set(NROS_LOCATE_<NAME> "<path>")` lines — normal variables, never CACHE.
    Cmake,
}

#[derive(Debug, Parser)]
pub struct Args {
    /// Resources to locate: a `[source.<name>]` of the index, or `store` /
    /// `nano-ros` for the two roots.
    pub names: Vec<String>,

    /// Every `[source.*]` row. A row that resolves nowhere is skipped in
    /// batch output (and named on stderr), never invented.
    #[arg(long)]
    pub all: bool,

    /// Print every rung and what it saw.
    #[arg(long)]
    pub why: bool,

    #[arg(long, value_enum, default_value_t)]
    pub format: Format,

    /// Path to the SDK index.
    #[arg(long, default_value = "nros-sdk-index.toml")]
    pub index: PathBuf,
}

fn row_of(name: &str, s: &SourcePackage) -> Row {
    crate::orchestration::sdk_store::locate_row(name, s)
}

/// One `[source.<name>]` located through the ONE ladder, with `root` as the
/// checkout — the same answer `nros locate <name>` prints. For other verbs
/// (`board-facts`) that need a source's location, so none re-derives it.
pub(crate) fn locate_source(
    index: &SdkIndex,
    root: &std::path::Path,
    name: &str,
) -> std::result::Result<PathBuf, String> {
    let src = index
        .source
        .get(name)
        .ok_or_else(|| format!("no [source.{name}] in the index"))?;
    let row = row_of(name, src);
    let checkout = root
        .join(nros_build_paths::CHECKOUT_MARKER)
        .is_file()
        .then(|| root.to_path_buf());
    let ctx = Ctx {
        arg: None,
        env_value: row
            .env
            .as_deref()
            .and_then(std::env::var_os)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from),
        checkout_edit: locate::checkout_edit(&row, checkout.as_deref()),
        checkout,
        store: crate::orchestration::store::root(),
    };
    locate::resolve(&row, &ctx)
        .map(|a| nros_build_paths::canonical(&a.path))
        .map_err(|r| r.to_string())
}

fn var_name(name: &str) -> String {
    format!(
        "NROS_LOCATE_{}",
        name.to_ascii_uppercase().replace(['-', '.'], "_")
    )
}

pub fn run(args: Args) -> Result<()> {
    if let Some(e) = crate::orchestration::store::retired_in_env() {
        bail!(e);
    }
    let index_path = crate::cmd::setup::resolve_index(&args.index)?;
    let index = SdkIndex::load(&index_path)?;
    // Absolute: a `--why` trace and a cmake `set()` are read from other cwds.
    let checkout = nros_build_paths::canonical(&crate::cmd::setup::index_workspace(&index_path));
    let checkout = checkout
        .join(nros_build_paths::CHECKOUT_MARKER)
        .is_file()
        .then_some(checkout);
    let store = crate::orchestration::store::root();

    let mut names = args.names.clone();
    if args.all {
        names.extend(index.source.keys().cloned());
    }
    if names.is_empty() {
        bail!("name a resource (`nros locate zenoh-pico`), or pass --all");
    }
    let batch = names.len() > 1;

    let mut failed = 0usize;
    for name in &names {
        let answer: Result<(PathBuf, Vec<locate::Step>), String> = match name.as_str() {
            "store" => Ok((
                store.clone(),
                vec![locate::Step {
                    rung: "root",
                    saw: format!(
                        "{} ({})",
                        store.display(),
                        crate::orchestration::store::root_origin()
                    ),
                    chosen: true,
                }],
            )),
            "nano-ros" => match &checkout {
                Some(c) => Ok((
                    c.clone(),
                    vec![locate::Step {
                        rung: "root",
                        saw: format!("{} (the SDK root this index belongs to)", c.display()),
                        chosen: true,
                    }],
                )),
                None => Err("nano-ros: this index belongs to no nano-ros root".into()),
            },
            _ => {
                let Some(src) = index.source.get(name) else {
                    let known: Vec<&str> = index.source.keys().map(String::as_str).collect();
                    bail!(
                        "no `[source.{name}]` in {} — the index has: store, nano-ros, {}",
                        index_path.display(),
                        known.join(", ")
                    );
                };
                let row = row_of(name, src);
                let ctx = Ctx {
                    arg: None,
                    env_value: row
                        .env
                        .as_deref()
                        .and_then(std::env::var_os)
                        .filter(|v| !v.is_empty())
                        .map(PathBuf::from),
                    checkout_edit: locate::checkout_edit(&row, checkout.as_deref()),
                    checkout: checkout.clone(),
                    store: store.clone(),
                };
                match locate::resolve(&row, &ctx) {
                    Ok(a) => Ok((nros_build_paths::canonical(&a.path), a.trace)),
                    Err(r) => Err(r.to_string()),
                }
            }
        };
        match answer {
            Ok((path, trace)) => {
                match args.format {
                    Format::Path if batch => println!("{name}\t{}", path.display()),
                    Format::Path => println!("{}", path.display()),
                    Format::Sh => println!(
                        "{}='{}'",
                        var_name(name),
                        path.display().to_string().replace('\'', "'\\''")
                    ),
                    Format::Cmake => {
                        println!("set({} \"{}\")", var_name(name), path.display());
                        // The row's override NAME as a second key, so a cmake
                        // consumer that only knows `FREERTOS_DIR` asks the same
                        // ladder rather than reading `$ENV{}` raw (phase-484 W2c).
                        if let Some(env) = index.source.get(name).and_then(|s| s.env.as_deref()) {
                            println!("set(NROS_LOCATE_ENV_{env} \"{}\")", path.display());
                        }
                    }
                }
                if args.why {
                    for s in &trace {
                        eprintln!(
                            "  {:<10} {}{}",
                            s.rung,
                            s.saw,
                            if s.chosen { "        <- chosen" } else { "" }
                        );
                    }
                }
            }
            Err(e) => {
                failed += 1;
                eprintln!("{e}");
            }
        }
    }
    // A batch reports what it found and names the rest; a single lookup that
    // missed is a failure — the caller was about to use the path.
    if failed > 0 && (!batch || !args.all) {
        bail!("{failed} resource(s) not located");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::var_name;

    #[test]
    fn variable_names_are_mechanical() {
        assert_eq!(var_name("zenoh-pico"), "NROS_LOCATE_ZENOH_PICO");
        assert_eq!(
            var_name("micro-xrce-dds-client"),
            "NROS_LOCATE_MICRO_XRCE_DDS_CLIENT"
        );
    }
}
