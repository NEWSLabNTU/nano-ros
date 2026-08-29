//! `nros materialize <image>` — take ownership of a generated entry
//! (phase-383 W7.b, RFC-0065 D5).
//!
//! The last resort, and it says so. Known escapes — `panic`, `profile` — are
//! declarations on the image and never leave generation; this is for what
//! nobody foresaw.
//!
//! **One way, deliberately.** Expo shipped `eject`, found it *"a one-way door
//! for most projects"*, and replaced it with always-generate plus declarative
//! plugins. We keep the door because the alternative is worse — a user with a
//! genuine need and no way to meet it — but we mark it, stamp it, and warn when
//! the shape it was cut for moves.

use std::path::PathBuf;

use clap::Parser;
use eyre::{Result, WrapErr};

#[derive(Parser, Debug)]
pub struct Args {
    /// Image whose entry to materialise — `native`, or `<bringup>:native`.
    pub image: String,

    /// Workspace root. Defaults to the current directory.
    #[arg(long)]
    pub workspace: Option<PathBuf>,

    /// nano-ros checkout holding `packages/boards`.
    #[arg(long)]
    pub nano_ros_path: Option<PathBuf>,

    /// Overwrite an entry that was already materialised.
    ///
    /// Off by default: the whole point is that the builder stops touching it,
    /// and so should this verb. A second `materialize` without `--force` would
    /// otherwise silently discard the edits the first one existed to enable.
    #[arg(long)]
    pub force: bool,
}

pub fn run(args: Args) -> Result<()> {
    let root = match &args.workspace {
        Some(w) => w.clone(),
        None => std::env::current_dir().wrap_err("resolving cwd as the workspace root")?,
    };

    // Reuse the builder's own resolution: an image that `nros build` cannot
    // resolve is one this verb must not guess at either.
    let build_args = super::build::Args {
        images: vec![args.image.clone()],
        workspace: Some(root.clone()),
        nano_ros_path: args.nano_ros_path.clone(),
        // Materialize resolves an image; it never runs west, so no Zephyr is
        // needed and none is looked for.
        zephyr_workspace: None,
        all: false,
        dry_run: true,
        offline: true,
        native_args: Vec::new(),
    };
    let plans = super::build::plan_builds(&build_args)?;
    let Some(plan) = plans.first() else {
        eyre::bail!("no image `{}` in this workspace", args.image);
    };

    let image_id = plan
        .qualified
        .rsplit(':')
        .next()
        .unwrap_or(&args.image)
        .to_string();
    let pkg = crate::builder::entry::package_name(&image_id);
    let dest = root.join("src").join(&pkg);

    if dest.exists() && !args.force {
        if crate::builder::materialize::is_materialized(&dest) {
            eyre::bail!(
                "{} is already materialised — it is yours, and `nros build` \
                 does not touch it.\n\
                 To regenerate and lose your edits: --force.",
                dest.display()
            );
        }
        eyre::bail!(
            "{} already exists and is not a generated entry. Refusing to \
             overwrite a package this verb did not write.",
            dest.display()
        );
    }

    // Find whatever the last build generated. Materialising is a COPY of a real
    // generated entry rather than a second emitter, so the two can never drift.
    let src = find_generated(&root, &pkg).ok_or_else(|| {
        eyre::eyre!(
            "no generated entry for `{image_id}` — run `nros build {image_id}` \
             first, so there is something to take ownership of.\n\
             (looked under {})",
            root.join("build").display()
        )
    })?;

    copy_tree(&src, &dest).wrap_err_with(|| format!("copying {}", src.display()))?;

    crate::builder::materialize::Stamp::current(
        &image_id,
        &plan.board,
        &plan.platform,
        entry_kind_token(plan.driver),
    )
    .write(&dest)
    .map_err(|e| eyre::eyre!("{e}"))?;

    println!("wrote {}", dest.display());
    println!(
        "\nThis entry is YOURS now — `nros build` will not regenerate it.\n\
         Its derivation stays live: `nros::main!` still reads the launch file at\n\
         compile time, so adding a node needs no change here. What is frozen is\n\
         the SHELL — the panic policy, the board boilerplate, the crate type.\n\
         `nros build` warns if the shape this was cut for moves."
    );
    Ok(())
}

/// The `entry_kind` token recorded in the stamp.
///
/// Derived from the driver rather than re-resolving the board: the driver was
/// already chosen from the board's platform, so this cannot disagree with it.
fn entry_kind_token(driver: crate::builder::plan::Driver) -> &'static str {
    use crate::builder::plan::Driver;
    match driver {
        Driver::West => "zephyr-staticlib",
        Driver::IdfPy => "board-run",
        Driver::Cargo | Driver::CMake => "hosted-main",
    }
}

/// Locate a generated entry named `pkg` under any build coordinate.
///
/// Searched rather than computed, because the coordinate depends on the RMW and
/// a user may have built with a different one than they are materialising from.
/// Deterministic: coordinates are walked in sorted order.
fn find_generated(root: &std::path::Path, pkg: &str) -> Option<PathBuf> {
    let mut coords: Vec<PathBuf> = std::fs::read_dir(root.join("build"))
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    coords.sort();
    coords
        .into_iter()
        .map(|c| c.join(pkg))
        .find(|d| d.join("Cargo.toml").is_file())
}

fn copy_tree(src: &std::path::Path, dest: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let to = dest.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_tree(&e.path(), &to)?;
        } else {
            std::fs::copy(e.path(), &to)?;
        }
    }
    Ok(())
}
