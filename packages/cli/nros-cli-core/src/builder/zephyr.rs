//! Zephyr overlay resolution (phase-383 W5, RFC-0065 D10).
//!
//! ## The correction this module exists to encode
//!
//! An earlier draft of RFC-0065 claimed that pointing `APPLICATION_CONFIG_DIR`
//! at a directory "preserves Zephyr's own `boards/<board>.conf`
//! auto-discovery". **It does not.** Zephyr's `configuration_files.cmake` puts
//! every auto-discovery inside one guard:
//!
//! ```cmake
//! zephyr_get(CONF_FILE SYSBUILD LOCAL)
//! if(NOT DEFINED CONF_FILE)                       # <- only when UNSET
//!   zephyr_file(CONF_FILES ${APPLICATION_CONFIG_DIR}        KCONF CONF_FILE NAMES "prj.conf" … REQUIRED)
//!   zephyr_file(CONF_FILES ${APPLICATION_CONFIG_DIR}/socs   KCONF CONF_FILE QUALIFIERS …)
//!   zephyr_file(CONF_FILES ${APPLICATION_CONFIG_DIR}/boards KCONF CONF_FILE …)
//! ```
//!
//! So setting `CONF_FILE` **suppresses `boards/` and `socs/` discovery
//! entirely**. Both downstream projects had already hit it —
//! `nano-ros-rt-eval`'s justfile carries the note *"boards/<board>.conf does not
//! automerge under explicit -DCONF_FILE; pass the NSOS … overlay explicitly"* —
//! and our own zephyr entries pass `-DCONF_FILE="prj.conf;prj-zenoh.conf"`,
//! which means they are suppressing it today.
//!
//! **Therefore: this module emits `EXTRA_CONF_FILE`, never `CONF_FILE`.**
//! Extras are merged AFTER the discovered set, which is also the right
//! precedence — a later fragment overrides an earlier one.
//!
//! ## Variants are Zephyr's own mechanism, not ours
//!
//! `prj_<buildtype>.conf` sets `CONF_FILE_BUILD_TYPE`, which then selects
//! `boards/<board>_<buildtype>.conf`. `autoware-safety-island`'s
//! `prj_actuation.conf` is already exactly this shape. So an image's `variant`
//! maps onto it (W5.c) rather than inventing a parallel axis — which would mean
//! two ways to say one thing, and this repository's most expensive defect class.
//!
//! ## sysbuild is detected, never declared
//!
//! Zephyr already decides how an application asks for a bootloader: a
//! `sysbuild.conf` carrying `SB_CONFIG_BOOTLOADER_MCUBOOT=y`. Its own source
//! comment reads *"sysbuild.conf is an optional file, because sysbuild is an
//! opt-in feature."* So presence IS the declaration and we invent no key (W5.d).
//!
//! Verified reachable through the external config dir by reading
//! `share/sysbuild/cmake/modules/sysbuild_kconfig.cmake` (Zephyr v3.7.0): it
//! resolves `sysbuild.conf` through `APPLICATION_CONFIG_DIR` and FORCEs that
//! variable into the cache so the images beneath inherit it.

use std::path::{Path, PathBuf};

use crate::orchestration::image::ImageBlock;

/// What `west build` must be told for one image.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ZephyrOverlays {
    /// `-DAPPLICATION_CONFIG_DIR=…` — where Zephyr takes `prj.conf`,
    /// `boards/<board>.conf` and `socs/*` from. `None` leaves Zephyr's default
    /// (the application directory).
    pub application_config_dir: Option<PathBuf>,
    /// `-DEXTRA_CONF_FILE=…` — the image's own fragments, in order.
    /// **Never `CONF_FILE`**; see the module docs.
    pub extra_conf_file: Vec<PathBuf>,
    /// `-DEXTRA_DTC_OVERLAY_FILE=…` — devicetree overlays, in order.
    pub extra_dtc_overlay_file: Vec<PathBuf>,
    /// `-DFILE_SUFFIX=…` from the image's `variant`, which drives Zephyr's own
    /// `prj_<variant>.conf` → `boards/<board>_<variant>.conf` chain.
    pub build_type: Option<String>,
    /// Whether `--sysbuild` is passed, from `sysbuild.conf`'s presence.
    pub sysbuild: bool,
}

/// Devicetree overlay extensions, so a `conf` list can carry both kinds and
/// each reaches the right Zephyr variable.
const DTS_EXTENSIONS: &[&str] = &["overlay", "dts", "dtsi"];

/// Resolve the overlays for `image`, whose board config lives under
/// `bringup_dir/boards/<board>/`.
///
/// Missing files are an ERROR, not a silent skip: an overlay the user wrote and
/// mis-spelled is the difference between a working image and a silent `.bss`
/// overflow, and Zephyr will not complain about a fragment nobody passed it.
pub fn resolve(
    bringup_dir: &Path,
    board: &str,
    image: &ImageBlock,
) -> Result<ZephyrOverlays, String> {
    resolve_in(bringup_dir, None, board, image)
}

/// [`resolve`] with the west APPLICATION directory too.
///
/// issue 0892 / RFC-0085. `conf` fragments live beside the thing they configure,
/// and on Zephyr that is the APPLICATION — `src/zephyr_entry/prj-zenoh.conf`,
/// next to the `CMakeLists.txt` west is pointed at. Resolving only against the
/// bringup was the assumption that the bringup IS the app, which the west
/// driver's fix removed: a `conf = ["prj-zenoh.conf"]` on the image reported
///
/// ```text
/// conf fragment `prj-zenoh.conf` not found. Looked in:
///   …/src/demo_bringup/boards/native_sim_native_64/prj-zenoh.conf
///   …/src/demo_bringup/prj-zenoh.conf
/// ```
///
/// while the file sat in the entry package all along.
pub fn resolve_in(
    bringup_dir: &Path,
    app_dir: Option<&Path>,
    board: &str,
    image: &ImageBlock,
) -> Result<ZephyrOverlays, String> {
    let mut out = ZephyrOverlays::default();

    // The board's config directory. Zephyr takes prj.conf, boards/* and socs/*
    // from here once it is set — see the module docs for why we must NOT also
    // set CONF_FILE.
    let config_dir = bringup_dir.join("boards").join(sanitize_board(board));
    if config_dir.is_dir() {
        out.application_config_dir = Some(config_dir.clone());
        // W5.d — presence IS the declaration.
        out.sysbuild = config_dir.join("sysbuild.conf").is_file();
    }

    // W5.c — the image's variant is Zephyr's own build-type axis.
    out.build_type = image.variant.clone();

    // The image's own fragments (F3: per-image, not per-board — rt-eval builds
    // one app on one board twice, differing only here).
    for name in &image.conf {
        let path = resolve_fragment(bringup_dir, app_dir, &config_dir, name)?;
        if is_devicetree(&path) {
            out.extra_dtc_overlay_file.push(path);
        } else {
            out.extra_conf_file.push(path);
        }
    }
    Ok(out)
}

/// Where a named fragment lives: beside the board config first, then relative
/// to the bringup package.
fn resolve_fragment(
    bringup_dir: &Path,
    app_dir: Option<&Path>,
    config_dir: &Path,
    name: &str,
) -> Result<PathBuf, String> {
    // Board config dir, then the application (where a Zephyr app keeps its own
    // `prj-*.conf`), then the bringup. The application rung is the one issue
    // 0892 needed: west is pointed at the entry package, and its fragments live
    // there.
    let mut candidates = vec![config_dir.join(name)];
    if let Some(app) = app_dir {
        candidates.push(app.join(name));
    }
    candidates.push(bringup_dir.join(name));
    for c in &candidates {
        if c.is_file() {
            return Ok(c.clone());
        }
    }
    Err(format!(
        "conf fragment `{name}` not found. Looked in:\n{}",
        candidates
            .iter()
            .map(|c| format!("  {}", c.display()))
            .collect::<Vec<_>>()
            .join("\n")
    ))
}

fn is_devicetree(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| DTS_EXTENSIONS.contains(&e))
}

/// A board id as a directory name.
///
/// Zephyr board strings carry slashes (`native_sim/native/64`), which cannot be
/// one directory component. `_` is the separator Zephyr's own overlay files
/// already use — `native_sim_native_64.conf` is what
/// `examples/workspaces/rust/src/zephyr_entry/boards/` ships.
#[must_use]
pub fn sanitize_board(board: &str) -> String {
    board.replace('/', "_")
}

/// Render the overlays as `west build` arguments, appended after `--`.
#[must_use]
pub fn west_args(o: &ZephyrOverlays) -> Vec<String> {
    let mut a = Vec::new();
    if let Some(dir) = &o.application_config_dir {
        a.push(format!("-DAPPLICATION_CONFIG_DIR={}", dir.display()));
    }
    if !o.extra_conf_file.is_empty() {
        a.push(format!(
            "-DEXTRA_CONF_FILE={}",
            join_semi(&o.extra_conf_file)
        ));
    }
    if !o.extra_dtc_overlay_file.is_empty() {
        a.push(format!(
            "-DEXTRA_DTC_OVERLAY_FILE={}",
            join_semi(&o.extra_dtc_overlay_file)
        ));
    }
    if let Some(bt) = &o.build_type {
        a.push(format!("-DFILE_SUFFIX={bt}"));
    }
    a
}

fn join_semi(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(";")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "").unwrap();
    }

    fn image(conf: &[&str]) -> ImageBlock {
        ImageBlock {
            board: Some("native_sim/native/64".to_string()),
            conf: conf.iter().map(|s| (*s).to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn a_fragment_beside_the_application_resolves() {
        // issue 0892 / RFC-0085. A Zephyr app keeps its own `prj-*.conf` next
        // to the `CMakeLists.txt` west is pointed at, and once the west driver
        // stopped assuming the bringup IS the app, an image naming one of them
        // could no longer find it:
        //
        //   conf fragment `prj-zenoh.conf` not found. Looked in:
        //     …/src/demo_bringup/boards/native_sim_native_64/prj-zenoh.conf
        //     …/src/demo_bringup/prj-zenoh.conf
        //
        // while the file sat in `src/zephyr_entry/` the whole time. This is
        // the real shape: `examples/workspaces/rust` keeps all four
        // `prj-<rmw>.conf` in the entry package, and its hand-written
        // CMakeLists FATAL_ERRORs without one, so the workspace could not
        // build through `nros build` at all.
        let tmp = tempfile::tempdir().unwrap();
        let bringup = tmp.path().join("src/demo_bringup");
        let app = tmp.path().join("src/zephyr_entry");
        touch(&app.join("prj-zenoh.conf"));

        let o = resolve_in(
            &bringup,
            Some(&app),
            "native_sim/native/64",
            &image(&["prj-zenoh.conf"]),
        )
        .expect("a fragment beside the application resolves");
        assert_eq!(o.extra_conf_file, vec![app.join("prj-zenoh.conf")]);
    }

    #[test]
    fn the_board_config_dir_still_wins_over_the_application() {
        // Precedence is unchanged by the rung above: a board-specific
        // fragment is the more specific answer, and adding the application to
        // the search must not let a generic copy shadow it.
        let tmp = tempfile::tempdir().unwrap();
        let bringup = tmp.path().join("bringup");
        let app = tmp.path().join("app");
        let board_dir = bringup.join("boards/native_sim_native_64");
        touch(&board_dir.join("prj-zenoh.conf"));
        touch(&app.join("prj-zenoh.conf"));

        let o = resolve_in(
            &bringup,
            Some(&app),
            "native_sim/native/64",
            &image(&["prj-zenoh.conf"]),
        )
        .expect("resolves");
        assert_eq!(o.extra_conf_file, vec![board_dir.join("prj-zenoh.conf")]);
    }

    #[test]
    fn a_missing_fragment_names_every_place_looked() {
        // The error is the whole diagnostic for this class — it is what turned
        // 0892's second layer from a guess into a two-minute fix — so it must
        // list the application rung too, not silently search it.
        let tmp = tempfile::tempdir().unwrap();
        let bringup = tmp.path().join("bringup");
        let app = tmp.path().join("app");
        std::fs::create_dir_all(&app).unwrap();

        let err = resolve_in(
            &bringup,
            Some(&app),
            "native_sim/native/64",
            &image(&["prj-zenoh.conf"]),
        )
        .expect_err("a fragment that exists nowhere is an error");
        assert!(err.contains(&app.display().to_string()), "{err}");
        assert!(err.contains(&bringup.display().to_string()), "{err}");
    }

    #[test]
    fn never_emits_conf_file_only_extra_conf_file() {
        // THE correction. `CONF_FILE` suppresses Zephyr's boards/ and socs/
        // auto-discovery entirely; `EXTRA_CONF_FILE` merges after it.
        let tmp = tempfile::tempdir().unwrap();
        let b = tmp.path();
        touch(&b.join("boards/native_sim_native_64/prj.conf"));
        touch(&b.join("boards/native_sim_native_64/prj-edf.conf"));

        let o = resolve(b, "native_sim/native/64", &image(&["prj-edf.conf"])).expect("resolves");
        let args = west_args(&o);
        let joined = args.join(" ");
        assert!(joined.contains("-DEXTRA_CONF_FILE="), "{joined}");
        assert!(
            !joined.contains("-DCONF_FILE="),
            "CONF_FILE suppresses boards/ and socs/ discovery: {joined}"
        );
    }

    #[test]
    fn the_board_config_dir_is_the_application_config_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let b = tmp.path();
        touch(&b.join("boards/native_sim_native_64/prj.conf"));
        let o = resolve(b, "native_sim/native/64", &image(&[])).expect("resolves");
        assert_eq!(
            o.application_config_dir,
            Some(b.join("boards/native_sim_native_64"))
        );
    }

    #[test]
    fn a_slashed_board_id_becomes_one_directory_component() {
        // `native_sim/native/64` cannot be a directory name; `_` is the
        // separator Zephyr's own overlay filenames already use.
        assert_eq!(
            sanitize_board("native_sim/native/64"),
            "native_sim_native_64"
        );
        assert_eq!(sanitize_board("mps2-an385-freertos"), "mps2-an385-freertos");
    }

    #[test]
    fn a_devicetree_overlay_reaches_the_dtc_variable_not_the_kconfig_one() {
        let tmp = tempfile::tempdir().unwrap();
        let b = tmp.path();
        touch(&b.join("boards/native_sim_native_64/prj.conf"));
        touch(&b.join("boards/native_sim_native_64/extra.overlay"));
        touch(&b.join("boards/native_sim_native_64/prj-edf.conf"));

        let o = resolve(
            b,
            "native_sim/native/64",
            &image(&["prj-edf.conf", "extra.overlay"]),
        )
        .expect("resolves");
        assert_eq!(o.extra_conf_file.len(), 1, "{:?}", o.extra_conf_file);
        assert_eq!(
            o.extra_dtc_overlay_file.len(),
            1,
            "{:?}",
            o.extra_dtc_overlay_file
        );
        let joined = west_args(&o).join(" ");
        assert!(joined.contains("-DEXTRA_DTC_OVERLAY_FILE="), "{joined}");
    }

    #[test]
    fn fragments_keep_the_order_they_were_declared_in() {
        // Order is load-bearing: a later Zephyr fragment overrides an earlier
        // one, so the declared sequence IS the precedence.
        let tmp = tempfile::tempdir().unwrap();
        let b = tmp.path();
        let d = b.join("boards/native_sim_native_64");
        touch(&d.join("prj.conf"));
        touch(&d.join("a.conf"));
        touch(&d.join("b.conf"));

        let o = resolve(b, "native_sim/native/64", &image(&["a.conf", "b.conf"])).expect("ok");
        let joined = west_args(&o).join(" ");
        let a = joined.find("a.conf").unwrap();
        let bpos = joined.find("b.conf").unwrap();
        assert!(a < bpos, "declared order is the precedence: {joined}");
    }

    #[test]
    fn a_missing_fragment_is_an_error_naming_where_it_looked() {
        // Silent-skip is the wrong failure: Zephyr never complains about a
        // fragment nobody passed it, so a typo becomes a silently different
        // image.
        let tmp = tempfile::tempdir().unwrap();
        let b = tmp.path();
        touch(&b.join("boards/native_sim_native_64/prj.conf"));
        let e = resolve(b, "native_sim/native/64", &image(&["prj-typo.conf"]))
            .expect_err("must refuse");
        assert!(e.contains("prj-typo.conf"), "{e}");
        assert!(e.contains("Looked in"), "says where it looked: {e}");
    }

    #[test]
    fn a_variant_becomes_zephyrs_own_build_type() {
        // W5.c — prj_<buildtype>.conf → CONF_FILE_BUILD_TYPE →
        // boards/<board>_<buildtype>.conf. ASI's prj_actuation.conf is already
        // this shape, so we map onto it rather than inventing an axis.
        let tmp = tempfile::tempdir().unwrap();
        let b = tmp.path();
        touch(&b.join("boards/native_sim_native_64/prj.conf"));
        let mut img = image(&[]);
        img.variant = Some("actuation".to_string());
        let o = resolve(b, "native_sim/native/64", &img).expect("resolves");
        assert_eq!(o.build_type.as_deref(), Some("actuation"));
        assert!(west_args(&o).join(" ").contains("-DFILE_SUFFIX=actuation"));
    }

    #[test]
    fn sysbuild_is_detected_from_the_file_never_declared() {
        // W5.d — Zephyr's own source: "sysbuild.conf is an optional file,
        // because sysbuild is an opt-in feature."
        let tmp = tempfile::tempdir().unwrap();
        let b = tmp.path();
        let d = b.join("boards/native_sim_native_64");
        touch(&d.join("prj.conf"));
        assert!(
            !resolve(b, "native_sim/native/64", &image(&[]))
                .unwrap()
                .sysbuild
        );

        touch(&d.join("sysbuild.conf"));
        assert!(
            resolve(b, "native_sim/native/64", &image(&[]))
                .unwrap()
                .sysbuild,
            "presence of sysbuild.conf IS the declaration"
        );
    }

    #[test]
    fn a_board_with_no_config_dir_resolves_to_zephyrs_defaults() {
        // Not an error: an application that needs no overlay is normal.
        let tmp = tempfile::tempdir().unwrap();
        let o = resolve(tmp.path(), "native_sim/native/64", &image(&[])).expect("resolves");
        assert!(o.application_config_dir.is_none());
        assert!(west_args(&o).is_empty());
    }
}
