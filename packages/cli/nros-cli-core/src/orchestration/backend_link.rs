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
    let Some(table) = crate::configure_stamp::read_external(&dir.join("Cargo.toml"))
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
    // RFC-0071 D7.
    format!(
        "image `{image}`: backend `{rmw}` provides only a cmake target; a Rust-rooted \
         build needs a `sys_crate` bridge. Its `[rmw.provides.cargo]` \
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
            // RFC-0071 D15.
            return Err(format!(
                "image `{image}`: backend `{}` links crate `{krate}` but registers \
                 through `{rc}`, which the image does not depend on. A backend the board \
                 does not link must register through its `[rmw.provides.cargo] crate` \
                 instead.",
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

/// The backend features the image's enabled capabilities name on backend `r`,
/// by `r`'s `[rmw.capabilities]`: the capability half of [`derived_features`],
/// without the edition. Also what a board crate is asked to forward (issue
/// 2423), since a board image reaches its backend through the board.
#[must_use]
pub fn capability_features(r: &ResolvedRmwIn, sys: &SystemToml) -> Vec<String> {
    cargo_nano_ros::capability_resolver::CAPABILITIES
        .iter()
        .filter(|cap| sys.capability_enabled(cap.declared) && cap.backend_supports(r))
        .filter_map(|cap| r.capability_feature(cap.declared).map(str::to_string))
        .collect()
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
    features.extend(capability_features(r, sys));
    features.sort();
    features.dedup();
    features
}

/// The refusal for an image that declares a capability backend `r` does not
/// carry, or `None` when `r` carries every declared capability.
///
/// Issue 2487: a capability with a backend half (`backend_feature`) that the
/// backend's `[rmw.capabilities]` does not declare used to be OMITTED with a
/// warning, so the image built without what it asked for. For `safety` that
/// is a fail-open: the validated-subscription API still compiles in (a node
/// package may name `nros/safety-e2e` itself), the backend's default
/// `take_validated` makes no check, and every sample reaches the application
/// unverified. Every road that lowers a capability onto a backend asks this
/// ONE predicate, so a declared capability is either built or refused, never
/// dropped. The message carries no tracker id (user ruling 2026-11-12).
#[must_use]
pub fn undeclared_capability_refusal(
    rmws: &RmwIndex,
    r: &ResolvedRmwIn,
    sys: &SystemToml,
) -> Option<String> {
    let missing: Vec<&str> = cargo_nano_ros::capability_resolver::CAPABILITIES
        .iter()
        .filter(|cap| {
            sys.capability_enabled(cap.declared)
                && cap.backend_feature.is_some()
                && !cap.backend_supports(r)
        })
        .map(|cap| cap.declared)
        .collect();
    if missing.is_empty() {
        return None;
    }
    let lines: Vec<String> = missing
        .iter()
        .map(|cap| {
            let carriers = rmws.backends_declaring(cap);
            let who = if carriers.is_empty() {
                "no available RMW carries it".to_string()
            } else {
                format!(
                    "RMWs that carry it: {}",
                    carriers
                        .iter()
                        .map(|c| format!("`{c}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            format!(
                "capability `{cap}` is declared, but the `{}` RMW does not carry it, \
                 so the image would be built without it ({who}). Select an RMW that \
                 carries `{cap}`, or drop `{cap}` from the features.",
                r.declared
            )
        })
        .collect();
    Some(lines.join("\n"))
}

// Issue 2704 -- the axis -> package table is not spelled here: it is the
// one `nros_orchestration_ir::runtime_interface_types` table
// (`data/runtime-interface-types.json`), which
// `nros_runtime_interface_types()` (`cmake/NanoRosRuntimeInterfaceTypes.cmake`)
// reads too, so the refusal and the cmake roads cannot disagree.

/// The `[rmw.capabilities]` key of a backend that creates an entity only for
/// a type whose descriptor is generated into the image.
const TYPE_DESCRIPTORS: &str = "type-descriptors";

/// The refusal for an image whose backend `r` needs a generated descriptor for
/// every type, when an enabled node interface axis needs a runtime package
/// that `find_pkg` cannot resolve; `None` when nothing is missing.
///
/// Issue 2704: a Zephyr image on Cyclone with `param_services` booted without
/// its six parameter services (`Transport(Unsupported)` at the first spin,
/// logged once, `ros2 param` unable to reach the node). Every road now
/// generates the runtime's own types (`nros_runtime_interface_types()`); this
/// is the case where it CANNOT, refused before any build, so an enabled
/// interface is either served or refused, never missing at run time. The
/// sibling of [`undeclared_capability_refusal`] (issue 2487): that one asks
/// whether the backend carries a capability, this one whether the image can
/// be given the types the capability's entities are made of. Image-level,
/// not per node: the descriptor table is the image's, linked once, and the
/// runtime packages are the same for every node that enables the axis.
/// No tracker id in the text (user ruling 2026-11-12).
#[must_use]
pub fn runtime_interface_types_refusal(
    r: &ResolvedRmwIn,
    sys: &SystemToml,
    find_pkg: &dyn Fn(&str) -> bool,
) -> Option<String> {
    if !r
        .descriptor
        .capabilities
        .iter()
        .any(|(k, _)| k == TYPE_DESCRIPTORS)
    {
        return None;
    }
    let lines: Vec<String> = nros_orchestration_ir::runtime_interface_types::table()
        .axes
        .iter()
        .map(|a| (a.axis.as_str(), &a.packages))
        .filter(|(axis, _)| sys.capability_enabled(axis))
        .filter_map(|(axis, pkgs)| {
            let missing: Vec<&str> = pkgs
                .iter()
                .map(String::as_str)
                .filter(|p| !find_pkg(p))
                .collect();
            if missing.is_empty() {
                return None;
            }
            let named = missing
                .iter()
                .map(|p| format!("`{p}`"))
                .collect::<Vec<_>>()
                .join(", ");
            let stock = nros_orchestration_ir::node_interfaces::STOCK.contains(&axis);
            let (how, way_out) = if stock {
                (
                    "is enabled (by the image's stock node interfaces, or declared in its \
                     features)",
                    format!(
                        "set `node_interfaces = \"none\"` (or drop `{axis}` from the \
                         features if you declared it)"
                    ),
                )
            } else {
                ("is declared", format!("drop `{axis}` from the features"))
            };
            Some(format!(
                "capability `{axis}` {how}, but the `{}` RMW creates an entity only for a \
                 type whose descriptor is generated into the image, and the interface \
                 package(s) the runtime makes `{axis}`'s entities from, {named}, are not on \
                 the interface search path (NROS_INTERFACE_SEARCH_PATH, AMENT_PREFIX_PATH), \
                 so the image would boot without them. Make {named} discoverable, or \
                 {way_out}.",
                r.declared
            ))
        })
        .collect();
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

/// Whether interface package `name` is on the interface index over `search`
/// (the run's interface search path, then the ament index, then the bundled
/// copies -- the layers `nros configure` generates from). The `find_pkg` the
/// roads hand [`runtime_interface_types_refusal`].
#[must_use]
pub fn interface_package_on_index(search: Option<&std::ffi::OsStr>, name: &str) -> bool {
    crate::cmd::ws::interface_index(search).is_some_and(|i| i.find_package(name).is_some())
}

/// The capability axes whose runtime half creates SERVICES on each node: the
/// six parameter services, and the five lifecycle services.
pub const SERVICE_AXES: &[&str] = &["param_services", "lifecycle"];

/// The refusal for an image whose nodes would need services the backend
/// cannot create, or `None` when the backend can (or does not say).
///
/// Phase-556 W13 made the stock node interfaces (the parameter services among
/// them) the default for every image. A backend whose service slots all
/// answer "unsupported" (`[rmw.facts] services = false`, a MEASURED fact; an
/// absent fact is unknown and refuses nothing) used to boot such a node with a
/// one-line runtime error and no parameter services, so `ros2 param` reached
/// nothing. Refused here instead, on every road that asks
/// [`undeclared_capability_refusal`].
///
/// Per node: each component the image declares runs on the image's backend
/// `r`, and each is named. A node a component opens on ANOTHER backend in its
/// own code (`NodeBuilder::rmw`) is invisible to the model, so it is neither
/// refused nor passed here — today's behaviour, never a guess. The message
/// carries no tracker id (user ruling 2026-11-12).
#[must_use]
pub fn unservable_services_refusal(
    image: &str,
    r: &ResolvedRmwIn,
    sys: &SystemToml,
) -> Option<String> {
    if r.states_fact("services") != Some(false) {
        return None;
    }
    let axes: Vec<&str> = SERVICE_AXES
        .iter()
        .copied()
        .filter(|a| sys.capability_enabled(a))
        .collect();
    if axes.is_empty() {
        return None;
    }
    let nodes: Vec<String> = if sys.components.is_empty() {
        vec![format!("the nodes of image `{image}`")]
    } else {
        sys.components
            .iter()
            .map(|c| format!("node `{}` of image `{image}`", c.name))
            .collect()
    };
    let lines: Vec<String> = nodes
        .iter()
        .flat_map(|node| {
            axes.iter().map(move |cap| {
                format!(
                    "{node}: capability `{cap}` is enabled (by the image's stock node \
                     interfaces, or declared in its features), but the `{}` RMW cannot \
                     create services, so the node would boot without them and `ros2` \
                     could not reach them. Set `node_interfaces = \"none\"` (or drop \
                     `{cap}` from the features if you declared it), or select an RMW that \
                     serves services.",
                    r.declared
                )
            })
        })
        .collect();
    Some(lines.join("\n"))
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

    /// A user who opts in to the bundled interfaces (the search path names
    /// `packages/cli/interfaces/`, ROS absent) must find every package the
    /// runtime makes a node interface from, or a backend that needs a
    /// generated descriptor for every type (Cyclone) refuses the image: the
    /// CLI here, and the cmake road at configure. Both read the bundled root
    /// as a directory of `<pkg>/package.xml`, so the table's packages must
    /// each be one. Env-free: the bundled index alone, no ament.
    /// The same opt-in, through the resolver the cmake road configures with
    /// (`_nros_find_msg_package_root`): with no ROS (no AMENT_PREFIX_PATH)
    /// and the bundled root on the search path, every table package resolves
    /// to its bundled copy. Negative control: without the opt-in the bundled
    /// copies are refused, so the script is answering the opt-in question
    /// and not passing whatever it is asked.
    #[test]
    fn runtime_interface_packages_resolve_from_the_opted_in_bundle() {
        let root = test_support::checkout();
        let bundled = root.join("packages/cli/interfaces");
        let script = root.join("scripts/check/runtime-interface-bundle-resolution.cmake");
        let run = |opt_in: bool| {
            let mut c = std::process::Command::new("cmake");
            c.arg("-P").arg(&script).env_remove("AMENT_PREFIX_PATH");
            if opt_in {
                c.env("NROS_INTERFACE_SEARCH_PATH", &bundled);
            } else {
                c.env_remove("NROS_INTERFACE_SEARCH_PATH");
            }
            let out = c
                .output()
                .expect("running cmake -P (cmake is a build requirement of this repo)");
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            (out.status.success(), text)
        };
        let (ok, text) = run(true);
        assert!(ok, "opted in, ROS absent:\n{text}");
        for p in &nros_orchestration_ir::runtime_interface_types::table().packages {
            let want = format!("{}={}", p.name, bundled.join(&p.name).display());
            assert!(text.contains(&want), "{want} missing:\n{text}");
        }
        let (ok, text) = run(false);
        assert!(
            !ok && text
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .contains("does not generate it from its bundled copy"),
            "not opted in, ROS absent, the bundle must be refused:\n{text}"
        );
    }

    #[test]
    fn every_runtime_interface_package_is_bundled() {
        let bundled = Path::new(env!("CARGO_MANIFEST_DIR")).join("../interfaces");
        let idx = rosidl_bindgen::ament::AmentIndex::from_directory(&bundled)
            .expect("the bundled interfaces load");
        let table = nros_orchestration_ir::runtime_interface_types::table();
        assert!(
            !table.packages.is_empty(),
            "the table names no package, so this proves nothing"
        );
        let missing: Vec<&str> = table
            .packages
            .iter()
            .map(|p| p.name.as_str())
            .filter(|p| idx.find_package(p).is_none())
            .collect();
        assert!(
            missing.is_empty(),
            "not bundled under {}: {missing:?}",
            bundled.display()
        );
    }

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

    /// Issue 2487 — a declared capability the backend does not carry is
    /// refused, naming the backend and the backends that carry it, and the
    /// refusal carries no tracker id. A backend that carries it is not refused.
    #[test]
    fn a_capability_the_backend_does_not_carry_is_refused() {
        let idx = test_support::in_tree_index();
        let sys = |rmw: &str, features: &str| -> SystemToml {
            toml::from_str(&format!(
                "[system]\nname = \"t\"\nrmw = \"{rmw}\"\ndomain_id = 0\n\
                 features = [{features}]\n"
            ))
            .unwrap()
        };
        for rmw in ["cyclonedds", "xrce"] {
            let r = idx.resolve(rmw).unwrap();
            let e = undeclared_capability_refusal(&idx, &r, &sys(rmw, "\"safety\""))
                .unwrap_or_else(|| panic!("`safety` on {rmw} must be refused"));
            assert!(e.contains(&format!("`{rmw}` RMW")), "{e}");
            assert!(e.contains("capability `safety`"), "{e}");
            assert!(e.contains("`zenoh`"), "the carriers are named: {e}");
            for id in ["issue", "RFC-", "phase-", "#"] {
                assert!(!e.contains(id), "no tracker id in a user message: {e}");
            }
            // The other backend-half axis takes the same predicate.
            assert!(
                undeclared_capability_refusal(&idx, &r, &sys(rmw, "\"link-tls\"")).is_some(),
                "`link-tls` on {rmw}"
            );
            // Entry-umbrella-only axes have no backend half: never refused.
            assert_eq!(
                undeclared_capability_refusal(&idx, &r, &sys(rmw, "\"params\"")),
                None
            );
        }
        let z = idx.resolve("zenoh").unwrap();
        assert_eq!(
            undeclared_capability_refusal(&idx, &z, &sys("zenoh", "\"safety\"")),
            None
        );
    }

    /// Issue 2704 — on a backend that creates entities only for generated
    /// descriptors, an enabled node interface whose runtime package cannot be
    /// found is refused, naming the axis, the package and the way out; a
    /// findable package, or a backend with no descriptor table, is not.
    #[test]
    fn runtime_interface_types_unprovidable_is_refused() {
        let idx = test_support::in_tree_index();
        let sys = |rmw: &str, features: &str| -> SystemToml {
            toml::from_str(&format!(
                "[system]\nname = \"t\"\nrmw = \"{rmw}\"\ndomain_id = 0\n\
                 features = [{features}]\n"
            ))
            .unwrap()
        };
        let none = |_: &str| false;
        let all = |_: &str| true;
        let c = idx.resolve("cyclonedds").unwrap();
        let e =
            runtime_interface_types_refusal(&c, &sys("cyclonedds", "\"param_services\""), &none)
                .expect("param_services on cyclonedds with no rcl_interfaces must be refused");
        assert!(e.contains("capability `param_services` is enabled"), "{e}");
        assert!(e.contains("`rcl_interfaces`"), "{e}");
        assert!(e.contains("`cyclonedds` RMW"), "{e}");
        assert!(e.contains("node_interfaces = \"none\""), "{e}");
        for id in ["issue", "RFC-", "phase-", "#"] {
            assert!(!e.contains(id), "no tracker id in a user message: {e}");
        }
        let l = runtime_interface_types_refusal(&c, &sys("cyclonedds", "\"lifecycle\""), &none)
            .expect("lifecycle on cyclonedds with no lifecycle_msgs must be refused");
        assert!(
            l.contains("`lifecycle_msgs`") && l.contains("drop `lifecycle`"),
            "{l}"
        );
        assert!(
            !l.contains("node_interfaces"),
            "lifecycle is no stock interface: {l}"
        );
        assert_eq!(
            runtime_interface_types_refusal(
                &c,
                &sys("cyclonedds", "\"param_services\", \"rosout\""),
                &all
            ),
            None
        );
        assert_eq!(
            runtime_interface_types_refusal(&c, &sys("cyclonedds", ""), &none),
            None
        );
        let z = idx.resolve("zenoh").unwrap();
        assert_eq!(
            runtime_interface_types_refusal(&z, &sys("zenoh", "\"param_services\""), &none),
            None
        );
    }

    /// A node that would serve services on a backend that MEASURED it cannot
    /// create them is refused: per node, naming the node, the image, the
    /// backend, the axis and the way out. The stock default reaches it too
    /// (nothing declared), `node_interfaces = "none"` clears it, and a backend
    /// that states nothing (unknown) is never refused.
    #[test]
    fn a_node_whose_backend_cannot_create_services_is_refused() {
        let idx = test_support::in_tree_index();
        let sys = |rmw: &str, extra: &str| -> SystemToml {
            toml::from_str(&format!(
                "[system]\nname = \"t\"\nrmw = \"{rmw}\"\ndomain_id = 0\n{extra}\n\
                 [[component]]\npkg = \"p\"\nclass = \"p::A\"\nname = \"alpha\"\n\
                 [[component]]\npkg = \"p\"\nclass = \"p::B\"\nname = \"beta\"\n"
            ))
            .unwrap()
        };
        let uorb = idx.resolve("uorb").unwrap();
        assert_eq!(
            uorb.states_fact("services"),
            Some(false),
            "the uORB descriptor states the measured fact"
        );

        // The stock default: nothing declared, both nodes refused.
        let e = unservable_services_refusal("img", &uorb, &sys("uorb", ""))
            .expect("stock param_services on uORB must be refused");
        for want in [
            "node `alpha` of image `img`",
            "node `beta` of image `img`",
            "capability `param_services` is enabled",
            "`uorb` RMW cannot create services",
            "node_interfaces = \"none\"",
        ] {
            assert!(e.contains(want), "missing {want:?} in: {e}");
        }
        for id in ["issue", "RFC-", "phase-", "#"] {
            assert!(!e.contains(id), "no tracker id in a user message: {e}");
        }
        // Lifecycle creates services too.
        let e = unservable_services_refusal(
            "img",
            &uorb,
            &sys(
                "uorb",
                "node_interfaces = \"none\"\nfeatures = [\"lifecycle\"]",
            ),
        )
        .expect("lifecycle on uORB must be refused");
        assert!(
            e.contains("capability `lifecycle`") && !e.contains("`param_services`"),
            "{e}"
        );
        // The way out works.
        assert_eq!(
            unservable_services_refusal("img", &uorb, &sys("uorb", "node_interfaces = \"none\"")),
            None
        );
        // A backend that does not state the fact is unknown, never refused.
        for rmw in ["zenoh", "xrce", "cyclonedds"] {
            let r = idx.resolve(rmw).unwrap();
            assert_ne!(
                r.states_fact("services"),
                Some(false),
                "{rmw} states no false fact"
            );
            assert_eq!(
                unservable_services_refusal("img", &r, &sys(rmw, "")),
                None,
                "{rmw}"
            );
        }
    }

    /// The parity table's rows: `(node_interfaces, declared features)`. Each
    /// reaches the refusals' axes the way an image does: the stock switch for
    /// the parameter services, a declaration for the rest, plus axes neither
    /// road refuses as controls. `link-tls` is left out on purpose: the cmake
    /// road refuses it on EVERY backend (no C/C++ TLS link exists), the Rust
    /// road only where no backend carries it, so the two answers differ by
    /// design, not by drift.
    const PARITY_ROWS: &[(&str, &[&str])] = &[
        ("stock", &[]),
        ("none", &["param_services"]),
        ("none", &["lifecycle"]),
        ("none", &["safety"]),
        ("none", &["params"]),
        ("none", &["rosout"]),
        ("none", &[]),
    ];

    /// The axes one refusal text refuses: every "capability `X`" or
    /// "capability 'X'" in it (cmake's 2487 message quotes with `'`).
    fn refused_axes(text: &str) -> std::collections::BTreeSet<String> {
        let mut out = std::collections::BTreeSet::new();
        for (i, _) in text.match_indices("capability ") {
            let rest = &text[i + "capability ".len()..];
            let Some(q) = rest.chars().next().filter(|c| *c == '`' || *c == '\'') else {
                continue;
            };
            if let Some(end) = rest[1..].find(q) {
                out.insert(rest[1..=end].to_string());
            }
        }
        out
    }

    fn squash(s: &str) -> String {
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// One row as the image's `system.toml` would state it.
    fn row_sys(rmw: &str, node_interfaces: &str, declared: &[&str]) -> SystemToml {
        let list = declared
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(", ");
        toml::from_str(&format!(
            "[system]\nname = \"t\"\nrmw = \"{rmw}\"\ndomain_id = 0\n\
             node_interfaces = \"{node_interfaces}\"\nfeatures = [{list}]\n\
             [[component]]\npkg = \"p\"\nclass = \"p::A\"\nname = \"alpha\"\n"
        ))
        .unwrap()
    }

    /// The Rust road's answer for one row: the axes refused, and the text.
    fn rust_answer(
        idx: &RmwIndex,
        r: &ResolvedRmwIn,
        sys: &SystemToml,
    ) -> (std::collections::BTreeSet<String>, String) {
        let text = [
            undeclared_capability_refusal(idx, r, sys),
            unservable_services_refusal("img", r, sys),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n");
        (refused_axes(&text), text)
    }

    /// The cmake road's answer for one row, through
    /// `scripts/check/capability-refusal-parity.cmake`, with the facts the CLI
    /// exports (`crate::cmd::ws::dispatch_rows`), never a second parse.
    fn cmake_answer(
        r: &ResolvedRmwIn,
        caps: &[String],
    ) -> (std::collections::BTreeSet<String>, String) {
        let rows = crate::cmd::ws::dispatch_rows(r);
        let row = |k: &str| {
            rows.iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.clone())
                .unwrap_or_else(|| panic!("dispatch rows carry no {k}"))
        };
        let script = test_support::checkout().join("scripts/check/capability-refusal-parity.cmake");
        let out = std::process::Command::new("cmake")
            .arg(format!("-DRMW={}", r.declared))
            .arg(format!("-DCAPS={}", caps.join(";")))
            .arg(format!(
                "-DRMW_CAPABILITIES={}",
                row("NROS_RMW_CAPABILITIES")
            ))
            .arg(format!("-DRMW_FACTS={}", row("NROS_RMW_FACTS")))
            .arg("-P")
            .arg(&script)
            .output()
            .expect("running cmake -P (cmake is a build requirement of this repo)");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let accepted = out.status.success() && text.contains("ACCEPTED");
        let axes = if accepted {
            Default::default()
        } else {
            refused_axes(&text)
        };
        assert!(
            accepted || !axes.is_empty(),
            "cmake neither accepted nor named an axis for {} {caps:?}:\n{text}",
            r.declared
        );
        (axes, text)
    }

    /// The two capability refusals are spelled twice, in Rust and in cmake,
    /// and nothing held the two together: one table, both spellings, one
    /// answer. Every in-tree backend x one axis per row (cmake stops at its
    /// first refusal, so one axis per row is what both can answer). For the
    /// services refusal the cmake text must carry the Rust message body word
    /// for word, after its prefix. A disagreement names the row and the side.
    #[test]
    fn capability_refusal_parity() {
        let idx = test_support::in_tree_index();
        let mut bad = Vec::new();
        let mut refused_rows = 0;
        for rmw in ["zenoh", "xrce", "cyclonedds", "uorb"] {
            let r = idx.resolve(rmw).unwrap();
            for (ni, declared) in PARITY_ROWS {
                let sys = row_sys(rmw, ni, declared);
                // What the CLI hands cmake: the image's EFFECTIVE features.
                let caps = sys.effective_features();
                let label = format!("({rmw}, node_interfaces = {ni}, features = {declared:?})");
                let (rust, rust_text) = rust_answer(&idx, &r, &sys);
                let (cmake, cmake_text) = cmake_answer(&r, &caps);
                if !rust.is_empty() {
                    refused_rows += 1;
                }
                // cmake stops at its FIRST refusal, Rust names every one: they
                // agree when both refuse nothing, or cmake's axis is one Rust names.
                let agree = if rust.is_empty() {
                    cmake.is_empty()
                } else {
                    cmake.len() == 1 && cmake.is_subset(&rust)
                };
                if !agree {
                    bad.push(format!(
                        "row {label}: the Rust road refuses {rust:?}, the cmake road \
                         refuses {cmake:?}\n  rust: {rust_text}\n  cmake: {}",
                        squash(&cmake_text)
                    ));
                    continue;
                }
                if cmake.contains("param_services") || cmake.contains("lifecycle") {
                    let axis = cmake.iter().next().unwrap();
                    let line = rust_text
                        .lines()
                        .find(|l| l.contains(&format!("capability `{axis}`")))
                        .unwrap_or_default();
                    let body = line.split_once(": ").map_or(line, |(_, b)| b);
                    if !squash(&cmake_text).contains(&squash(body)) {
                        bad.push(format!(
                            "row {label}: both refuse `{axis}`, but the cmake text does not \
                             carry the Rust body\n  rust body: {body}\n  cmake: {}",
                            squash(&cmake_text)
                        ));
                    }
                }
            }
        }
        assert!(
            bad.is_empty(),
            "the two roads disagree:\n{}",
            bad.join("\n")
        );
        // Negative control: the table must actually reach a refusal on both
        // refusals' axes, or "they agree" is two roads that never refuse.
        assert!(
            refused_rows >= 3,
            "only {refused_rows} refused rows: the table proves nothing"
        );
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
