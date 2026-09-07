// phase-439 W4 — the `nros-rmw.toml` descriptor, read at RUNTIME as well as at
// build time (RFC-0094 D5, issues 1214/1215/1216).
//
// **This module is `include!`d by `build.rs`** as well as compiled into the
// library, exactly like its sibling `serdes_descriptor.rs`, and for the same
// reason: `build.rs` may not use `toml`, which is an ordinary dependency rather
// than a build-dependency, and adding a build-dependency would move
// `Cargo.lock`.
//
// One parser, two callers, on purpose. Until this wave there were two answers
// to "what does `nros-rmw.toml` say": `build.rs`'s private `parse()` (in-tree
// backends only, baked into the binary at compile time) and nothing at all for
// an out-of-tree one — which is issue 1214's whole shape, a provider the scan
// FINDS and the dispatch calls unknown. `resolve_rmw_in` reads a descriptor
// through this module at selection time; `build.rs` reads the in-tree ones
// through it at compile time; there is no second spelling to drift.
//
// Everything here is std-only and dependency-free for that reason. A path is
// never opened here — callers hand in TEXT — so the same functions serve a
// build script walking `packages/rmw/` and a resolver holding a `ScanResult`.

/// The provider kind these descriptors belong to.
pub const RMW_KIND: &str = "rmw";

/// How a backend reaches the final link (RFC-0094 D5).
///
/// This is the fact the root `CMakeLists.txt` used to encode as an `if/elseif`
/// chain on backend NAMES — `zenoh OR xrce` in one arm, `cyclonedds` in the
/// next, and a `FATAL_ERROR` naming a closed set for everything else, which is
/// how `uorb` came to be advertised in the cache drop-down and rejected at
/// configure (issue 1215). A backend declares its strategy; nothing central
/// enumerates backends.
pub const LINK_STRATEGIES: &[&str] = &[LINK_UMBRELLA, LINK_CMAKE];

/// The backend is a Rust rlib BUNDLED into the C/C++ umbrella staticlib
/// (`libnros_c.a` / `libnros_cpp.a`) and anchored there by a `#[used]` static.
/// The root adds nothing to the link line — there is no separate archive.
pub const LINK_UMBRELLA: &str = "umbrella";

/// The backend is a CMake project. The root `add_subdirectory()`s the
/// directory the descriptor names and whole-archives the target it names into
/// both umbrellas, because the backend's registration object is referenced by
/// nothing and a plain archive link would drop it.
pub const LINK_CMAKE: &str = "cmake";

/// What a backend's `nros-rmw.toml` says.
///
/// Split into what convention CANNOT produce (the fields below) and what it
/// can (the `stated_*` fields, which exist only so a descriptor written
/// against the older shape can be checked for agreement rather than silently
/// preferred — RFC-0087 D4).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RmwDescriptor {
    /// `[rmw].cpp_define` — the define `nros-cpp` puts on its INTERFACE.
    /// NOT derivable: spellings are inconsistent across backends by history
    /// (`_CFFI` on two, bare on two) and consumer headers `#if` on them.
    pub cpp_define: String,
    /// The cmake target the backend's own `CMakeLists.txt` creates —
    /// `[rmw.provides.cmake].target`, or the legacy top-level
    /// `[rmw].cmake_target`.
    pub cmake_target: String,
    /// `[rmw.provides.cmake].dir` — the backend's cmake project, relative to
    /// the descriptor. `"."` in every in-tree backend; kept because a provider
    /// may legitimately put its `CMakeLists.txt` in a subdirectory.
    pub cmake_dir: String,
    /// Whether a `[rmw.provides.cmake]` table was present at all.
    pub has_cmake_provision: bool,
    /// `[rmw.link].rlib_dep` — the Rust backend crate bundled into the
    /// umbrella, or empty for a backend that is not Rust.
    pub rlib_dep: String,
    /// `[rmw.link].needs_cxx_linker` — force the C++ linker driver.
    pub needs_cxx_linker: bool,
    /// `[rmw.link].strategy`, or the derivation in [`derive_link_strategy`].
    pub link_strategy: String,
    /// `[rmw.link].c_cffi_feature` — the `nros-c` feature that bundles this
    /// backend. AUTHORED, not derived: the two in-tree values are
    /// `cffi-zenoh-cffi` and `cffi-xrce-c`, irregular by history in the same
    /// way `cpp_define` is, and `nros-c`'s feature table is what a wrong guess
    /// would break. Empty means "this backend is not bundled into `nros-c`",
    /// which is correct for every non-Rust backend.
    pub c_cffi_feature: String,
    /// `[rmw.capabilities]` — capability name -> THIS backend's own feature.
    /// An open vocabulary by design (RFC-0071 D6): core never learns the
    /// right-hand side, so a third-party backend can offer a capability
    /// nano-ros has never heard of.
    pub capabilities: Vec<(String, String)>,
    /// `[rmw.codegen].per_message` — a cmake command run per message type.
    pub per_message_hook: String,

    // ---- restatements of derivable facts (RFC-0087 D4) --------------------
    /// `[rmw].names`, if the descriptor still restates the announcements.
    pub stated_names: Vec<String>,
    pub stated_cargo_feature: String,
    pub stated_cmake_value: String,
    pub stated_c_define_token: String,
    pub stated_cffi_feature: String,
}

/// Parse an `nros-rmw.toml`.
///
/// Line-oriented, matching `serdes_descriptor::parse_serdes_descriptor` and
/// `NanoRosCapabilities.cmake`'s `file(STRINGS … REGEX …)`: the descriptor
/// shape is flat `key = value` under one table, and it is ours.
///
/// `origin` names the file in error messages; it is never opened here.
///
/// Unknown keys are IGNORED rather than rejected, deliberately — the four
/// in-tree descriptors carry `[rmw.provides.cargo]` tables that no consumer
/// reads yet, and a forward-compatible reader is what lets a wave add a field
/// without every older descriptor becoming unparsable. An unknown value for a
/// key that IS read is a different matter and errors below.
pub fn parse_rmw_descriptor(text: &str, origin: &str) -> Result<RmwDescriptor, String> {
    let mut d = RmwDescriptor {
        cmake_dir: ".".to_string(),
        ..RmwDescriptor::default()
    };
    let mut section = String::new();

    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.to_string();
            if section == "rmw.provides.cmake" {
                d.has_cmake_provision = true;
            }
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        let scalar = || value.trim_matches('"').to_string();
        let list = || {
            value
                .trim_start_matches('[')
                .trim_end_matches(']')
                .split(',')
                .map(|x| x.trim().trim_matches('"').to_string())
                .filter(|x| !x.is_empty())
                .collect::<Vec<_>>()
        };
        match (section.as_str(), key) {
            ("rmw", "cpp_define") => d.cpp_define = scalar(),
            ("rmw", "cmake_target") => d.cmake_target = scalar(),
            ("rmw", "names") => d.stated_names = list(),
            ("rmw", "cargo_feature") => d.stated_cargo_feature = scalar(),
            ("rmw", "cmake_value") => d.stated_cmake_value = scalar(),
            ("rmw", "c_define_token") => d.stated_c_define_token = scalar(),
            ("rmw", "cffi_feature") => d.stated_cffi_feature = scalar(),
            ("rmw.provides.cmake", "target") => d.cmake_target = scalar(),
            ("rmw.provides.cmake", "dir") => d.cmake_dir = scalar(),
            ("rmw.link", "rlib_dep") => d.rlib_dep = scalar(),
            ("rmw.link", "needs_cxx_linker") => d.needs_cxx_linker = value == "true",
            ("rmw.link", "strategy") => d.link_strategy = scalar(),
            ("rmw.link", "c_cffi_feature") => d.c_cffi_feature = scalar(),
            ("rmw.capabilities", k) => d.capabilities.push((k.to_string(), scalar())),
            ("rmw.codegen", "per_message") => d.per_message_hook = scalar(),
            _ => {}
        }
    }

    if !d.link_strategy.is_empty() && !LINK_STRATEGIES.contains(&d.link_strategy.as_str()) {
        return Err(format!(
            "{origin}: [rmw.link].strategy is {:?}, which is not one of {} — a typo \
             here would produce a build that links no backend and says nothing",
            d.link_strategy,
            LINK_STRATEGIES.join(" | ")
        ));
    }
    if d.link_strategy.is_empty() {
        d.link_strategy = derive_link_strategy(&d.rlib_dep, &d.cmake_target)
            .ok_or_else(|| {
                format!(
                    "{origin}: cannot tell how this backend reaches the link. Declare \
                     `[rmw.link] rlib_dep = \"<crate>\"` (bundled into the C/C++ \
                     umbrella), or `[rmw.provides.cmake] target = \"<cmake target>\"` \
                     (a CMake project the build add_subdirectory()s), or state \
                     `[rmw.link] strategy` outright."
                )
            })?
            .to_string();
    }

    Ok(d)
}

/// The link strategy a descriptor gets when it states none.
///
/// A Rust backend names the rlib the umbrella bundles, which IS the statement
/// that nothing separate reaches the link line. Anything else that names a
/// cmake target is a CMake project the build must add and whole-archive.
/// Neither: there is no answer, and guessing one produces an image with no
/// backend registered and no diagnostic (issue 0155's failure mode).
#[must_use]
pub fn derive_link_strategy(rlib_dep: &str, cmake_target: &str) -> Option<&'static str> {
    if !rlib_dep.is_empty() {
        Some(LINK_UMBRELLA)
    } else if !cmake_target.is_empty() {
        Some(LINK_CMAKE)
    } else {
        None
    }
}

/// A descriptor may restate a derivable field; it may not disagree with it.
///
/// Empty means "not stated" — after phase-420 W5 the field's whole point is to
/// be absent. `check-derived-descriptor-fields` is the buildless form of this
/// same rule; this is the copy that fires for anyone who builds or resolves.
pub fn agree_with_convention(
    origin: &str,
    field: &str,
    stated: &str,
    derived: &str,
) -> Result<(), String> {
    if !stated.is_empty() && stated != derived {
        return Err(format!(
            "{origin}: [rmw].{field} states {stated:?} but convention derives \
             {derived:?} (RFC-0087 D4). Delete the field — it is derived from the \
             provider's announced name — or fix the name it disagrees with."
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rust_backend_defaults_to_the_umbrella_strategy() {
        let d = parse_rmw_descriptor(
            "[rmw]\ncpp_define = \"NROS_RMW_ACME\"\n[rmw.link]\nrlib_dep = \"nros-rmw-acme\"\n",
            "acme",
        )
        .unwrap();
        assert_eq!(d.link_strategy, LINK_UMBRELLA);
        assert_eq!(d.rlib_dep, "nros-rmw-acme");
        assert!(!d.has_cmake_provision);
    }

    #[test]
    fn a_cmake_backend_defaults_to_the_cmake_strategy() {
        let d = parse_rmw_descriptor(
            "[rmw]\ncpp_define = \"NROS_RMW_ACME\"\n\
             [rmw.provides.cmake]\ndir = \".\"\ntarget = \"nros_rmw_acme\"\n\
             [rmw.link]\nrlib_dep = \"\"\nneeds_cxx_linker = true\n",
            "acme",
        )
        .unwrap();
        assert_eq!(d.link_strategy, LINK_CMAKE);
        assert_eq!(d.cmake_target, "nros_rmw_acme");
        assert_eq!(d.cmake_dir, ".");
        assert!(d.has_cmake_provision);
        assert!(d.needs_cxx_linker);
    }

    #[test]
    fn a_backend_that_names_no_route_to_the_link_is_refused() {
        let err = parse_rmw_descriptor("[rmw]\ncpp_define = \"X\"\n", "acme").unwrap_err();
        assert!(
            err.contains("cannot tell how this backend reaches the link"),
            "{err}"
        );
    }

    #[test]
    fn a_typoed_strategy_is_refused_rather_than_defaulted() {
        let err = parse_rmw_descriptor(
            "[rmw]\ncpp_define = \"X\"\n[rmw.link]\nrlib_dep = \"a\"\nstrategy = \"umbrela\"\n",
            "acme",
        )
        .unwrap_err();
        assert!(err.contains("umbrela"), "{err}");
    }

    #[test]
    fn capabilities_are_an_open_vocabulary() {
        let d = parse_rmw_descriptor(
            "[rmw]\ncpp_define = \"X\"\n[rmw.link]\nrlib_dep = \"a\"\n\
             [rmw.capabilities]\nnever-heard-of-it = \"acme-feature\"\n",
            "acme",
        )
        .unwrap();
        assert_eq!(
            d.capabilities,
            vec![("never-heard-of-it".to_string(), "acme-feature".to_string())]
        );
    }

    #[test]
    fn an_unread_table_does_not_make_a_descriptor_unparsable() {
        // `[rmw.provides.cargo]` is read by nothing today. A reader that
        // rejected it would make every in-tree descriptor unparsable.
        let d = parse_rmw_descriptor(
            "[rmw]\ncpp_define = \"X\"\n[rmw.provides.cargo]\ncrate = \"nros-rmw-acme\"\n\
             [rmw.link]\nrlib_dep = \"nros-rmw-acme\"\n",
            "acme",
        )
        .unwrap();
        assert_eq!(d.link_strategy, LINK_UMBRELLA);
    }

    #[test]
    fn a_restatement_that_disagrees_is_refused() {
        assert!(agree_with_convention("acme", "cmake_value", "acme", "acme").is_ok());
        assert!(agree_with_convention("acme", "cmake_value", "", "acme").is_ok());
        let err = agree_with_convention("acme", "cmake_value", "acme-rmw", "acme").unwrap_err();
        assert!(err.contains("RFC-0087 D4"), "{err}");
    }
}
