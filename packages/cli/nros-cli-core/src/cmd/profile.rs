//! `nros profile` — phase-336: the cargo build-profile table, as a verb.
//!
//! The table itself lives in `nros-cargo-profile` so Rust callers link it
//! directly; this is the bridge for the callers that cannot — cmake
//! (`execute_process`), bash (`scripts/build/cargo.sh`), and just. Before this
//! verb existed, the `profile → target subdirectory` derivation alone was
//! spelled three times and `/release/` was hardcoded in the FFI artifact path.
//!
//! Verbs:
//!
//! ```text
//! nros profile resolve --build-type RelWithDebInfo   # -> nros-relwithdebinfo
//! nros profile args    nros-minsizerel               # -> --profile nros-minsizerel
//! nros profile dir     nros-minsizerel               # -> nros-minsizerel
//! nros profile env     nros-minsizerel               # -> CARGO_PROFILE_*=... lines
//! ```
//!
//! `env` prints nothing for a profile nano-ros does not own — see the ownership
//! rule in `nros_cargo_profile`.

use clap::{Args as ClapArgs, Parser, Subcommand};
use eyre::Result;

#[derive(Debug, Parser)]
pub struct Args {
    #[command(subcommand)]
    pub verb: Verb,
}

/// `nros dev profile`'s verbs: the plumbing cmake and bash call.
#[derive(Debug, Subcommand)]
pub enum Verb {
    /// Print the cargo profile for a `CMAKE_BUILD_TYPE` (empty/absent → the
    /// development default). An unmapped build type is an error, not a guess.
    Resolve(ResolveArgs),

    /// Print the `cargo build` flags selecting this profile (nothing for
    /// `dev`, which is cargo's default).
    Args(FlagArgs),

    /// Print the `target/` subdirectory this profile's artifacts land in.
    Dir(DirArgs),

    /// Print the profile a named carve-out forces (e.g. `nuttx-rust`, whose
    /// images miscompile at `lto = "off"`). Errors on an unknown name rather
    /// than falling back, so a typo cannot silently build the broken profile.
    CarveOut(CarveOutArgs),

    /// Print this profile's definition as `KEY=VALUE` environment lines —
    /// empty unless nano-ros owns the profile.
    Env(EnvArgs),
}

#[derive(Debug, ClapArgs)]
pub struct ResolveArgs {
    /// The CMake build type. Case-insensitive; empty means "not chosen".
    #[arg(long = "build-type", value_name = "TYPE", default_value = "")]
    pub build_type: String,
}

#[derive(Debug, ClapArgs)]
pub struct FlagArgs {
    pub profile: String,
    /// Emit `cargo nextest` spelling (`--cargo-profile`) instead.
    #[arg(long)]
    pub nextest: bool,
}

#[derive(Debug, ClapArgs)]
pub struct DirArgs {
    pub profile: String,
}

#[derive(Debug, ClapArgs)]
pub struct CarveOutArgs {
    pub name: String,
}

#[derive(Debug, ClapArgs)]
pub struct EnvArgs {
    pub profile: String,
    /// Emit CMake list syntax (`KEY=VALUE;KEY=VALUE`) on one line, for
    /// `corrosion_set_env_vars`.
    #[arg(long)]
    pub cmake: bool,
}

/// `nros profile`: the build profiles an image's `profile` and `nros build
/// --profile=` accept (RFC-0133 D3). The plumbing verbs stay reachable here,
/// hidden, as the old spellings of `nros dev profile …`.
#[derive(Debug, Subcommand)]
pub enum UserVerb {
    /// List the build profiles an image can name, each with the CMake build
    /// type it maps to
    List(ListArgs),
    /// Show one build profile: the CMake build type it maps to and the cargo
    /// settings nano-ros defines for it
    Show(ShowArgs),
    #[command(hide = true)]
    Resolve(ResolveArgs),
    #[command(hide = true)]
    Args(FlagArgs),
    #[command(hide = true)]
    Dir(DirArgs),
    #[command(hide = true)]
    CarveOut(CarveOutArgs),
    #[command(hide = true)]
    Env(EnvArgs),
}

#[derive(Debug, ClapArgs)]
pub struct ListArgs {
    /// Print one JSON object instead of text.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, ClapArgs)]
pub struct ShowArgs {
    /// The profile, as an image names it (`nros-minsizerel`), or the CMake
    /// build type it maps from (`MinSizeRel`).
    #[arg(add = clap_complete::ArgValueCandidates::new(crate::cmd::complete::builtin_profiles))]
    pub profile: String,
    /// Print one JSON object instead of text.
    #[arg(long)]
    pub json: bool,
}

impl From<Verb> for UserVerb {
    fn from(v: Verb) -> Self {
        match v {
            Verb::Resolve(a) => UserVerb::Resolve(a),
            Verb::Args(a) => UserVerb::Args(a),
            Verb::Dir(a) => UserVerb::Dir(a),
            Verb::CarveOut(a) => UserVerb::CarveOut(a),
            Verb::Env(a) => UserVerb::Env(a),
        }
    }
}

/// The `--json` schema names (RFC-0133 D1).
pub const LIST_SCHEMA: &str = "nros.profile.list.v1";
pub const SHOW_SCHEMA: &str = "nros.profile.show.v1";

/// One build profile.
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct ProfileItem {
    /// The cargo profile, as an image's `profile` names it.
    pub name: &'static str,
    /// The `CMAKE_BUILD_TYPE` it maps to, or `None` for a cargo-only one.
    pub cmake_build_type: Option<&'static str>,
    /// What it is for.
    pub summary: &'static str,
    /// Whether nano-ros defines it (an `nros-*` preset) or cargo does.
    pub defined_by: &'static str,
    /// The cargo settings nano-ros defines for it; empty for cargo's own.
    pub settings: Vec<(String, String)>,
}

/// Every built-in profile, in [`nros_cargo_profile::BUILTIN`]'s order.
pub fn items() -> Vec<ProfileItem> {
    nros_cargo_profile::BUILTIN
        .iter()
        .map(|(name, summary)| {
            let preset = nros_cargo_profile::preset(name);
            ProfileItem {
                name,
                cmake_build_type: nros_cargo_profile::cmake_build_type(name),
                summary,
                defined_by: if preset.is_some() {
                    "nano-ros"
                } else {
                    "cargo"
                },
                settings: preset
                    .map(|p| {
                        p.settings
                            .iter()
                            .map(|s| (s.key.to_string(), s.value.to_string()))
                            .collect()
                    })
                    .unwrap_or_default(),
            }
        })
        .collect()
}

/// The built-in profile `name` names, by cargo name or CMake build type.
pub fn find(name: &str) -> Result<ProfileItem> {
    let all = items();
    let by_cmake = nros_cargo_profile::resolve(Some(name)).ok();
    all.iter()
        .find(|i| i.name == name || (!name.is_empty() && Some(i.name) == by_cmake))
        .cloned()
        .ok_or_else(|| {
            crate::cmd::frame::unknown(
                "build profile",
                name,
                all.iter().map(|i| i.name),
                "nros profile list",
            )
        })
}

pub fn render_list(items: &[ProfileItem]) -> String {
    let mut t = crate::cmd::frame::Table::new(
        &["PROFILE", "CMAKE BUILD TYPE", "WHAT IT IS"],
        "No build profiles.",
    );
    for i in items {
        t.row(vec![
            i.name.to_string(),
            i.cmake_build_type
                .unwrap_or("- (cargo images only)")
                .to_string(),
            i.summary.to_string(),
        ]);
    }
    t.footer(
        "Name one as `profile` on an image, or pass `nros build --profile=<name>`. \
         `nros profile show <name>` prints its settings.",
    );
    t.render()
}

pub fn render_show(i: &ProfileItem) -> String {
    let mut out = format!("profile: {}\n  {}\n", i.name, i.summary);
    out.push_str(&format!(
        "  CMake build type: {}\n",
        i.cmake_build_type
            .unwrap_or("none (a cargo-only profile; a CMake image refuses it)")
    ));
    out.push_str(&format!("  defined by: {}\n", i.defined_by));
    if !i.settings.is_empty() {
        out.push_str("  settings:\n");
        for (k, v) in &i.settings {
            out.push_str(&format!("    {k} = {v}\n"));
        }
    }
    out.push_str(&crate::cmd::frame::book_line(
        "guide/build/build-profiles.html",
    ));
    out
}

/// `nros profile …`: `list` and `show`, or a hidden plumbing verb.
pub fn run_user(verb: UserVerb) -> Result<()> {
    let plumbing = |verb| run(Args { verb });
    match verb {
        UserVerb::List(a) => {
            let items = items();
            if a.json {
                return crate::cmd::frame::print_json(LIST_SCHEMA, "items", &items, &[]);
            }
            print!("{}", render_list(&items));
            Ok(())
        }
        UserVerb::Show(a) => {
            let i = find(&a.profile)?;
            if a.json {
                return crate::cmd::frame::print_json(SHOW_SCHEMA, "item", &i, &[]);
            }
            print!("{}", render_show(&i));
            Ok(())
        }
        UserVerb::Resolve(a) => plumbing(Verb::Resolve(a)),
        UserVerb::Args(a) => plumbing(Verb::Args(a)),
        UserVerb::Dir(a) => plumbing(Verb::Dir(a)),
        UserVerb::CarveOut(a) => plumbing(Verb::CarveOut(a)),
        UserVerb::Env(a) => plumbing(Verb::Env(a)),
    }
}

pub fn run(args: Args) -> Result<()> {
    match args.verb {
        Verb::Resolve(ResolveArgs { build_type }) => {
            let profile =
                nros_cargo_profile::resolve(Some(&build_type)).map_err(|e| eyre::eyre!("{e}"))?;
            println!("{profile}");
        }
        Verb::Args(FlagArgs { profile, nextest }) => {
            let flags = if nextest {
                nros_cargo_profile::nextest_args(&profile)
            } else {
                nros_cargo_profile::build_args(&profile)
            };
            // Space-joined on ONE line: every caller substitutes this into a
            // command line, and an empty profile must yield an empty word, not
            // a stray blank argument.
            println!("{}", flags.join(" "));
        }
        Verb::Dir(DirArgs { profile }) => {
            println!("{}", nros_cargo_profile::target_dir(&profile));
        }
        Verb::CarveOut(CarveOutArgs { name }) => {
            let profile = nros_cargo_profile::carve_out(&name).ok_or_else(|| {
                eyre::eyre!(
                    "no build-profile carve-out named `{name}` (known: {})",
                    nros_cargo_profile::CARVE_OUTS
                        .iter()
                        .map(|(n, _)| *n)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })?;
            println!("{profile}");
        }
        Verb::Env(EnvArgs { profile, cmake }) => {
            let vars: Vec<String> = nros_cargo_profile::env(&profile)
                .into_iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect();
            if cmake {
                println!("{}", vars.join(";"));
            } else {
                for var in vars {
                    println!("{var}");
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The list is the table: one row per `BUILTIN` line, in its order, so a
    /// profile added there is listed with no CLI edit.
    #[test]
    fn the_list_is_the_builtin_table() {
        let names: Vec<&str> = items().iter().map(|i| i.name).collect();
        let want: Vec<&str> = nros_cargo_profile::BUILTIN
            .iter()
            .map(|(n, _)| *n)
            .collect();
        assert_eq!(names, want);
        let text = render_list(&items());
        for n in want {
            assert!(text.contains(n), "{n} missing from:\n{text}");
        }
    }

    /// `show` takes either vocabulary an image's `profile` takes: the cargo
    /// name or the CMake build type. Each preset shows its own settings.
    #[test]
    fn show_finds_a_profile_by_either_name() {
        assert_eq!(find("nros-minsizerel").unwrap().name, "nros-minsizerel");
        assert_eq!(find("MinSizeRel").unwrap().name, "nros-minsizerel");
        assert_eq!(find("Debug").unwrap().name, "dev");
        let p = find("nros-iterate").unwrap();
        assert_eq!(p.cmake_build_type, None);
        assert!(p.settings.contains(&("incremental".into(), "true".into())));
        assert_eq!(find("release").unwrap().defined_by, "cargo");
        let err = find("nope").unwrap_err().to_string();
        assert!(err.contains("nros profile list"), "{err}");
    }

    /// The JSON documents carry their schema names.
    #[test]
    fn json_documents_name_their_schema() {
        let doc = crate::cmd::frame::json_doc(LIST_SCHEMA, "items", &items(), &[]).unwrap();
        assert_eq!(doc["schema"], LIST_SCHEMA, "{doc}");
        assert_eq!(doc["items"].as_array().unwrap().len(), items().len());
        let doc =
            crate::cmd::frame::json_doc(SHOW_SCHEMA, "item", &find("dev").unwrap(), &[]).unwrap();
        assert_eq!(doc["schema"], SHOW_SCHEMA, "{doc}");
    }
}
