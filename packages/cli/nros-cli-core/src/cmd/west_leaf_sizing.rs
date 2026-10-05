//! Issue 1662 — `nros ws west-leaf-sizing`: a standalone Zephyr RUST leaf's
//! derived sizes, for the west configure that builds it.
//!
//! A Zephyr Rust leaf (`examples/zephyr/rust/*`) calls no `nano_ros_entry()`,
//! so neither of the two things an ENTRY image gets on the west road reaches
//! it: the entity-inventory fragment the module's knob resolver reads as rung 3
//! (`<build>/nros/entity_inventory.cmake`), and the sizing descriptor named to
//! cargo (issue 1407). Since issue 1603 split its component into a
//! host-buildable node package, the metadata probe DOES answer for it — and
//! nothing read that answer, so its pools stayed at Kconfig / crate defaults.
//!
//! This verb is the ONE derivation both lanes size from. It composes the
//! leaf's inventory exactly as the cargo-leaf road does
//! ([`crate::leaf_entity_env::inventory_for_leaf`] plus the monitor rows), then
//! renders it twice:
//!
//! * the C lane — the fragment `nros_resolve_knobs()` already loads, written
//!   at `--output-cmake`, so the resolver gains a SOURCE, not a second ladder;
//! * the Rust lane — the sizing descriptor, written under `--build-dir` by the
//!   cargo road's own writer ([`crate::sizing_descriptor::write_for_leaf`]),
//!   whose path is printed as `NROS_SIZING_DESCRIPTOR=<path>` for the
//!   configure to put on both cargo commands.
//!
//! Every probe sidecar read is printed as an `input <path>` line, so the
//! configure registers it and a probe that changes re-runs the configure.
//!
//! A directory that is not a standalone Zephyr Rust leaf prints nothing and
//! exits 0: the module calls this for every application, and an entry image or
//! a C leaf is a normal answer, not an error.

use std::{collections::BTreeMap, path::PathBuf};

use clap::Args as ClapArgs;
use eyre::{Result, WrapErr, eyre};

#[derive(Debug, ClapArgs)]
pub struct WestLeafSizingArgs {
    /// The west APPLICATION directory (`APPLICATION_SOURCE_DIR`).
    #[arg(long, value_name = "DIR")]
    pub leaf: PathBuf,

    /// The west build directory. The descriptor lands at
    /// `<build-dir>/nros/sizing/<image>.toml`.
    #[arg(long, value_name = "DIR")]
    pub build_dir: PathBuf,

    /// Write the entity-inventory fragment the knob resolver loads here.
    #[arg(long, value_name = "PATH")]
    pub output_cmake: PathBuf,

    /// The rustc triple this image builds for, when the configure knows it.
    /// Absent, the descriptor's `[target]` facts are refused rather than read
    /// off this host — a Zephyr board is not the host.
    #[arg(long, value_name = "TRIPLE")]
    pub target_triple: Option<String>,

    /// The nano-ros checkout (board catalog). Defaults to the one this binary
    /// resolves.
    #[arg(long, value_name = "DIR")]
    pub nano_ros_path: Option<PathBuf>,
}

pub fn run(args: WestLeafSizingArgs) -> Result<()> {
    // phase-447 A2 — the shared nano-ros-root ladder, as `board-facts` asks it.
    let root = crate::orchestration::nano_ros_root::resolve(args.nano_ros_path.clone(), &args.leaf)
        .ok_or_else(|| eyre!("{}", crate::orchestration::nano_ros_root::not_found_help()))?;
    let Some(mut img) = crate::cmd::leaf_settings::resolve_west(&args.leaf, &root)? else {
        return Ok(());
    };
    // The configure, not the board catalog, knows the triple of a Zephyr build
    // (west picks it from `-b`), and an absent one must not read as "host".
    img.target = args.target_triple.clone().or(img.target);
    let leaf = img.leaf.clone();
    let who = "west-leaf-sizing";

    // ---- ONE inventory ----------------------------------------------------
    let (mut inv, unprobeable) = crate::leaf_entity_env::inventory_for_leaf(&leaf)
        .wrap_err_with(|| format!("{}: entity inventory", leaf.display()))?;
    if !unprobeable.is_empty() {
        eprintln!(
            "{who}: {}: un-probeable component(s) skipped: {}",
            leaf.display(),
            unprobeable.join(", ")
        );
    }
    crate::leaf_entity_env::with_leaf_monitor_rows(&leaf, &mut inv);

    // ---- the C lane: the fragment rung 3 reads ---------------------------
    // Written even for an EMPTY inventory: `to_cmake` then states a refusal
    // and no `NROS_DERIVED_*` number, which is what the placeholder said too.
    if let Some(dir) = args.output_cmake.parent() {
        std::fs::create_dir_all(dir).wrap_err_with(|| format!("create `{}`", dir.display()))?;
    }
    crate::atomic_file::atomic_write(&args.output_cmake, &inv.to_cmake())
        .map_err(|e| eyre!("write `{}`: {e}", args.output_cmake.display()))?;

    // ---- the Rust lane: the descriptor ------------------------------------
    // Only when the inventory says something: an all-refused descriptor moves
    // the `[meta] basis` every consumer guards on in order to say nothing
    // (phase-454 W12's control, held on every road).
    if !inv.is_empty() {
        let facts = crate::cmd::board_facts::resolve(&leaf, &root, None, Some(&img.board), &|k| {
            std::env::var(k).ok()
        })
        .map_err(|e| eyre!("{}: board facts for `{}`: {e}", leaf.display(), img.board))?;
        let path_env: BTreeMap<String, PathBuf> = facts
            .into_iter()
            .filter(|(_, v)| {
                let p = std::path::Path::new(v);
                p.is_absolute() && p.exists()
            })
            .map(|(k, v)| (k, PathBuf::from(v)))
            .collect();
        let written =
            crate::sizing_descriptor::write_for_leaf(&img, &path_env, &args.build_dir, who)?;
        println!(
            "{}={}",
            nros_sizing_descriptor::DESCRIPTOR_ENV,
            written.path.display()
        );
    }

    // ---- what the configure must watch ------------------------------------
    for dir in std::iter::once(leaf.join("metadata")).chain(
        crate::orchestration::workspace::root_path_dep_packages(&leaf)
            .into_iter()
            .map(|p| p.join("metadata")),
    ) {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut files: Vec<PathBuf> = rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        files.sort();
        for f in files {
            println!("input {}", f.display());
        }
    }
    println!(
        "input {}",
        leaf.join(nros_orchestration_ir::leaf_system::SYSTEM_TOML)
            .display()
    );
    Ok(())
}
