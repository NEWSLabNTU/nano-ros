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

use clap::{Parser, Subcommand};
use eyre::Result;

#[derive(Debug, Parser)]
pub struct Args {
    #[command(subcommand)]
    pub verb: Verb,
}

#[derive(Debug, Subcommand)]
pub enum Verb {
    /// Print the cargo profile for a `CMAKE_BUILD_TYPE` (empty/absent → the
    /// development default). An unmapped build type is an error, not a guess.
    Resolve {
        /// The CMake build type. Case-insensitive; empty means "not chosen".
        #[arg(long = "build-type", value_name = "TYPE", default_value = "")]
        build_type: String,
    },

    /// Print the `cargo build` flags selecting this profile (nothing for
    /// `dev`, which is cargo's default).
    Args {
        profile: String,
        /// Emit `cargo nextest` spelling (`--cargo-profile`) instead.
        #[arg(long)]
        nextest: bool,
    },

    /// Print the `target/` subdirectory this profile's artifacts land in.
    Dir { profile: String },

    /// Print the profile a named carve-out forces (e.g. `nuttx-rust`, whose
    /// images miscompile at `lto = "off"`). Errors on an unknown name rather
    /// than falling back, so a typo cannot silently build the broken profile.
    CarveOut { name: String },

    /// Print this profile's definition as `KEY=VALUE` environment lines —
    /// empty unless nano-ros owns the profile.
    Env {
        profile: String,
        /// Emit CMake list syntax (`KEY=VALUE;KEY=VALUE`) on one line, for
        /// `corrosion_set_env_vars`.
        #[arg(long)]
        cmake: bool,
    },
}

pub fn run(args: Args) -> Result<()> {
    match args.verb {
        Verb::Resolve { build_type } => {
            let profile =
                nros_cargo_profile::resolve(Some(&build_type)).map_err(|e| eyre::eyre!("{e}"))?;
            println!("{profile}");
        }
        Verb::Args { profile, nextest } => {
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
        Verb::Dir { profile } => {
            println!("{}", nros_cargo_profile::target_dir(&profile));
        }
        Verb::CarveOut { name } => {
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
        Verb::Env { profile, cmake } => {
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
