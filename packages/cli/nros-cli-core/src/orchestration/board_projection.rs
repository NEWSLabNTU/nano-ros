//! The board descriptor, projected into cmake variables.
//!
//! RFC-0064 R5 D4. A Zephyr build needs a dozen facts about its board, and
//! cmake cannot read TOML. The previous answer was to author those facts a
//! SECOND time, in a `board.cmake` sitting beside the descriptor — which made
//! the two faces a drift problem, watched by a gate
//! (`phase215_f_manifest_drift.rs`) that checked zero boards for two
//! independent reasons and so watched nothing.
//!
//! This module is the other answer: one authored descriptor, one MECHANICAL
//! projection of it, regenerated per configure into the build directory and
//! never committed. There is nothing for a drift gate to compare because there
//! is only one file a human writes. The round-trip test in this module is what
//! replaces that gate, and it cannot go vacuous the way the old one did: it has
//! no "skip if the other face is missing" arm, because there is no other face.
//!
//! The same shape as `nros ws board-facts` (`cmake/NanoRosBoardFacts.cmake`),
//! and for the reason stated there: the resolution needs the board catalog, so
//! it belongs to the one implementation that has it rather than a second one in
//! cmake that would drift.
//!
//! DERIVED, not projected from an authored field (RFC-0064 R5 D3/D6):
//!
//! * `NROS_BOARD_PRJ_CONF` — `prj.conf` beside the descriptor, if present.
//! * `NROS_BOARD_BOARD_CONF` / `_OVERLAY` — `boards/<flat>.{conf,overlay}`,
//!   where `<flat>` is the Zephyr board id with `/` and `@` replaced by `_`.
//!   `board.cmake` stated exactly this rule in a comment and then hand-wrote
//!   both paths anyway.
//! * `NROS_BOARD_REQUIRES_RUST` — whether `rust_targets` is non-empty.
//! * `NROS_BOARD_RUST_SUPPORT_MODULE` — GENERATED (see
//!   [`write_rust_support_module`]), because its whole content is a function of
//!   the board id.

use std::path::{Path, PathBuf};

use eyre::{Result, eyre};

use super::board_descriptor::BoardDescriptor;

/// Zephyr board id → the filename stem Zephyr uses for per-board files.
///
/// `fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp` →
/// `fvp_baser_aemv8r_fvp_aemv8r_aarch64_smp`.
///
/// `/` separates the hwv2 `<board>/<soc>/<variant>` triple. `@` and `.` are
/// here for the REVISION form: a revision is spelled `board@1.5` on the command
/// line and board_1_5 in the per-revision file names, so a rule that handled
/// only `/` would silently look for a file nobody writes.
pub fn flatten_board_id(id: &str) -> String {
    id.chars()
        .map(|c| if matches!(c, '/' | '@' | '.') { '_' } else { c })
        .collect()
}

/// Kconfig symbol for a Zephyr board id's BOARD, e.g.
/// `fvp_baser_aemv8r/...` → `BOARD_FVP_BASER_AEMV8R`.
///
/// Zephyr's `BOARD_<name>` symbol names the BOARD, which is the first
/// `/`-separated segment — not the SoC and not the variant.
pub fn board_kconfig_symbol(id: &str) -> String {
    let board = id.split('/').next().unwrap_or(id);
    format!("BOARD_{}", board.to_uppercase())
}

/// The Zephyr files a board package carries beside its descriptor.
///
/// DERIVED by one rule (RFC-0064 R5 D3/D6), with two readers: the cmake
/// projection [`project`] (what `nano_ros_use_board()` applies to a
/// hand-written application) and `nros build`'s west road, which hands the
/// same files to a GENERATED application (issue 1288 — the FVP image was the
/// last hand-written entry, and it was hand-written only to call
/// `nano_ros_use_board`). Each is `Some` only when the file exists.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BoardFiles {
    /// `prj.conf` beside the descriptor — the board's base Kconfig.
    pub prj_conf: Option<PathBuf>,
    /// `boards/<flat>.conf` — the per-board-id Kconfig.
    pub board_conf: Option<PathBuf>,
    /// `boards/<flat>.overlay` — the per-board-id devicetree overlay.
    pub board_overlay: Option<PathBuf>,
}

impl BoardFiles {
    /// The Kconfig fragments, in the order `nano_ros_use_board()` merges them.
    pub fn conf_files(&self) -> Vec<PathBuf> {
        [&self.prj_conf, &self.board_conf]
            .into_iter()
            .flatten()
            .cloned()
            .collect()
    }
}

/// [`BoardFiles`] for a board whose Zephyr id is `west_board` and whose
/// descriptor lives in `board_dir`.
pub fn board_files(west_board: &str, board_dir: &Path) -> BoardFiles {
    let flat = flatten_board_id(west_board);
    let existing = |rel: PathBuf| {
        let abs = board_dir.join(rel);
        abs.is_file().then_some(abs)
    };
    BoardFiles {
        prj_conf: existing(PathBuf::from("prj.conf")),
        board_conf: existing(PathBuf::from("boards").join(format!("{flat}.conf"))),
        board_overlay: existing(PathBuf::from("boards").join(format!("{flat}.overlay"))),
    }
}

/// The directory holding `board`'s descriptor, or `None` for an in-memory one.
///
/// [`BoardDescriptor::source`] is recorded absolute for a workspace package's
/// descriptor and relative to the catalog root (`root`) for an in-tree one.
pub fn descriptor_dir(board: &BoardDescriptor, root: &Path) -> Option<PathBuf> {
    let src = Path::new(board.source.as_deref()?);
    let abs = if src.is_absolute() {
        src.to_path_buf()
    } else {
        root.join(src)
    };
    abs.parent().map(Path::to_path_buf)
}

/// One `set(<var> "<value>")` line, with cmake's escaping.
fn set_line(out: &mut String, var: &str, value: &str) {
    // A cmake string needs `\` and `"` escaped. Paths on this project never
    // carry either, but a projection that silently corrupts an unusual path is
    // worse than one that handles it.
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    out.push_str(&format!("set({var} \"{escaped}\")\n"));
}

/// The projection for `board`, whose descriptor lives in `board_dir`.
///
/// `board_dir` is where the derived files are looked for, so it is passed
/// rather than recomputed: an out-of-tree board's descriptor is reached by an
/// absolute path and an in-tree one by a workspace-relative one, and this
/// function must not have to know which.
pub fn project(board: &BoardDescriptor, board_dir: &Path, module_dir: &Path) -> Result<String> {
    let zephyr = board.zephyr.as_ref().ok_or_else(|| {
        eyre!(
            "board `{}` has no [board.zephyr] block, so it cannot be used from a \
             Zephyr build.\n  \
             `nano_ros_use_board()` is Zephyr-specific; a board for another \
             platform is consumed as a cargo path-dep instead.\n  \
             If this IS a Zephyr board, add:\n    \
             [board.zephyr]\n    west_board = \"<board>/<soc>/<variant>\"",
            board.names.first().map(String::as_str).unwrap_or("?")
        )
    })?;

    let mut out = String::new();
    out.push_str(
        "# GENERATED by `nros board cmake-vars` — DO NOT EDIT, DO NOT COMMIT.\n\
         #\n\
         # A mechanical projection of the board descriptor named below, written\n\
         # per configure into the build directory. RFC-0064 R5 D4: the descriptor\n\
         # is the single authored source, and this file exists only because cmake\n\
         # cannot read TOML. Editing it edits nothing that survives a reconfigure.\n",
    );
    out.push_str(&format!(
        "# SSoT: {}\n\n",
        board_dir.join("nros-board.toml").display()
    ));

    set_line(&mut out, "NROS_BOARD_ZEPHYR_ID", &zephyr.west_board);
    if let Some(v) = &zephyr.sdk_abi {
        set_line(&mut out, "NROS_BOARD_TOOLCHAIN", v);
    }
    if let Some(v) = &zephyr.default_rmw {
        set_line(&mut out, "NROS_BOARD_DEFAULT_RMW", v);
    }
    if let Some(v) = &zephyr.default_transport {
        set_line(&mut out, "NROS_BOARD_DEFAULT_TRANSPORT", v);
    }
    // phase-528 W2 — DERIVED from `[board.run] emulator` (RFC-0118 D4): the
    // emulator platform Zephyr's `-t run` names for that kind (`fvp:` ->
    // `armfvp`). It was `[board.zephyr] runner`, a second spelling of the
    // launch. Zephyr never reads it (its own board definition selects the
    // emulator); `nano_ros_entry` reads it as "an embedded board was
    // imported". Absent when the board states no emulator.
    if let Some(v) = board.run.as_ref().and_then(|r| r.zephyr_emulator()) {
        set_line(&mut out, "NROS_BOARD_RUNNER", v);
    }

    // --- derived paths -----------------------------------------------------
    let files = board_files(&zephyr.west_board, board_dir);
    for (var, path) in [
        ("NROS_BOARD_PRJ_CONF", &files.prj_conf),
        ("NROS_BOARD_BOARD_CONF", &files.board_conf),
        ("NROS_BOARD_BOARD_OVERLAY", &files.board_overlay),
    ] {
        if let Some(abs) = path {
            set_line(&mut out, var, &abs.display().to_string());
        }
    }

    // --- provisioning ------------------------------------------------------
    if let Some(p) = &board.provisioning {
        if let Some(v) = &p.zephyr_line {
            set_line(&mut out, "NROS_BOARD_ZEPHYR_LINE", v);
        }
        if let Some(v) = &p.rmw_source {
            set_line(&mut out, "NROS_BOARD_RMW_SOURCE", v);
        }
        if !p.tools.is_empty() {
            set_line(&mut out, "NROS_BOARD_GATED_PKGS", &p.tools.join(";"));
        }
        // ONE fact: the list. `REQUIRES_RUST` was a second field stating
        // whether the list was empty, which is a fact that can disagree with
        // itself.
        set_line(
            &mut out,
            "NROS_BOARD_REQUIRES_RUST",
            if p.rust_targets.is_empty() { "n" } else { "y" },
        );
        if !p.rust_targets.is_empty() {
            set_line(
                &mut out,
                "NROS_BOARD_RUST_TARGETS",
                &p.rust_targets.join(";"),
            );
            set_line(
                &mut out,
                "NROS_BOARD_RUST_SUPPORT_MODULE",
                &module_dir.display().to_string(),
            );
        }
    }

    Ok(out)
}

/// Generate the Zephyr module that enables `RUST_SUPPORTED` for this board.
///
/// Upstream's `RUST_SUPPORTED` lists the boards zephyr-lang-rust knows about,
/// and it does not know about ours. The fix is a cross-file `default y if
/// BOARD_<X>` extension carried as a Zephyr module, which is three lines whose
/// every character is a function of the board id — so it is generated, not
/// authored. Doing this by hand cost `fvp-aemv8r-smp` two committed files
/// (`Kconfig` and `zephyr/module.yml`) that said nothing the descriptor did not
/// already imply.
pub fn write_rust_support_module(dir: &Path, board_name: &str, west_board: &str) -> Result<()> {
    let sym = board_kconfig_symbol(west_board);
    // Zephyr derives `ZEPHYR_<NAME>_MODULE_DIR` from this by uppercasing and
    // substituting, so a hyphen in the name becomes a different identifier
    // than the one anybody would write. Underscores, as every module in the
    // tree already uses.
    let module_name = format!(
        "nros_board_{}_rust_support",
        board_name.replace(['-', '.'], "_")
    );
    std::fs::create_dir_all(dir.join("zephyr"))?;
    std::fs::write(
        dir.join("Kconfig"),
        format!(
            "# GENERATED by `nros board cmake-vars` — DO NOT EDIT, DO NOT COMMIT.\n\
             #\n\
             # zephyr-lang-rust's own `RUST_SUPPORTED` cannot list a board it has\n\
             # never heard of. This is the cross-file `default y if <board>`\n\
             # extension that adds ours, WITHOUT mutating the consumer's\n\
             # zephyr-lang-rust checkout.\n\
             config RUST_SUPPORTED\n\tbool\n\tdefault y if {sym}\n"
        ),
    )?;
    // `build.kconfig` resolves against the module ROOT, not against this
    // `zephyr/` directory — so it is `Kconfig`, not `../Kconfig`. Getting that
    // wrong yields a module Zephyr loads and whose Kconfig it silently never
    // reads, which is `RUST_SUPPORTED` staying off with no error anywhere.
    std::fs::write(
        dir.join("zephyr").join("module.yml"),
        format!(
            "# GENERATED by `nros board cmake-vars` — DO NOT EDIT, DO NOT COMMIT.\n\
             #\n\
             # Only the `RUST_SUPPORTED` Kconfig symbol lives here. The\n\
             # zephyr-lang-rust `_rust_map_target` mapping is CMake, not Kconfig,\n\
             # and comes from the patch set `nros setup board` applies.\n\
             name: {module_name}\n\
             \n\
             build:\n  \
             kconfig: Kconfig\n"
        ),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_board_id_flattens_the_way_zephyr_names_its_files() {
        assert_eq!(
            flatten_board_id("fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp"),
            "fvp_baser_aemv8r_fvp_aemv8r_aarch64_smp"
        );
        // A board REVISION uses `@`, and Zephyr flattens that too. The rule
        // `board.cmake` documented only mentioned `/`.
        assert_eq!(flatten_board_id("plank@1.5/soc"), "plank_1_5_soc");
    }

    #[test]
    fn the_kconfig_symbol_names_the_board_not_the_soc() {
        // The SoC and variant segments are NOT part of `BOARD_<x>`; a symbol
        // built from the whole id would never match and `RUST_SUPPORTED` would
        // stay off with no error anywhere.
        assert_eq!(
            board_kconfig_symbol("fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp"),
            "BOARD_FVP_BASER_AEMV8R"
        );
    }
}
