//! `nros materialize <image>` — take ownership of a generated entry
//! (phase-383 W7.b, RFC-0065 D5).
//!
//! The last resort, and it says so. Known escapes — `panic`, `profile`,
//! `before_boot` — are declarations on the image and never leave generation;
//! this is for what nobody foresaw.
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
    /// Image whose entry to materialise — `linux`, or `<bringup>:linux`.
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
        board: None,
        all: false,
        dry_run: true,
        verbose: false,
        offline: true,
        // phase-420 W7 — a QUERY resolves the whole workspace. Narrowing it
        // would answer a question nobody asked.
        selection: Default::default(),
        continue_on_error: false,
        cmake_args: Vec::new(),
        jobs: None,
        parallel_workers: None,
        build_base: None,
        check: None,
        allow_overriding: Vec::new(),
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

    // Issue 2175 — the manifest's path deps were written relative to where it
    // was GENERATED, and `src/<entry>/` sits at a different depth.
    let manifest = dest.join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest)
        .wrap_err_with(|| format!("reading {}", manifest.display()))?;
    let rebased = crate::builder::materialize::rebase_manifest_paths(&text, &src, &dest)
        .map_err(|e| eyre::eyre!("{}: {e}", manifest.display()))?;
    // RFC-0071 D13 / RFC-0065 D5 — the derived features and the registration
    // move to the owned entry's adapter crate, which `nros configure`
    // regenerates, so the derivation stays live. `dest` is `<ws>/src/<entry>`.
    let adapter_rel = format!(
        "../../generated/{}/{pkg}",
        nros_orchestration_ir::entry_names::ADAPTER_DIR
    );
    let rebased = crate::builder::materialize::owned_manifest(&rebased, &pkg, &adapter_rel);
    for name in ["main.rs", "lib.rs"] {
        let path = dest.join("src").join(name);
        if let Ok(text) = std::fs::read_to_string(&path) {
            std::fs::write(&path, crate::builder::materialize::owned_source(&text))
                .wrap_err_with(|| format!("writing {}", path.display()))?;
        }
    }
    std::fs::write(&manifest, rebased)
        .wrap_err_with(|| format!("writing {}", manifest.display()))?;
    // Write the adapter now, so the manifest resolves before the next build
    // (an editor's cargo loads it at once). `nros build` rewrites it each time.
    write_adapter_now(
        &root,
        &plan.qualified,
        &pkg,
        &dest,
        args.nano_ros_path.as_deref(),
    );

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

/// RFC-0071 D13 — write the owned entry's adapter crate straight away.
///
/// Best effort, and only a note when it cannot: `nros build` writes the same
/// adapter before its cargo step, so a miss here costs an editor's first load,
/// never a build. The bringup is the package named before the `:` of the
/// qualified image, found among `src/*` by directory or `package.xml` name.
fn write_adapter_now(
    root: &std::path::Path,
    qualified: &str,
    pkg: &str,
    dest: &std::path::Path,
    sdk_root: Option<&std::path::Path>,
) {
    let bringup = qualified.split(':').next().unwrap_or_default();
    let found = std::fs::read_dir(root.join("src"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|d| d.join("system.toml").is_file())
        .find(|d| {
            d.file_name().is_some_and(|n| n == bringup)
                || std::fs::read_to_string(d.join("package.xml"))
                    .is_ok_and(|x| x.contains(&format!("<name>{bringup}</name>")))
        });
    let result = found
        .ok_or_else(|| format!("no bringup `{bringup}` under src/"))
        .and_then(|dir| {
            let raw =
                std::fs::read_to_string(dir.join("system.toml")).map_err(|e| e.to_string())?;
            let sys =
                toml::from_str::<crate::orchestration::cargo_metadata_schema::SystemToml>(&raw)
                    .map_err(|e| e.to_string())?;
            crate::orchestration::facade::write_facade(
                pkg,
                dest,
                &dest.join("Cargo.toml"),
                &dir,
                &sys,
                &root.join("generated"),
                sdk_root,
            )
            .map_err(|e| format!("{e:#}"))
        });
    match result {
        Ok(Some(f)) => println!("wrote {}", f.dir.display()),
        Ok(None) => {}
        Err(why) => {
            println!("note: the adapter crate was not written here ({why}); `nros build` writes it")
        }
    }
}

/// The `entry_kind` token recorded in the stamp.
///
/// Derived from the driver rather than re-resolving the board: the driver was
/// already chosen from the board's platform, so this cannot disagree with it.
fn entry_kind_token(driver: crate::builder::plan::Driver) -> &'static str {
    use crate::builder::plan::Driver;
    match driver {
        Driver::West => "zephyr-staticlib",
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

/// What the generated entry dir holds that is BUILD STATE, not the entry: the
/// image's cargo target dir and its per-image settings file, both of which
/// `nros build` keeps writing under `build/` (issue 2175 — copying them put a
/// whole target dir into the user's source tree).
const NOT_THE_ENTRY: [&str; 2] = ["target", crate::builder::cargo_config::FILE_NAME];

fn copy_tree(src: &std::path::Path, dest: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        if NOT_THE_ENTRY.iter().any(|n| e.file_name() == *n) {
            continue;
        }
        let to = dest.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_tree(&e.path(), &to)?;
        } else {
            std::fs::copy(e.path(), &to)?;
        }
    }
    Ok(())
}
