//! RFC-0071 D15 (phase-545 W2) — linking a backend the board crate does not
//! name.
//!
//! D13 links a Rust backend through the board crate's `rmw-<x>` feature, and
//! the board registers it in its boot path. An out-of-tree backend has no such
//! feature on any in-tree board, so before D15 it was not linked at all.
//!
//! Exactly one party registers each backend:
//!
//! * **the board links it** ([`board_links`]) — nothing changes;
//! * **it does not** — the image links it, through a direct path dependency on
//!   the descriptor's `[rmw.provides.cargo] crate` ([`ImageBackend`]), and
//!   registers it through the `(image)` arm of `__nros_register_backends!`,
//!   which the ordinary `nros::main!` arms call.
//!
//! An image with a west half (Zephyr) is not this module's: the west build's
//! own backend dependency is D15's dependency there (amended ruling 2), so the
//! callers skip it.

use std::path::{Path, PathBuf};

use crate::orchestration::{
    cargo_metadata_schema::SystemToml,
    facade::crate_declares_feature,
    rmw_index::{ResolvedRmwIn, RmwIndex},
};

/// One backend the image links and registers itself (D15).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageBackend {
    /// The backend's canonical name.
    pub rmw: String,
    /// `[rmw.provides.cargo] crate` — the dependency key.
    pub krate: String,
    /// The crate's directory, absolute ([`crate_dir`]).
    pub dir: PathBuf,
    /// The edition and the backend's capability features, each named only
    /// where the crate declares it. The board's `platform_feature` is added by
    /// the writer of the dependency line, which holds the board's facts.
    pub features: Vec<String>,
    /// `[rmw.provides.cargo] register`, the function in [`Self::krate`] that
    /// registers the backend; `None` when the descriptor names no register
    /// crate.
    pub register: Option<String>,
}

/// How the `(image)` arm reaches a backend crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Root {
    /// `::<crate>` — the generated entry depends on it directly.
    Direct,
    /// `$crate::<re-export>` — an owned entry's adapter depends on it and
    /// re-exports it under [`reexport_name`].
    Reexported,
}

/// The register call of `b`, reached as `root` says.
#[must_use]
pub fn register_call(b: &ImageBackend, root: Root) -> Option<String> {
    let f = b.register.as_ref()?;
    Some(match root {
        Root::Direct => format!("::{}::{f}()", b.krate.replace('-', "_")),
        Root::Reexported => format!("$crate::{}::{f}()", reexport_name(&b.krate)),
    })
}

/// The name an owned entry's adapter re-exports `krate` under, so its
/// exported macro reaches the backend as `$crate::<this>`.
#[must_use]
pub fn reexport_name(krate: &str) -> String {
    format!("__nros_backend_{}", krate.replace('-', "_"))
}

/// Does the backend name no `[rmw.provides.cargo] crate`, so that an image
/// gets it from a C/C++ library its cmake build links rather than from an rlib
/// (Cyclone DDS, uorb)?
///
/// The ONE predicate for "no rlib": [`crate_dir`] answers `None` exactly here,
/// which is what `west_app` keys a Rust application's per-message type support
/// on, and the `(gated)` registration arm keys the C register hook on (see
/// `facade::render_backend_macros_inner`). Two spellings of it would be free to
/// disagree about which backends a Zephyr Rust image has to register through C.
#[must_use]
pub fn is_cmake_only(r: &ResolvedRmwIn) -> bool {
    r.descriptor.cargo_crate.is_empty()
}

/// Ruling 3 — the directory of the backend's cargo crate, or `None` when the
/// descriptor names no `[rmw.provides.cargo] crate` (cmake-only for D15).
///
/// `package_dir.join(register_dir)` when `crate` is the register crate, else
/// `package_dir`. This is the deleted `west_app::backend_dir`'s answer for
/// every in-tree backend (xrce's `nros-rmw-xrce-cffi` sits at
/// `../nros-rmw-xrce-cffi`), and the scan's answer for an out-of-tree one.
///
/// # Errors
///
/// The directory has no readable `Cargo.toml`, or its `[package] name` is not
/// `crate`: the provider's error, and the scan answer is authoritative.
pub fn crate_dir(r: &ResolvedRmwIn) -> Result<Option<PathBuf>, String> {
    let krate = r.descriptor.cargo_crate.as_str();
    if is_cmake_only(r) {
        return Ok(None);
    }
    let dir = if r.descriptor.register_crate == krate {
        crate::builder::paths::lexical_normalize(&r.package_dir.join(&r.descriptor.register_dir))
    } else {
        r.package_dir.clone()
    };
    let manifest = dir.join("Cargo.toml");
    let name = std::fs::read_to_string(&manifest)
        .ok()
        .and_then(|raw| toml::from_str::<toml::Value>(&raw).ok())
        .and_then(|v| v.get("package")?.get("name")?.as_str().map(str::to_string));
    match name {
        Some(n) if n == krate => Ok(Some(dir)),
        Some(n) => Err(format!(
            "rmw `{}`: `[rmw.provides.cargo] crate = \"{krate}\"`, but the crate at {} \
             is `{n}` (provider at {})",
            r.declared,
            dir.display(),
            r.package_dir.display()
        )),
        None => Err(format!(
            "rmw `{}`: `[rmw.provides.cargo] crate = \"{krate}\"` names no crate at {} \
             (provider at {})",
            r.declared,
            dir.display(),
            r.package_dir.display()
        )),
    }
}

/// Does the board crate at `board_dir`, with `board_features` selected, link
/// the backend `r`?
///
/// Yes when it declares the backend's `rmw-<x>` (the ruling's case), and also
/// when a feature the image already selects on it enables the backend's crate
/// (`xrce-transport = ["serial", "dep:nros-rmw-xrce-cffi"]` on the mps2 board,
/// which registers XRCE in its own boot path after installing the transport).
#[must_use]
pub fn board_links(dir: &Path, board_features: &[String], r: &ResolvedRmwIn) -> bool {
    if crate_declares_feature(dir, &r.cargo_feature) {
        return true;
    }
    let krate = r.descriptor.register_crate.as_str();
    if krate.is_empty() {
        return false;
    }
    let Some(table) = std::fs::read_to_string(dir.join("Cargo.toml"))
        .ok()
        .and_then(|raw| toml::from_str::<toml::Value>(&raw).ok())
        .and_then(|v| v.get("features").and_then(|f| f.as_table()).cloned())
    else {
        return false;
    };
    // The transitive closure of the selected features inside the board's own
    // table; `dep:<crate>` or `<crate>/<feat>` anywhere in it links the crate.
    let mut todo: Vec<String> = board_features.to_vec();
    let mut seen = std::collections::BTreeSet::new();
    while let Some(f) = todo.pop() {
        if !seen.insert(f.clone()) {
            continue;
        }
        for v in table
            .get(&f)
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str())
        {
            let dep = v.strip_prefix("dep:").unwrap_or(v);
            let dep = dep.split('/').next().unwrap_or(dep).trim_end_matches('?');
            if dep == krate {
                return true;
            }
            if table.contains_key(v) {
                todo.push(v.to_string());
            }
        }
    }
    false
}

/// D7's refusal for a cargo-rooted image whose backend offers no cargo crate.
#[must_use]
pub fn cmake_only_refusal(image: &str, r: &ResolvedRmwIn, board: &str) -> String {
    format!(
        "image `{image}`: backend `{rmw}` provides only a cmake target; a Rust-rooted \
         build needs a `sys_crate` bridge (RFC-0071 D7). Its `[rmw.provides.cargo]` \
         names no `crate`, and board crate `{board}` does not link it through \
         `{feature}`.",
        rmw = r.declared,
        feature = r.cargo_feature,
    )
}

/// The backends `image` must link itself: each of `linked` the board crate at
/// `board_dir` does not link ([`board_links`]), as the dependency to write, or
/// the refusal to report when the image is built with no west half.
///
/// The refusal is carried rather than returned because only the writer knows
/// whether the image has a west half, where none of this applies.
///
/// No board crate (`None`) is no answer, not "the board links nothing": an
/// owned entry may reach its board through a selection package this cannot
/// see, so D15 says nothing and the image is built as before.
///
/// # Errors
///
/// A backend name that does not resolve (D5's refusal).
pub fn image_backends(
    rmws: &RmwIndex,
    image: &str,
    linked: &[String],
    sys: &SystemToml,
    edition: rosidl_codegen::RosEdition,
    board_dir: Option<&Path>,
    board_features: &[String],
) -> eyre::Result<Vec<Result<ImageBackend, String>>> {
    let Some(board_dir) = board_dir else {
        return Ok(Vec::new());
    };
    let mut rows: Vec<std::sync::Arc<ResolvedRmwIn>> = Vec::new();
    for b in linked {
        let r = rmws.resolve(b).map_err(|e| {
            eyre::eyre!("image `{image}` links rmw `{b}`, which does not resolve: {e}")
        })?;
        if !rows.iter().any(|x| x.declared == r.declared) {
            rows.push(r);
        }
    }
    rows.sort_by(|a, b| a.declared.cmp(&b.declared));
    let board_name = board_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("?");
    let mut out = Vec::new();
    for r in rows {
        if board_links(board_dir, board_features, &r) {
            continue;
        }
        out.push(image_backend(image, &r, sys, edition, board_name));
    }
    Ok(out)
}

fn image_backend(
    image: &str,
    r: &ResolvedRmwIn,
    sys: &SystemToml,
    edition: rosidl_codegen::RosEdition,
    board_name: &str,
) -> Result<ImageBackend, String> {
    let krate = r.descriptor.cargo_crate.clone();
    let Some(dir) = crate_dir(r).map_err(|e| format!("image `{image}`: {e}"))? else {
        return Err(cmake_only_refusal(image, r, board_name));
    };
    let register = match r.descriptor.register_crate.as_str() {
        "" => None,
        rc if rc == krate => Some(r.descriptor.register_fn.clone()),
        rc => {
            return Err(format!(
                "image `{image}`: backend `{}` links crate `{krate}` but registers \
                 through `{rc}`, which the image does not depend on. A backend the board \
                 does not link must register through its `[rmw.provides.cargo] crate` \
                 (RFC-0071 D15).",
                r.declared
            ));
        }
    };
    // The board's platform feature is not known here: the writer of the
    // dependency line adds it ([`with_platform_features`]).
    let features = dependency_features(r, &dir, sys, edition, &[]);
    Ok(ImageBackend {
        rmw: r.declared.clone(),
        krate,
        dir,
        features,
        register,
    })
}

/// RFC-0071 D9 — the features a direct dependency on backend `r`'s crate (at
/// `dir`) names, on EVERY road that writes one: the image's ROS edition
/// (`ros-<edition>`), the capability features the image declares and the
/// backend implements (`[rmw.capabilities]`, e.g. `safety` → `safety-e2e`),
/// and then the board's `platform` features. Each is named only where the
/// crate declares it.
///
/// The edition and capabilities come first, sorted; the platform features
/// follow in the order given ([`with_platform_features`]). Both roads call
/// this: the cargo road through [`image_backend`] (with no platform, which its
/// dependency writer adds once it holds the board), the west road through
/// `builder::west_app::resolve_in` (issue 2393 — it used to forward the
/// platform feature alone, so a jazzy Zephyr Rust image built its backend for
/// humble).
#[must_use]
pub fn dependency_features(
    r: &ResolvedRmwIn,
    dir: &Path,
    sys: &SystemToml,
    edition: rosidl_codegen::RosEdition,
    platform: &[String],
) -> Vec<String> {
    let mut features = derived_features(r, sys, edition);
    features.retain(|f| crate_declares_feature(dir, f));
    with_platform_features(&features, dir, platform)
}

/// The image-implied half of [`dependency_features`], before any crate is
/// asked what it declares: the image's ROS edition and the capability
/// features it declares that backend `r` implements, by the descriptor's
/// `[rmw.capabilities]` name. Sorted.
///
/// Also the selection's `backend_features`
/// ([`crate::orchestration::facade::EntrySelection`]), which reaches a
/// backend line it did not write (a bridge's) and filters per crate there.
#[must_use]
pub fn derived_features(
    r: &ResolvedRmwIn,
    sys: &SystemToml,
    edition: rosidl_codegen::RosEdition,
) -> Vec<String> {
    let mut features = vec![edition.cargo_feature().to_string()];
    for cap in cargo_nano_ros::capability_resolver::CAPABILITIES {
        if sys.capability_enabled(cap.declared)
            && cap.backend_supports(r)
            && let Some(f) = r.capability_feature(cap.declared)
        {
            features.push(f.to_string());
        }
    }
    features.sort();
    features.dedup();
    features
}

/// `derived` (a backend dependency's edition and capability features, as
/// [`dependency_features`] computes them) followed by each of the board's
/// `platform` features the crate at `dir` declares, without repeats.
///
/// The second half of [`dependency_features`], for a writer that holds an
/// [`ImageBackend`] (whose features were derived before the board was known)
/// rather than the backend itself.
#[must_use]
pub fn with_platform_features(derived: &[String], dir: &Path, platform: &[String]) -> Vec<String> {
    let mut features = derived.to_vec();
    for f in platform {
        if !features.contains(f) && crate_declares_feature(dir, f) {
            features.push(f.clone());
        }
    }
    features
}

/// The `(image)` arm of `__nros_register_backends!`, calling each of
/// `backends` reached as `root` says; empty when none registers.
#[must_use]
pub fn image_arm(backends: &[ImageBackend], root: Root) -> String {
    let calls: Vec<String> = backends
        .iter()
        .filter_map(|b| register_call(b, root))
        .collect();
    if calls.is_empty() {
        return String::new();
    }
    let mut out = format!(
        "    {} {{{{\n",
        nros_orchestration_ir::entry_names::IMAGE_REGISTRATION_ARM
    );
    for c in calls {
        out.push_str(&format!("        let _ = {c};\n"));
    }
    out.push_str("    }};\n");
    out
}

/// `registration` (as [`crate::orchestration::facade`] renders it) with the
/// `(image)` arm added to `__nros_register_backends!`. Unchanged when `arm` is
/// empty, so an image with no D15 backend is byte-identical.
#[must_use]
pub fn with_image_arm(registration: &str, arm: &str) -> String {
    const HEAD: &str = "macro_rules! __nros_register_backends {\n";
    if arm.is_empty() {
        return registration.to_string();
    }
    match registration.find(HEAD) {
        Some(at) => {
            let at = at + HEAD.len();
            format!("{}{arm}{}", &registration[..at], &registration[at..])
        }
        None => registration.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cargo_nano_ros::rmw_resolver::test_support;

    /// The deleted `west_app::backend_dir`, kept here as the oracle ruling 3
    /// must reproduce: a walk of `packages/rmw/<family>/<crate>` for the
    /// descriptor's `rlib_dep`.
    fn deleted_backend_dir(nros_root: &Path, krate: &str) -> Option<PathBuf> {
        if krate.is_empty() {
            return None;
        }
        ["zenoh", "cyclonedds", "xrce", "uorb", "dds"]
            .iter()
            .map(|fam| nros_root.join("packages/rmw").join(fam).join(krate))
            .find(|p| p.join("Cargo.toml").is_file())
    }

    /// Amended ruling 3 — the scan's directory rule gives the deleted walk's
    /// answer for all four in-tree backends: xrce's crate is the sibling
    /// `../nros-rmw-xrce-cffi`, zenoh's is its package directory, and
    /// cyclonedds (no `crate` key) and uorb (no cargo table) have none.
    #[test]
    fn the_crate_directory_is_the_deleted_walks_for_every_in_tree_backend() {
        let root = test_support::checkout();
        let idx = test_support::in_tree_index();
        for rmw in ["zenoh", "xrce", "cyclonedds", "uorb"] {
            let r = idx.resolve(rmw).expect("in-tree");
            let ours = crate_dir(&r).expect("no refusal in-tree");
            let theirs = deleted_backend_dir(&root, &r.descriptor.rlib_dep);
            assert_eq!(
                ours.map(|p| p.canonicalize().unwrap()),
                theirs.map(|p| p.canonicalize().unwrap()),
                "{rmw}"
            );
        }
        let x = idx.resolve("xrce").unwrap();
        assert!(
            crate_dir(&x)
                .unwrap()
                .unwrap()
                .ends_with("packages/rmw/xrce/nros-rmw-xrce-cffi")
        );
        assert!(
            crate_dir(&idx.resolve("cyclonedds").unwrap())
                .unwrap()
                .is_none()
        );
        assert!(crate_dir(&idx.resolve("uorb").unwrap()).unwrap().is_none());
    }

    /// Amended ruling 3 — a `crate` whose directory holds a different package
    /// is the provider's error, refused naming both names.
    #[test]
    fn a_crate_name_mismatch_is_refused() {
        let ws = tempfile::tempdir().unwrap();
        let idx = test_support::acme_index(ws.path());
        let manifest = ws.path().join("src/acme_rmw/Cargo.toml");
        let raw = std::fs::read_to_string(&manifest).unwrap();
        std::fs::write(
            &manifest,
            raw.replace("name = \"acme-rmw\"", "name = \"other-rmw\""),
        )
        .unwrap();
        let e = crate_dir(&idx.resolve("acme").unwrap()).unwrap_err();
        assert!(
            e.contains("crate = \"acme-rmw\"") && e.contains("is `other-rmw`"),
            "{e}"
        );
    }

    /// RFC-0071 D9 on the cargo road, pinned (issue 2393 moved its derivation
    /// into [`dependency_features`], which the west road shares): edition and
    /// capabilities sorted, then the board's platform feature, each only where
    /// the crate declares it.
    #[test]
    fn the_cargo_road_backend_dependency_features_are_pinned() {
        use rosidl_codegen::RosEdition;
        let ws = tempfile::tempdir().unwrap();
        let idx = test_support::acme_index(ws.path());
        let linux = test_support::checkout().join("packages/boards/nros-board-linux");
        let sys = |extra: &str| -> SystemToml {
            toml::from_str(&format!(
                "[system]\nname = \"t\"\nrmw = \"acme\"\ndomain_id = 0\n{extra}\n\
                 [image.linux]\nboard = \"linux\"\n"
            ))
            .unwrap()
        };
        let features = |sys: &SystemToml, edition| {
            let rows = image_backends(
                &idx,
                "linux",
                &["acme".to_string()],
                sys,
                edition,
                Some(&linux),
                &[],
            )
            .unwrap();
            assert_eq!(rows.len(), 1, "{rows:?}");
            let b = rows[0].clone().unwrap();
            let platform = ["platform-posix".to_string(), "platform-zephyr".to_string()];
            (
                b.features.clone(),
                with_platform_features(&b.features, &b.dir, &platform),
            )
        };
        let v = |a: &[&str]| a.iter().map(ToString::to_string).collect::<Vec<_>>();

        let (derived, line) = features(&sys(""), RosEdition::Humble);
        assert_eq!(derived, v(&["ros-humble"]));
        // `platform-zephyr` is not declared by acme, so it is left out.
        assert_eq!(line, v(&["ros-humble", "platform-posix"]));

        let (_, line) = features(&sys(""), RosEdition::Jazzy);
        assert_eq!(line, v(&["ros-jazzy", "platform-posix"]));

        let (derived, line) = features(&sys("features = [\"safety\"]"), RosEdition::Humble);
        assert_eq!(derived, v(&["acme-safety", "ros-humble"]));
        assert_eq!(line, v(&["acme-safety", "ros-humble", "platform-posix"]));

        // The shared function gives the same list in one call.
        let r = idx.resolve("acme").unwrap();
        let dir = crate_dir(&r).unwrap().unwrap();
        assert_eq!(
            dependency_features(
                &r,
                &dir,
                &sys("features = [\"safety\"]"),
                RosEdition::Humble,
                &["platform-posix".to_string()],
            ),
            line
        );
    }

    /// The board links a backend through its `rmw-<x>`, or through a feature
    /// the image selects that enables the backend's crate (mps2's
    /// `xrce-transport`); never an out-of-tree one.
    #[test]
    fn the_board_links_what_its_selected_features_reach() {
        let ws = tempfile::tempdir().unwrap();
        let idx = test_support::acme_index(ws.path());
        let root = test_support::checkout();
        let linux = root.join("packages/boards/nros-board-linux");
        let mps2 = root.join("packages/boards/nros-board-mps2-an385");
        let xrce = idx.resolve("xrce").unwrap();
        assert!(board_links(&linux, &[], &idx.resolve("zenoh").unwrap()));
        assert!(!board_links(&linux, &[], &idx.resolve("acme").unwrap()));
        assert!(!board_links(&mps2, &["serial".into()], &xrce));
        assert!(board_links(&mps2, &["xrce-transport".into()], &xrce));
    }
}
