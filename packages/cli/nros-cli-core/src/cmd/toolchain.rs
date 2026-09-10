//! `nros toolchain uninstall <version>` — phase-440 W6, RFC-0095 D11.
//!
//! ## What this can and cannot know today
//!
//! `toolchains/<version>/` is phase-440 **W7**'s directory and no host has one
//! yet; `nros-toolchain.toml`, the pin that names a version, is W7's file and no
//! project has one either. So on today's tree this verb has two honest outcomes
//! and one dishonest one, and the dishonest one is the default a lazier
//! implementation would take:
//!
//! * the directory is absent → **say so** and remove nothing. No risk, no
//!   refusal needed;
//! * the directory is there and a pin file names the version → **refuse**,
//!   naming the file;
//! * the directory is there and **no pin file could be found at all** →
//!   **refuse**. Not "no pin names it, therefore delete": those are different
//!   answers. The store is shared between projects while pins are per-project,
//!   so an empty pin set means the search looked in the wrong place at least as
//!   often as it means nothing needs the toolchain — and this is the exact
//!   moment where being wrong costs somebody a working build.
//!
//! Until W7 lands, the third branch is the one a real invocation reaches, which
//! is the correct behaviour rather than a limitation: nothing can yet establish
//! that no project pins a version, so nothing may act as though it had.
//!
//! `--ignore-pins` is the way to say it anyway. It is a sentence the user has to
//! write, which is the whole design.

use std::path::{Path, PathBuf};

use clap::{Args as ClapArgs, Subcommand};
use eyre::{Result, WrapErr, bail};

use crate::orchestration::{
    pin, release_manifest,
    store::{self, format_size},
};

#[derive(Debug, ClapArgs)]
pub struct Args {
    #[command(subcommand)]
    pub command: Sub,
}

#[derive(Debug, Subcommand)]
pub enum Sub {
    /// Remove one nano-ros toolchain from the store. Refuses while a known pin
    /// names the version — or while no pin file could be consulted at all.
    Uninstall(UninstallArgs),
    /// phase-443 W2 — read, or stamp, `share/nros/manifest.toml`: what a
    /// release DECLARES it is made of (RFC-0097 D7).
    Manifest(ManifestArgs),
}

/// `nros toolchain manifest` — RFC-0097 D7's file, both directions.
///
/// With no arguments it READS: the manifest of the toolchain this binary
/// belongs to. With `--write` it STAMPS one, which is what
/// `.github/workflows/release-nros.yml` calls while staging the asset.
///
/// The release workflow calls the binary it just built rather than `printf`-ing
/// four lines of YAML, and that is the whole design: `codegen` comes from
/// [`crate::abi_guard::EMITTED_VERSION`], so a manifest cannot claim a codegen
/// version its own binary does not emit. There is no `--codegen` flag, and
/// adding one would give the field back the drift the file exists to remove.
#[derive(Debug, ClapArgs)]
pub struct ManifestArgs {
    /// The STORE version being released (`0.5.0-nros1`) — the string a pin
    /// names, not the crate version `nros --version` prints.
    //
    // Deliberately NOT `--version`: `propagate_version = true` on the binary
    // generates that flag on every subcommand, so this spelling would collide
    // at startup. The name is also the more accurate of the two here.
    #[arg(long, value_name = "VERSION")]
    pub store_version: Option<String>,

    /// When the `nros-sdk-index.toml` in this asset was last moved (a date).
    #[arg(long, value_name = "DATE")]
    pub index: Option<String>,

    /// The nano-ros commit this was built from.
    #[arg(long = "nano-ros", value_name = "COMMIT")]
    pub nano_ros: Option<String>,

    /// Write the manifest here instead of printing it. Requires
    /// `--store-version`.
    #[arg(long, value_name = "PATH")]
    pub write: Option<PathBuf>,
}

#[derive(Debug, ClapArgs)]
pub struct UninstallArgs {
    /// The toolchain version, exactly as `toolchains/<version>` spells it.
    //
    // The explicit `id` is load-bearing, and a plain comment so it stays out of
    // `--help`. The binary sets `propagate_version = true`, so clap generates a
    // `--version` flag on EVERY subcommand, and a field named `version` collides
    // with it — a `debug_assert` panic at startup, on this verb alone, in every
    // build. Constructing `UninstallArgs` in a test never goes through clap, so
    // the whole suite passed while `nros toolchain uninstall 0.6.2` could not
    // run at all; `nros-cli/tests/store_verbs.rs` executes the real binary for
    // exactly that reason. `value_name` keeps the help reading `<VERSION>`.
    #[arg(id = "toolchain-version", value_name = "VERSION")]
    pub version: String,

    /// An extra file whose contents pin versions. Repeatable.
    #[arg(long, value_name = "PATH")]
    pub pin_file: Vec<PathBuf>,

    /// Uninstall without consulting any pin file. You are asserting no project
    /// pins this version.
    #[arg(long)]
    pub ignore_pins: bool,

    /// Report what would happen and remove nothing.
    #[arg(long)]
    pub dry_run: bool,

    /// Store root. Defaults to `$NROS_STORE`, else `$NROS_HOME`, else
    /// `~/.nros` — never an absolute literal (RFC-0095 D2).
    #[arg(long)]
    pub root: Option<PathBuf>,
}

pub fn run(args: Args) -> Result<()> {
    let cwd = std::env::current_dir().ok();
    match args.command {
        Sub::Uninstall(a) => uninstall(a, cwd.as_deref()),
        Sub::Manifest(a) => manifest(a),
    }
}

/// RFC-0097 D7's file, read or written.
pub fn manifest(args: ManifestArgs) -> Result<()> {
    if let Some(version) = &args.store_version {
        let m = release_manifest::stamp(version, args.index.as_deref(), args.nano_ros.as_deref());
        let rendered = release_manifest::render(&m);
        let Some(out) = &args.write else {
            print!("{rendered}");
            return Ok(());
        };
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)
                .wrap_err_with(|| format!("create {}", parent.display()))?;
        }
        std::fs::write(out, &rendered).wrap_err_with(|| format!("write {}", out.display()))?;
        println!("wrote {} (codegen {})", out.display(), m.codegen);
        return Ok(());
    }
    if args.write.is_some() {
        bail!(
            "--write needs --store-version: a manifest that cannot name its release records nothing."
        );
    }

    // Read mode: what does the release this binary belongs to declare?
    let exe = std::env::current_exe()?;
    match release_manifest::for_exe(&exe)? {
        Some(found) => {
            println!("{}", found.path.display());
            print!("{}", release_manifest::render(&found.manifest));
        }
        None => {
            // Not an error. A contributor's `packages/cli/target/**` build is
            // in no release at all, and a pre-W2 asset shipped VERSION alone —
            // different facts, so they are reported as different sentences.
            let where_from = pin::running_version(&exe)
                .map(|(v, o)| format!("{v} (from the {})", o.label()))
                .unwrap_or_else(|| "none — this is not an installed release".to_string());
            println!(
                "no share/nros/{} beside {}\n\
                 store version: {where_from}\n\
                 A release cut before RFC-0097 D7 declares no components, so \n\
                 `nros pin` cannot answer the codegen question for it.",
                release_manifest::FILE_NAME,
                exe.display()
            );
        }
    }
    Ok(())
}

/// `uninstall`, with the directory pin discovery walks up from passed IN — the
/// same argument as `cmd::store::gc`: in this checkout every ancestor of a
/// test's working directory holds a pin file, so a `current_dir()` read inside
/// would make the refusal below untestable in-tree.
pub fn uninstall(args: UninstallArgs, pin_search_from: Option<&Path>) -> Result<()> {
    let root = args.root.clone().unwrap_or_else(store::root);
    let dir = store::toolchains_dir(&root);

    let Some(entry) = store::toolchain_entry(&root, &args.version) else {
        // Nothing to delete, so the pin question does not arise. Say which of
        // the two absences it is: a store with no `toolchains/` at all is a
        // host phase-440 W7 has not reached, not a typo in the version.
        if dir.is_dir() {
            let have = store::scan(&root)
                .into_iter()
                .filter(|e| e.category == store::Category::Toolchains)
                .map(|e| e.version)
                .collect::<Vec<_>>();
            println!(
                "{} is not installed ({} does not exist). Installed: {}",
                args.version,
                dir.join(&args.version).display(),
                if have.is_empty() {
                    "none".to_string()
                } else {
                    have.join(", ")
                }
            );
        } else {
            println!(
                "no toolchains are installed — {} does not exist.\n\
                 (phase-440 W7 is what creates it; until then nano-ros is the checkout \
                 you are in, and there is nothing here to uninstall.)",
                dir.display()
            );
        }
        return Ok(());
    };

    let sources = if args.ignore_pins {
        Vec::new()
    } else {
        store::collect_pins(&args.pin_file, pin_search_from)?
    };

    if sources.is_empty() && !args.ignore_pins {
        bail!(
            "cannot verify that no project pins {}: no pin file was consulted.\n\
             Looked for {} in this directory and its ancestors and found none, so \
             `nothing pins it` is unestablished rather than true — and the store is \
             shared between projects while pins are per-project.\n\
             Refusing to remove {} ({}).\n\
             Either name a pin file (--pin-file <path>) or assert it yourself \
             (--ignore-pins).",
            args.version,
            store::PIN_FILE_NAMES.join(", "),
            entry.path.display(),
            format_size(entry.size_bytes)
        );
    }

    let pinned = store::pins_naming(&entry, &sources);
    if !pinned.is_empty() {
        let by: Vec<String> = pinned
            .iter()
            .map(|p| p.path.display().to_string())
            .collect();
        bail!(
            "refusing to uninstall {}: it is pinned by {}.\n\
             Change the pin first — removing a toolchain a project names breaks that \
             project's next build, and this verb has no way to warn it.",
            args.version,
            by.join(", ")
        );
    }

    if args.ignore_pins {
        println!("pins: NONE CONSULTED (--ignore-pins)");
    } else {
        println!(
            "pins consulted ({}): none names {}",
            sources.len(),
            args.version
        );
    }
    println!(
        "{}  {}  state {}",
        entry.path.display(),
        format_size(entry.size_bytes),
        entry.state.label()
    );

    if args.dry_run {
        println!("DRY RUN — nothing removed.");
        return Ok(());
    }

    std::fs::remove_dir_all(&entry.path)
        .map_err(|e| eyre::eyre!("remove {}: {e}", entry.path.display()))?;
    println!(
        "removed {} ({} freed).",
        args.version,
        format_size(entry.size_bytes)
    );
    Ok(())
}
