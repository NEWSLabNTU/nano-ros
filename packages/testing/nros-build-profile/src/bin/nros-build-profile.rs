//! `nros-build-profile` — passive build-profile analyzer CLI (phase-251 P3).
//!
//! Reads the timing artifacts a normal build already emitted under a project
//! directory and prints a stage table (+ optional drill-down / hints / JSON).
//! It never builds or flashes anything.
//!
//!   nros-build-profile [DIR] [--deep] [--json] [--no-hints]
//!
//! DIR defaults to the current directory.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use nros_build_profile::diagnostics::{self, Context};
use nros_build_profile::report::{self, Opts};
use nros_build_profile::{analyze, model::BuildProfile};

struct Args {
    dir: PathBuf,
    deep: bool,
    json: bool,
    hints: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut dir: Option<PathBuf> = None;
    let mut deep = false;
    let mut json = false;
    let mut hints = true;
    for a in std::env::args().skip(1) {
        match a.as_str() {
            "--deep" => deep = true,
            "--json" => json = true,
            "--no-hints" => hints = false,
            "-h" | "--help" => return Err(usage()),
            s if s.starts_with('-') => return Err(format!("unknown flag `{s}`\n\n{}", usage())),
            s => {
                if dir.is_some() {
                    return Err(format!("unexpected extra argument `{s}`\n\n{}", usage()));
                }
                dir = Some(PathBuf::from(s));
            }
        }
    }
    Ok(Args {
        dir: dir.unwrap_or_else(|| PathBuf::from(".")),
        deep,
        json,
        hints,
    })
}

fn usage() -> String {
    "usage: nros-build-profile [DIR] [--deep] [--json] [--no-hints]\n\
     \n\
     Parses build-timing artifacts under DIR (default '.') into a stage profile.\n\
     Looks for build*/.ninja_log (west/cmake/idf) and target*/cargo-timings/ (cargo).\n\
     \n\
       --deep       show the per-unit drill-down\n\
       --json       write nros-build-profile.json next to DIR\n\
       --no-hints   suppress diagnostic hints"
        .to_string()
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(2);
        }
    };

    let profile = match analyze(&args.dir) {
        Some(p) => p,
        None => {
            eprintln!(
                "no build-timing artifacts found under {}\n\
                 looked for: build*/.ninja_log (west/cmake/idf), target*/cargo-timings/ (cargo)\n\
                 build the project first; for deep cargo data run `cargo build --timings`.",
                args.dir.display()
            );
            return ExitCode::from(1);
        }
    };

    let hints = if args.hints {
        diagnostics::run(&profile, &Context::detect())
    } else {
        Vec::new()
    };

    let opts = Opts {
        deep: args.deep,
        top_n: 8,
        hints: args.hints,
    };
    print!("{}", report::render(&profile, &hints, opts));

    if args.json {
        match write_json(&args.dir, &profile, &hints) {
            Ok(path) => eprintln!("wrote {}", path.display()),
            Err(e) => {
                eprintln!("error writing JSON: {e}");
                return ExitCode::from(1);
            }
        }
    }

    ExitCode::SUCCESS
}

fn write_json(dir: &Path, profile: &BuildProfile, hints: &[String]) -> std::io::Result<PathBuf> {
    let path = dir.join("nros-build-profile.json");
    std::fs::write(&path, report::to_json(profile, hints))?;
    Ok(path)
}
