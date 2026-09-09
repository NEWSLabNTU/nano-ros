//! `nros store list` / `nros store gc` — phase-440 W6, RFC-0095 D11.
//!
//! The store is additive by design, so it grows and nothing shrinks it. These
//! are the two verbs that make that survivable, and they are deliberately dumb:
//! `list` reports, `gc` proposes. The reasoning lives in
//! [`crate::orchestration::store`]; what belongs HERE is why the flags are shaped
//! the way they are.
//!
//! **`--dry-run` is the default, and removing requires `--delete`.** Not a
//! preference — a store holds a 4 G workspace shared by three projects
//! (RFC-0095 D2), and that is precisely the thing nobody wants deleted by a flag
//! they misread. The inverse default would make the destructive path the one you
//! reach by typing LESS.
//!
//! **`--delete` refuses when no pin file was consulted.** "No pin names this"
//! and "I found nothing that could name anything" are different answers and used
//! to be indistinguishable in every tool that has made this mistake. A store is
//! shared between projects while pins are per-project, so `gc --delete` run from
//! `/tmp` can see no pins at all — and that is the case where deleting is most
//! likely to take out somebody's toolchain.

use std::path::PathBuf;

use clap::{Args as ClapArgs, Subcommand};
use eyre::{Result, bail};

use crate::orchestration::store::{self, Entry, GcPlan, KeepReason, format_age, format_size};

#[derive(Debug, ClapArgs)]
pub struct Args {
    #[command(subcommand)]
    pub command: Sub,
}

#[derive(Debug, Subcommand)]
pub enum Sub {
    /// What the store holds: name, version, size, when it was last read.
    List(ListArgs),

    /// Propose (and, with `--delete`, perform) removal of unused entries.
    /// Dry-run by default.
    Gc(GcArgs),
}

#[derive(Debug, ClapArgs)]
pub struct ListArgs {
    /// Store root. Defaults to `$NROS_STORE`, else `$NROS_HOME`, else
    /// `~/.nros` — never an absolute literal (RFC-0095 D2).
    #[arg(long)]
    pub root: Option<PathBuf>,
}

#[derive(Debug, ClapArgs)]
pub struct GcArgs {
    /// Remove entries not read for at least this long: `90d`, `12h`, `2w`.
    /// A bare number is refused — say the unit.
    #[arg(long, value_name = "DURATION")]
    pub older_than: String,

    /// Actually remove. Without it this only reports (the default).
    #[arg(long)]
    pub delete: bool,

    /// Report only. The default; accept it so the safe intent can be SPELLED
    /// rather than only implied, and so it can conflict with `--delete`
    /// instead of one of them silently winning.
    #[arg(long, conflicts_with = "delete")]
    pub dry_run: bool,

    /// An extra file whose contents pin versions. Repeatable. Any string value
    /// in it protects an entry with that version.
    #[arg(long, value_name = "PATH")]
    pub pin_file: Vec<PathBuf>,

    /// Delete without consulting any pin file. You are asserting no project
    /// needs these.
    #[arg(long)]
    pub ignore_pins: bool,

    /// Store root (see `list --root`).
    #[arg(long)]
    pub root: Option<PathBuf>,
}

pub fn run(args: Args) -> Result<()> {
    let cwd = std::env::current_dir().ok();
    match args.command {
        Sub::List(a) => list(a),
        Sub::Gc(a) => gc(a, cwd.as_deref()),
    }
}

fn resolve_root(explicit: Option<PathBuf>) -> (PathBuf, String) {
    match explicit {
        Some(p) => (p, "--root".to_string()),
        None => (store::root(), store::root_origin().to_string()),
    }
}

fn list(args: ListArgs) -> Result<()> {
    let (root, origin) = resolve_root(args.root);
    println!("store root: {}   (from {origin})", root.display());
    if !root.is_dir() {
        println!("  (no store here yet — nothing is provisioned)");
        return Ok(());
    }

    let entries = store::scan(&root);
    if entries.is_empty() {
        println!("  (empty)");
        return Ok(());
    }

    let now = std::time::SystemTime::now();
    let rows: Vec<[String; 5]> = entries
        .iter()
        .map(|e| {
            let age = now
                .duration_since(e.last_used)
                .unwrap_or(std::time::Duration::ZERO);
            [
                format_size(e.size_bytes),
                format_age(age),
                e.evidence.label().to_string(),
                e.state.label().to_string(),
                e.display_id(),
            ]
        })
        .collect();
    let header = ["SIZE", "LAST-USED", "EVIDENCE", "STATE", "ENTRY"];
    let widths: Vec<usize> = (0..5)
        .map(|i| {
            rows.iter()
                .map(|r| r[i].len())
                .chain(std::iter::once(header[i].len()))
                .max()
                .unwrap_or(0)
        })
        .collect();

    println!();
    print_row(&header.map(String::from), &widths);
    for row in &rows {
        print_row(row, &widths);
    }

    let total: u64 = entries.iter().map(|e| e.size_bytes).sum();
    let installed = entries
        .iter()
        .filter(|e| e.state == store::EntryState::Installed)
        .count();
    println!();
    println!(
        "{} entries, {} total — {installed} installed, {} not attributable to an install.",
        entries.len(),
        format_size(total),
        entries.len() - installed
    );
    for line in [
        "EVIDENCE `read` means a file in the entry was read after it was written;",
        "  `install` means nothing was, so LAST-USED is when it landed. A `noatime`",
        "  mount records no reads at all and every entry then reads `install` —",
        "  check before trusting an age.",
    ] {
        println!("{line}");
    }
    for line in [
        "`nros store gc` only ever removes `installed` entries. `legacy-flat` is the",
        "  unversioned prefix `nros sdk-path` still resolves through (issue 0628) —",
        "  removing one un-provisions a working host. `source-tree` is what a source",
        "  install BUILT from: nothing needs it, but it is also where local patches",
        "  would be, so it is reported rather than collected.",
    ] {
        println!("{line}");
    }
    let source_trees: u64 = entries
        .iter()
        .filter(|e| e.state == store::EntryState::SourceTree)
        .map(|e| e.size_bytes)
        .sum();
    if source_trees > 0 {
        println!(
            "  source trees hold {} — remove one by hand when you are sure it carries",
            format_size(source_trees)
        );
        println!("  nothing of yours; the next source install re-clones it.");
    }
    Ok(())
}

fn print_row(cells: &[String; 5], widths: &[usize]) {
    let line: Vec<String> = cells
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{:<width$}", c, width = widths[i]))
        .collect();
    println!("  {}", line.join("  ").trim_end());
}

/// `gc`, with the directory pin discovery walks up from passed IN.
///
/// The public verb hands it `current_dir()`; a test hands it a tempdir. The
/// reason is not tidiness: in this checkout every ancestor of a test's working
/// directory holds an `nros-sdk-index.toml`, so a `current_dir()` read inside
/// would make the "no pin file could be consulted" refusal — the one that keeps
/// a `gc --delete` run from `/tmp` from eating another project's toolchain —
/// impossible to test in-tree.
pub fn gc(args: GcArgs, pin_search_from: Option<&std::path::Path>) -> Result<()> {
    let older_than = store::parse_duration(&args.older_than)?;
    let (root, origin) = resolve_root(args.root.clone());
    println!("store root: {}   (from {origin})", root.display());
    if !root.is_dir() {
        println!("  (no store here — nothing to collect)");
        return Ok(());
    }

    let sources = if args.ignore_pins {
        Vec::new()
    } else {
        store::collect_pins(&args.pin_file, pin_search_from)?
    };
    if sources.is_empty() {
        if args.ignore_pins {
            println!("pins: NONE CONSULTED (--ignore-pins)");
        } else {
            println!("pins: none found");
        }
    } else {
        println!("pins consulted:");
        for s in &sources {
            println!("  {}  ({} version(s))", s.path.display(), s.rules.len());
        }
    }

    // The refusal is here rather than at the top so the report above is printed
    // either way: a user who ran this in the wrong directory learns WHY from the
    // same output that refuses them.
    if args.delete && sources.is_empty() && !args.ignore_pins {
        bail!(
            "refusing to delete: no pin file was consulted, so `no pin names this entry` \
             could not be established.\n\
             The store is shared between projects while pins are per-project, so an empty \
             pin set here means `I looked in the wrong place`, not `nothing needs these`.\n\
             Fix it by one of:\n  \
             run this from a project (looked for {} in this directory and its ancestors)\n  \
             name one:      --pin-file <path>\n  \
             or assert it:  --ignore-pins   (you are saying no project needs these)",
            store::PIN_FILE_NAMES.join(", ")
        );
    }

    let entries = store::scan(&root);
    let now = std::time::SystemTime::now();
    let plan = store::plan_gc(entries, older_than, &sources, now);

    report_plan(&plan, now, &args.older_than);

    if !args.delete {
        println!();
        println!(
            "DRY RUN (the default) — nothing was removed. Re-run with --delete to remove \
             the {} entr{} above.",
            plan.remove.len(),
            if plan.remove.len() == 1 { "y" } else { "ies" }
        );
        return Ok(());
    }

    if plan.remove.is_empty() {
        println!();
        println!("nothing to remove.");
        return Ok(());
    }

    println!();
    let mut freed = 0u64;
    for entry in &plan.remove {
        std::fs::remove_dir_all(&entry.path)
            .map_err(|e| eyre::eyre!("remove {}: {e}", entry.path.display()))?;
        freed += entry.size_bytes;
        println!(
            "removed {}  ({})",
            entry.display_id(),
            format_size(entry.size_bytes)
        );
    }
    println!("freed {}.", format_size(freed));
    Ok(())
}

fn report_plan(plan: &GcPlan, now: std::time::SystemTime, older_than: &str) {
    let pinned: Vec<(&Entry, &[PathBuf])> = plan.pinned().collect();
    if !pinned.is_empty() {
        println!();
        println!("kept — a pin names it:");
        for (entry, files) in pinned {
            let by: Vec<String> = files.iter().map(|f| f.display().to_string()).collect();
            println!("  {}  ({})", entry.display_id(), by.join(", "));
        }
    }

    let too_new = plan
        .keep
        .iter()
        .filter(|(_, r)| matches!(r, KeepReason::TooNew))
        .count();
    let unmanaged = plan
        .keep
        .iter()
        .filter(|(_, r)| matches!(r, KeepReason::NotInstalled(_)))
        .count();
    println!();
    println!(
        "kept: {} used within {older_than}, {unmanaged} not attributable to an install \
         (never removed — see `nros store list`).",
        too_new
    );

    println!();
    if plan.remove.is_empty() {
        println!("would remove: nothing.");
        return;
    }
    println!(
        "would remove {} entr{}, {}:",
        plan.remove.len(),
        if plan.remove.len() == 1 { "y" } else { "ies" },
        format_size(plan.bytes_to_remove())
    );
    for entry in &plan.remove {
        let age = now
            .duration_since(entry.last_used)
            .unwrap_or(std::time::Duration::ZERO);
        println!(
            "  {}  {}  last used {} ago ({})",
            entry.display_id(),
            format_size(entry.size_bytes),
            format_age(age),
            entry.evidence.label()
        );
    }
}
