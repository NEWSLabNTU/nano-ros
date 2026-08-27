//! Shadow-mode fixture cache: compute the key, record what the build produced,
//! report where the key was wrong — phase-395 W10.
//!
//! ```console
//! $ fixture-cache-shadow key    <artifact>          # the key and its coverage
//! $ fixture-cache-shadow record <artifact>...       # observe + store
//! $ fixture-cache-shadow record --all-known         # re-observe everything stored
//! $ fixture-cache-shadow report                     # per-coordinate tallies
//! $ fixture-cache-shadow report --check             # …and exit 1 on any mismatch
//! $ fixture-cache-shadow coverage                   # the class table alone
//! ```
//!
//! **It cannot serve a hit.** There is no lookup verb and no restore verb, on
//! purpose: a cache hit skips a build, so an incomplete key does not merely
//! cost a rebuild — it silently serves a wrong artifact. See
//! [`nros_tests::fixtures::cache_key`] for the argument in full and for which
//! of the four known-invisible input classes the key covers.
//!
//! # The intended shadow run
//!
//! Seed once after a normal fixture build, then re-observe after each change
//! the spec names — a rebase, a Kconfig edit, an env change, a linker-flag
//! change, a toolchain bump — and read `report`. `--all-known` exists so the
//! re-observation costs one command rather than a path list.
//!
//! `record` refuses rather than degrades: an artifact whose input set nothing
//! measured, or which attributes to no manifest row, is REPORTED and not
//! recorded. A key over an unmeasured input set is a key that matches
//! everything, which is the object this whole design exists to keep out of a
//! cache.

use std::{path::PathBuf, process::ExitCode};

use nros_tests::fixtures::{
    cache_key::{self, Coverage, INVISIBLE_CLASSES, Observation, ObserveError},
    lane::Coord,
};

fn usage() -> ! {
    eprintln!(
        "usage: fixture-cache-shadow <verb>\n\
         \n\
           key    <artifact>              print the key and per-class coverage\n\
           record <artifact>...           observe and store\n\
           record --all-known             re-observe every artifact in the store\n\
           report [--check]               per-coordinate tallies + every mismatch\n\
           coverage                       the invisible-input class table\n\
         \n\
         `key` and `record` take the AUTHORED leaf path and attribute its\n\
         coordinate through `row_coord()`. 8 of 221 artifact roots carry rows at\n\
         several coordinates (issue 0517) and fail closed there; name the\n\
         coordinate yourself for those:\n\
         \n\
           --coord <platform,lang,rmw>    supply the coordinate instead\n\
         \n\
         Shadow mode: no verb here can make a build skip work."
    );
    std::process::exit(2)
}

fn print_coverage() {
    println!("Input classes the compiler's dep record cannot see, and what the KEY does:\n");
    for c in INVISIBLE_CLASSES {
        println!(
            "  {} (issue {}) — {}",
            c.name,
            c.issue,
            c.designed.tag_public()
        );
        println!("    what: {}", c.what);
        println!("    why:  {}\n", c.rationale);
    }
}

fn describe(obs: &Observation) -> String {
    let mut s = format!(
        "{}\n  coord {}\n  row {}\n  key {:016x}\n  artifact {:016x}\n  \
         inputs {} (digest {:016x}, via {})\n",
        obs.artifact,
        obs.coord_str(),
        obs.row_label,
        obs.key,
        obs.artifact_hash,
        obs.covered_count,
        obs.covered_digest,
        match obs.provenance {
            cache_key::Provenance::CargoDepInfo => "cargo .d dep-info",
            cache_key::Provenance::NinjaDeps => "ninja -t deps",
        },
    );
    for c in &obs.classes {
        s.push_str(&format!(
            "  class {:<14} {:<15} {} witness(es)\n",
            c.name,
            c.coverage.tag_public(),
            c.witnesses.len()
        ));
    }
    s
}

/// Pull `--coord <platform,lang,rmw>` out of an argument list.
///
/// A malformed value is a HARD error, never a silent fall-back to attribution:
/// `--coord linux,rust` silently attributing would answer a question the caller
/// did not ask, and the coordinate is half the key.
fn take_coord(args: &mut Vec<String>) -> Option<Coord> {
    let i = args.iter().position(|a| a == "--coord")?;
    let Some(v) = args.get(i + 1).cloned() else {
        eprintln!("--coord needs a `platform,lang,rmw` value");
        std::process::exit(2);
    };
    let parts: Vec<&str> = v.split(',').map(str::trim).collect();
    if parts.len() != 3 || parts.iter().any(|p| p.is_empty()) {
        eprintln!("--coord expects `platform,lang,rmw`, got {v:?}");
        std::process::exit(2);
    }
    let coord = (
        parts[0].to_string(),
        parts[1].to_string(),
        parts[2].to_string(),
    );
    args.drain(i..=i + 1);
    Some(coord)
}

fn observe_one(path: &std::path::Path, coord: Option<&Coord>) -> Result<Observation, ObserveError> {
    match coord {
        Some(c) => cache_key::observe_with_coord(c, "(coordinate supplied)", path),
        None => cache_key::observe(path),
    }
}

fn record(paths: &[PathBuf], coord: Option<&Coord>) -> ExitCode {
    let mut refused = 0usize;
    for p in paths {
        match observe_one(p, coord) {
            Ok(obs) => match cache_key::record(&obs) {
                Ok(at) => {
                    print!("{}", describe(&obs));
                    println!("  recorded {}\n", at.display());
                }
                Err(e) => {
                    eprintln!("could not write a record for {}: {e}", obs.artifact);
                    refused += 1;
                }
            },
            Err(e) => {
                // A refusal is the designed outcome for an unmeasurable
                // artifact, and it is LOUD: silently skipping one would let a
                // report read as coverage of fixtures it never observed.
                eprintln!("REFUSED {}", ObserveErrorLine(&e));
                refused += 1;
            }
        }
    }
    if refused > 0 {
        eprintln!(
            "\n{refused} of {} artifact(s) were not observed. Each line above says why; \
             none of them are in the report.",
            paths.len()
        );
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

struct ObserveErrorLine<'a>(&'a ObserveError);

impl std::fmt::Display for ObserveErrorLine<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(verb) = args.first().map(String::as_str) else {
        usage()
    };
    match verb {
        "coverage" => {
            print_coverage();
            ExitCode::SUCCESS
        }
        "key" => {
            let mut rest: Vec<String> = args[1..].to_vec();
            let coord = take_coord(&mut rest);
            let Some(p) = rest.first() else { usage() };
            match observe_one(std::path::Path::new(p), coord.as_ref()) {
                Ok(obs) => {
                    print!("{}", describe(&obs));
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("REFUSED {e}");
                    ExitCode::from(1)
                }
            }
        }
        "record" => {
            let mut rest: Vec<String> = args[1..].to_vec();
            let coord = take_coord(&mut rest);
            if rest.iter().any(|a| a == "--all-known") {
                // Re-observe with each record's OWN stored coordinate rather
                // than re-attributing the path: the store already knows which
                // row an artifact belonged to, and re-deriving it would refuse
                // on exactly the ambiguous roots (issue 0517) whose records
                // were seeded with `--coord` in the first place.
                let mut known: Vec<(String, Coord)> = cache_key::load_records()
                    .into_iter()
                    .map(|o| (o.authored, o.coord))
                    .collect();
                known.sort();
                known.dedup();
                if known.is_empty() {
                    eprintln!(
                        "record --all-known: the store is empty, so there is nothing to \
                         re-observe. Seed it first:  fixture-cache-shadow record <artifact>...\n\
                         (Reporting success here would be a re-observation pass that \
                         observed nothing.)"
                    );
                    return ExitCode::from(1);
                }
                let root = nros_tests::project_root();
                let mut worst = ExitCode::SUCCESS;
                for (authored, c) in &known {
                    if record(&[root.join(authored)], Some(c)) != ExitCode::SUCCESS {
                        worst = ExitCode::from(1);
                    }
                }
                return worst;
            }
            if rest.is_empty() {
                usage()
            }
            record(
                &rest.iter().map(PathBuf::from).collect::<Vec<_>>(),
                coord.as_ref(),
            )
        }
        "report" => {
            let check = args.iter().any(|a| a == "--check");
            let records = cache_key::load_records();
            let report = cache_key::report_from(&records);
            print!("{}", cache_key::render(&report, &records));
            if records.is_empty() {
                eprintln!(
                    "\nThe store is empty. Nothing has been observed, so this report \
                     proves nothing about the key."
                );
                return ExitCode::from(1);
            }
            if check && !report.mismatches.is_empty() {
                eprintln!(
                    "\n{} mismatch(es): a key predicted an artifact the build did not \
                     produce. In a real cache each of these would have been a wrong \
                     artifact served silently.",
                    report.mismatches.len()
                );
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        _ => usage(),
    }
}

// `Coverage::tag` is private to the module (it is the on-disk spelling). The
// report and this binary want the same words, so expose them here rather than
// inventing a second vocabulary for the same three states.
trait CoverageWords {
    fn tag_public(self) -> &'static str;
}

impl CoverageWords for Coverage {
    fn tag_public(self) -> &'static str {
        match self {
            Coverage::Covered => "IN THE KEY",
            Coverage::Uncovered => "NOT in the key",
            Coverage::NotObservable => "nothing to observe",
        }
    }
}
