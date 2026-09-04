//! Serialization-format selection lowering — phase-421 W4, RFC-0088 D6.
//!
//! The sibling of [`crate::rmw_resolver`], and deliberately its shape: a
//! declared name is validated and **lowered** to each language's build
//! mechanism — a cargo feature, a CMake value, a C `#define` token — and an
//! unknown name is an error that lists what IS available.
//!
//! Two differences from the RMW resolver, both consequences of RFC-0087 D4 and
//! RFC-0088 D6:
//!
//! * **The names live in the announcement, not the descriptor.** A provider
//!   says `<nano_ros_provides kind="serdes" name="cdr"/>` in its `package.xml`;
//!   `nros-serdes.toml` carries only `impl` and `format_id`, the two facts no
//!   convention can produce. Everything else is derived — see
//!   [`crate::serdes_descriptor`].
//! * **There is an out-of-repo path.** [`resolve_serdes`] answers from the
//!   generated in-tree table (no I/O, `'static`); [`resolve_serdes_in`] answers
//!   from a provider scan over the search path, which is what makes a provider
//!   package living outside this repo selectable by name.
//!
//! The default is `cdr` ([`DEFAULT_SERDES_NAME`]): a build that declares no
//! format behaves exactly as it does today.

use std::{fmt, path::PathBuf};

use crate::{
    package_xml::PackageXml,
    provider_scan::{ResolveError, ScanResult, resolve_unique},
    serdes_descriptor::{
        DEFAULT_SERDES_NAME, cargo_package_name, parse_serdes_descriptor, serdes_c_define_token,
        serdes_cargo_feature, serdes_cmake_value,
    },
};

include!(concat!(env!("OUT_DIR"), "/serdes_table.rs"));

/// The provider family name — the `kind=` of the announcement and the `<kind>`
/// of `nros-<kind>.toml`. One constant so the string is not retyped at each of
/// the three sites that must agree.
pub const SERDES_KIND: &str = "serdes";

/// The serialization formats THIS CHECKOUT provides, derived from the in-tree
/// descriptors plus their announcements.
///
/// Not the whole answer: a provider in the user's workspace or anywhere else on
/// the search path is found by [`resolve_serdes_in`] and does not appear here.
/// This is "what nano-ros ships", used for the default and for error text when
/// no scan is available.
#[must_use]
pub fn known_serdes() -> Vec<&'static str> {
    SERDES_ROWS.iter().map(|r| r.declared).collect()
}

/// A declared serdes value lowered to its per-language build forms.
///
/// Owned `String`s rather than `&'static str` (the RMW resolver's choice)
/// because an out-of-repo provider's name is read from a file at selection
/// time and cannot be `'static`. The RMW resolver could stay `'static` only
/// because it has no out-of-tree path at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSerdes {
    /// The canonical declared name, e.g. `"cdr"`.
    pub declared: String,
    /// The crate that implements it — DERIVED from the provider's `Cargo.toml`.
    pub crate_name: String,
    /// The cargo feature the selection lowers to, `serdes-<name>`.
    pub cargo_feature: String,
    /// The `-DNANO_ROS_SERDES` CMake value — the canonical name.
    pub cmake_value: String,
    /// The `#define NROS_SERIALIZATION_FORMAT_<TOKEN>` token, `UPPER(name)`.
    pub c_define_token: String,
    /// `"schema"` | `"codegen"` (RFC-0088 D7). From the descriptor.
    pub impl_strategy: String,
    /// The image-local `u8` discriminant, when the provider states one
    /// (RFC-0088 D2). `None` ⇒ the build assigns it from the set of formats the
    /// image declares. **Never an identity across images** — the name is.
    pub format_id: Option<u8>,
    /// Where the provider package was found. `None` for a row that came from
    /// the compiled-in table, which records no path.
    pub package_dir: Option<PathBuf>,
}

/// A declared serdes value no provider claims.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownSerdes {
    pub declared: String,
    /// The names that ARE claimed, sorted — an unknown name is usually a typo,
    /// and the list is the fix.
    pub available: Vec<String>,
}

impl fmt::Display for UnknownSerdes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown serdes `{}`", self.declared)?;
        if self.available.is_empty() {
            write!(f, " — no serdes providers were discovered at all")
        } else {
            write!(f, " (known: {})", self.available.join(", "))
        }
    }
}

impl std::error::Error for UnknownSerdes {}

/// Why a search-path resolution did not produce exactly one provider.
#[derive(Debug)]
pub enum SerdesResolveError {
    /// The scan claims no such name, or two packages in one root claim it.
    Scan(ResolveError),
    /// The provider was found but could not be lowered — an unreadable or
    /// malformed descriptor, or a missing `Cargo.toml` to derive the crate
    /// from. A provider that cannot be lowered is not a provider.
    Provider { dir: PathBuf, message: String },
}

impl fmt::Display for SerdesResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SerdesResolveError::Scan(e) => write!(f, "{e}"),
            SerdesResolveError::Provider { dir, message } => {
                write!(f, "serdes provider at {}: {message}", dir.display())
            }
        }
    }
}

impl std::error::Error for SerdesResolveError {}

/// The canonical serdes name from any announced spelling. `None` for unknown.
#[must_use]
pub fn canonical_serdes(input: &str) -> Option<&'static str> {
    SERDES_ROWS
        .iter()
        .find(|r| r.names.contains(&input))
        .map(|r| r.declared)
}

/// Lower a declared serdes string against the IN-TREE table.
///
/// The no-I/O answer, for callers with no provider scan in hand. An out-of-repo
/// provider is invisible here by construction — use [`resolve_serdes_in`].
pub fn resolve_serdes(declared: &str) -> Result<ResolvedSerdes, UnknownSerdes> {
    SERDES_ROWS
        .iter()
        .find(|r| r.names.contains(&declared))
        .map(|r| ResolvedSerdes {
            declared: r.declared.to_string(),
            crate_name: r.crate_name.to_string(),
            cargo_feature: r.cargo_feature.to_string(),
            cmake_value: r.cmake_value.to_string(),
            c_define_token: r.c_define_token.to_string(),
            impl_strategy: r.impl_strategy.to_string(),
            format_id: r.format_id,
            package_dir: None,
        })
        .ok_or_else(|| UnknownSerdes {
            declared: declared.to_string(),
            available: known_serdes().iter().map(|s| (*s).to_string()).collect(),
        })
}

/// Lower a declared serdes string against a PROVIDER SCAN.
///
/// This is the path RFC-0088 D6 is about: the scan walks the search path, and
/// the nano-ros tree is simply its first root, so an in-tree provider and one
/// in the user's workspace — or any other root a caller puts on the path — are
/// found by the same code. A later root overlays an earlier one
/// ([`resolve_unique`]), so a user's `cdr` shadows ours rather than colliding
/// with it.
///
/// The descriptor is read HERE and only here: the scan reads `package.xml`
/// alone, so one cheap parse per package and one detailed parse per build.
/// A provider with no `nros-serdes.toml` at all is legal and gets every default
/// (RFC-0088 D6: "optional; absent means every default applies").
pub fn resolve_serdes_in(
    scan: &ScanResult,
    declared: &str,
) -> Result<ResolvedSerdes, SerdesResolveError> {
    let resolution =
        resolve_unique(scan, SERDES_KIND, declared).map_err(SerdesResolveError::Scan)?;
    let pkg = resolution.winner;

    // The CANONICAL name is the provider's first `serdes` announcement, not the
    // string the consumer typed: a provider announcing `cdr` and `ros-cdr` has
    // one canonical spelling, and the lowering must not depend on which alias
    // the consumer reached it by.
    let canonical = pkg
        .provides
        .iter()
        .find(|p| p.kind == SERDES_KIND)
        .map(|p| p.name.clone())
        .ok_or_else(|| SerdesResolveError::Provider {
            dir: pkg.dir.clone(),
            message: "resolved as a serdes provider but announces no serdes provision".to_string(),
        })?;

    let fail = |message: String| SerdesResolveError::Provider {
        dir: pkg.dir.clone(),
        message,
    };

    let descriptor_path = pkg.descriptor_path(SERDES_KIND);
    let descriptor = if descriptor_path.is_file() {
        let text = std::fs::read_to_string(&descriptor_path)
            .map_err(|e| fail(format!("read {}: {e}", descriptor_path.display())))?;
        parse_serdes_descriptor(&text, &descriptor_path.display().to_string()).map_err(fail)?
    } else {
        // RFC-0088 D6 — a provider with nothing non-derivable to say needs no
        // descriptor. `impl` defaults to `schema`, `format_id` to unassigned.
        parse_serdes_descriptor("", &descriptor_path.display().to_string())
            .expect("the empty descriptor is the default descriptor")
    };

    let manifest = pkg.dir.join("Cargo.toml");
    let crate_name = std::fs::read_to_string(&manifest)
        .map_err(|e| {
            fail(format!(
                "read {}: {e} — the serdes `crate` is DERIVED from the provider's \
                 Cargo.toml (RFC-0087 D4), so a provider without one cannot be linked",
                manifest.display()
            ))
        })
        .and_then(|text| {
            cargo_package_name(&text)
                .ok_or_else(|| fail(format!("{}: no [package] name", manifest.display())))
        })?;

    Ok(ResolvedSerdes {
        cargo_feature: serdes_cargo_feature(&canonical),
        cmake_value: serdes_cmake_value(&canonical),
        c_define_token: serdes_c_define_token(&canonical),
        declared: canonical,
        crate_name,
        impl_strategy: descriptor.impl_strategy,
        format_id: descriptor.format_id,
        package_dir: Some(pkg.dir),
    })
}

/// The serdes a package declares FOR ITSELF, from
/// `<nano_ros_uses kind="serdes" name="…"/>` (phase-420 W1).
///
/// The first one wins if a package writes several: ROS 2's semantic is one
/// image, one encoding (RFC-0088), so several selections is an authoring error
/// rather than a set — and it is [`crate::rmw_resolver`]'s situation exactly,
/// where no package declares two backends either.
#[must_use]
pub fn package_declared_serdes(pkg: &PackageXml) -> Option<&str> {
    pkg.uses_of_kind(SERDES_KIND)
        .next()
        .map(|u| u.name.as_str())
}

/// The declared serdes name for one package in one system, or the default.
///
/// **The ladder, and where it comes from.** `rmw`'s is `--rmw`, then
/// `[image.<t>]`, then the deprecated `[deploy.<t>]`, then `[system]`, then a
/// built-in default (`SystemToml::resolved_rmw`). This mirrors it, with the
/// package's own `<nano_ros_uses>` between the CLI flag and the system answer:
///
/// 1. an explicit CLI override;
/// 2. the package's own `<nano_ros_uses kind="serdes"/>`;
/// 3. the system-level answer (`SystemToml::resolved_serdes`, which is itself
///    image > deploy > system);
/// 4. [`DEFAULT_SERDES_NAME`].
///
/// Rung 2 sits above rung 3 on the same principle that puts `[image.<t>]` above
/// `[system]` in the RMW ladder: the more specific declaration wins. RFC-0088
/// orders neither against the other, so this states the choice rather than
/// leaving it to whichever caller asks first.
#[must_use]
pub fn declared_serdes(
    cli: Option<&str>,
    package: Option<&PackageXml>,
    system: Option<&str>,
) -> String {
    cli.map(str::to_string)
        .or_else(|| {
            package
                .and_then(package_declared_serdes)
                .map(str::to_string)
        })
        .or_else(|| system.map(str::to_string))
        .unwrap_or_else(|| DEFAULT_SERDES_NAME.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider_scan::{default_search_path, scan_roots};
    use std::{fs, path::Path};

    fn repo_root() -> PathBuf {
        // CARGO_MANIFEST_DIR = packages/cli/cargo-nano-ros
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .expect("the repo root is reachable from the manifest dir")
    }

    #[test]
    fn cdr_is_in_the_table_and_lowers_to_each_language() {
        let r = resolve_serdes("cdr").expect("cdr is provided in-tree");
        assert_eq!(r.declared, "cdr");
        assert_eq!(r.cargo_feature, "serdes-cdr");
        assert_eq!(r.cmake_value, "cdr");
        assert_eq!(r.c_define_token, "CDR");
        assert_eq!(r.crate_name, "nros-serdes");
        // Non-derivable, so they come from the descriptor and nowhere else.
        assert_eq!(r.impl_strategy, "codegen");
        assert_eq!(r.format_id, Some(1));
    }

    #[test]
    fn the_default_is_cdr_and_it_resolves() {
        assert_eq!(DEFAULT_SERDES_NAME, "cdr");
        assert!(
            resolve_serdes(DEFAULT_SERDES_NAME).is_ok(),
            "a build that declares nothing must still resolve"
        );
    }

    #[test]
    fn every_known_serdes_resolves() {
        let known = known_serdes();
        assert!(
            !known.is_empty(),
            "the generated table is empty — build.rs must refuse that"
        );
        for name in known {
            assert!(resolve_serdes(name).is_ok(), "{name} should resolve");
        }
    }

    #[test]
    fn unknown_serdes_is_rejected_and_names_what_is_available() {
        let err = resolve_serdes("flatbuf").expect_err("flatbuf is not in-tree");
        assert_eq!(err.declared, "flatbuf");
        let msg = err.to_string();
        assert!(msg.contains("flatbuf"), "{msg}");
        assert!(
            msg.contains("cdr"),
            "the error must list what IS known: {msg}"
        );
    }

    #[test]
    fn every_lowering_is_derived_from_the_name() {
        for name in known_serdes() {
            let r = resolve_serdes(name).unwrap();
            assert_eq!(r.cargo_feature, format!("serdes-{name}"));
            assert_eq!(r.cmake_value, name);
            assert_eq!(r.c_define_token, name.to_uppercase());
        }
    }

    /// The two readers of `package.xml` must agree.
    ///
    /// `build.rs` cannot reach [`PackageXml`]'s quick-xml parser, so
    /// [`crate::serdes_descriptor::package_xml_provides`] is a second, smaller
    /// reader. Two readers of one file that nobody compares is how the RMW
    /// parity map came to disagree with the vtable by 25 symbols, so they are
    /// compared here, against the real tree.
    #[test]
    fn generated_names_match_the_real_package_xml_parser() {
        let root = repo_root();
        let mut checked = 0;
        for row in SERDES_ROWS {
            // Find the provider by crate name under packages/*/*.
            let mut found = None;
            for family in fs::read_dir(root.join("packages")).expect("packages/ is readable") {
                let family = family.expect("readable entry").path();
                let Ok(pkgs) = fs::read_dir(&family) else {
                    continue;
                };
                for pkg in pkgs.flatten() {
                    let dir = pkg.path();
                    if dir.join("nros-serdes.toml").is_file() && dir.join("package.xml").is_file() {
                        let parsed = PackageXml::parse(&dir.join("package.xml"))
                            .expect("an in-tree provider's package.xml parses");
                        let names: Vec<String> = parsed
                            .provides_of_kind(SERDES_KIND)
                            .map(|p| p.name.clone())
                            .collect();
                        if names.first().map(String::as_str) == Some(row.declared) {
                            found = Some(names);
                        }
                    }
                }
            }
            let names = found.unwrap_or_else(|| {
                panic!(
                    "the generated table claims serdes `{}` but no in-tree package.xml \
                     announces it — the two package.xml readers disagree",
                    row.declared
                )
            });
            assert_eq!(
                names,
                row.names
                    .iter()
                    .map(|s| (*s).to_string())
                    .collect::<Vec<_>>(),
                "generated names for `{}` differ from the quick-xml parser's",
                row.declared
            );
            checked += 1;
        }
        assert!(checked > 0, "no rows were cross-checked");
    }

    #[test]
    fn canonical_serdes_maps_every_announced_spelling_to_one_name() {
        assert_eq!(canonical_serdes("cdr"), Some("cdr"));
        assert_eq!(canonical_serdes("CDR"), None, "names are not case-folded");
        assert_eq!(canonical_serdes("flatbuf"), None);
    }

    // -----------------------------------------------------------------------
    // The shared descriptor parser — `build.rs` depends on exactly this code
    // -----------------------------------------------------------------------

    use crate::serdes_descriptor::{
        DEFAULT_SERDES_IMPL, package_xml_provides, serdes_c_define_token,
    };

    #[test]
    fn an_empty_descriptor_is_the_default_descriptor() {
        let d = parse_serdes_descriptor("", "test").expect("empty parses");
        assert_eq!(d.impl_strategy, DEFAULT_SERDES_IMPL);
        assert_eq!(d.format_id, None);
    }

    #[test]
    fn the_shipped_cdr_descriptor_parses_to_what_the_table_says() {
        let text =
            std::fs::read_to_string(repo_root().join("packages/core/nros-serdes/nros-serdes.toml"))
                .expect("the in-tree descriptor is readable");
        let d = parse_serdes_descriptor(&text, "nros-serdes.toml").expect("parses");
        assert_eq!(d.impl_strategy, "codegen");
        assert_eq!(d.format_id, Some(1));
    }

    #[test]
    fn format_id_zero_is_rejected() {
        // RFC-0088 D2 — in-tree formats hold low values FROM 1, and 0 is the
        // "no format" hole a `#[repr(u8)]` enum leaves. Accepting it would let
        // an image assign a discriminant that means "absent" elsewhere.
        let err = parse_serdes_descriptor("[serdes]\nformat_id = 0\n", "t")
            .expect_err("0 must be rejected");
        assert!(err.contains("format_id"), "{err}");
    }

    #[test]
    fn a_non_numeric_format_id_is_an_error() {
        let err =
            parse_serdes_descriptor("[serdes]\nformat_id = \"one\"\n", "t").expect_err("not a u8");
        assert!(err.contains("u8"), "{err}");
    }

    #[test]
    fn keys_outside_the_serdes_table_are_not_read() {
        let d = parse_serdes_descriptor("[other]\nimpl = \"codegen\"\n", "t").expect("parses");
        assert_eq!(
            d.impl_strategy, DEFAULT_SERDES_IMPL,
            "a key under the wrong table must not decide the strategy"
        );
    }

    #[test]
    fn cargo_package_name_reads_only_the_package_table() {
        let manifest = "[package]\nname = \"the-crate\"\n\n\
                        [dependencies.serde]\nname = \"not-this\"\n";
        assert_eq!(cargo_package_name(manifest).as_deref(), Some("the-crate"));
        assert_eq!(cargo_package_name("[dependencies]\nname = \"x\"\n"), None);
    }

    #[test]
    fn the_build_script_xml_reader_ignores_commented_out_announcements() {
        // The real parser asserts this too
        // (`package_xml::tests::commented_out_provision_is_ignored`); the two
        // must agree, which is the point of the cross-check test above.
        let xml = r#"<export>
  <!-- <nano_ros_provides kind="serdes" name="ghost"/> -->
  <nano_ros_provides kind="serdes" name="real"/>
  <nano_ros_provides kind="rmw" name="zenoh"/>
</export>"#;
        assert_eq!(
            package_xml_provides(xml, "serdes"),
            vec!["real".to_string()]
        );
        assert_eq!(package_xml_provides(xml, "rmw"), vec!["zenoh".to_string()]);
        assert!(package_xml_provides(xml, "board").is_empty());
    }

    #[test]
    fn the_c_token_survives_a_hyphenated_name() {
        // `name=` is an open vocabulary, and a hyphen cannot appear in a
        // preprocessor identifier.
        assert_eq!(serdes_c_define_token("flat-buf"), "FLAT_BUF");
    }

    // -----------------------------------------------------------------------
    // Search-path resolution — the RFC-0088 D6 acceptance
    // -----------------------------------------------------------------------

    /// Write a provider package into `dir`. `descriptor` of `None` exercises
    /// "absent means every default applies".
    fn write_provider(dir: &Path, pkg_name: &str, serdes_name: &str, descriptor: Option<&str>) {
        fs::create_dir_all(dir).expect("create provider dir");
        fs::write(
            dir.join("package.xml"),
            format!(
                r#"<?xml version="1.0"?>
<package format="3">
  <name>{pkg_name}</name>
  <version>0.0.0</version>
  <description>test serdes provider</description>
  <maintainer email="d@example.com">D</maintainer>
  <license>Apache-2.0</license>
  <export>
    <build_type>nros_cargo</build_type>
    <nano_ros_provides kind="serdes" name="{serdes_name}"/>
  </export>
</package>
"#
            ),
        )
        .expect("write package.xml");
        fs::write(
            dir.join("Cargo.toml"),
            format!("[package]\nname = \"{pkg_name}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n"),
        )
        .expect("write Cargo.toml");
        if let Some(d) = descriptor {
            fs::write(dir.join("nros-serdes.toml"), d).expect("write descriptor");
        }
    }

    #[test]
    fn a_provider_outside_the_repo_is_selected_by_name() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().join("vendor_ws");
        write_provider(
            &root.join("flatbuf_serdes"),
            "flatbuf-serdes",
            "flatbuf",
            Some("[serdes]\nimpl = \"schema\"\nformat_id = 7\n"),
        );

        // A root that is NOT the nano-ros tree — this is the whole point.
        let scan = scan_roots(std::slice::from_ref(&root)).expect("scan the vendor root");
        assert!(
            scan.errors.is_empty(),
            "scan reported errors: {:?}",
            scan.errors
        );

        let r = resolve_serdes_in(&scan, "flatbuf").expect("the out-of-repo provider resolves");
        assert_eq!(r.declared, "flatbuf");
        assert_eq!(r.crate_name, "flatbuf-serdes");
        assert_eq!(r.cargo_feature, "serdes-flatbuf");
        assert_eq!(r.cmake_value, "flatbuf");
        assert_eq!(r.c_define_token, "FLATBUF");
        assert_eq!(r.impl_strategy, "schema");
        assert_eq!(r.format_id, Some(7));
        assert_eq!(
            r.package_dir.as_deref(),
            Some(root.join("flatbuf_serdes").as_path())
        );

        // And it is invisible to the in-tree table, which is the honest cost of
        // a compile-time table (the same one `rmw_resolver` documents).
        assert!(resolve_serdes("flatbuf").is_err());
    }

    /// The acceptance case, through the SHIPPED search path rather than a
    /// hand-built root list.
    ///
    /// `default_search_path` is `[nano-ros tree, user workspace]`, and the user
    /// workspace is a directory outside this repo. So a provider package the
    /// user drops into their own workspace IS selected by name today; it is not
    /// waiting on phase-420 W6, which widens the path beyond those two roots.
    /// A provider at a third location that is neither root is not reachable
    /// yet, and that is W6's job rather than this wave's.
    #[test]
    fn a_provider_in_the_user_workspace_reaches_the_default_search_path() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let workspace = tmp.path().join("robot_ws");
        write_provider(
            &workspace.join("src/flatbuf_serdes"),
            "flatbuf-serdes",
            "flatbuf",
            Some("[serdes]\nimpl = \"schema\"\nformat_id = 9\n"),
        );

        let nano_ros = repo_root();
        let roots = default_search_path(Some(&nano_ros), &workspace);
        assert_eq!(
            roots.len(),
            2,
            "the user workspace is outside the nano-ros tree, so it is its own root: {roots:?}"
        );

        let scan = scan_roots(&roots).expect("scan the default search path");
        let r = resolve_serdes_in(&scan, "flatbuf").expect("the user's provider resolves");
        assert_eq!(r.crate_name, "flatbuf-serdes");
        assert_eq!(r.cargo_feature, "serdes-flatbuf");
        assert_eq!(r.format_id, Some(9));
    }

    /// MEASURED, and it surprised this wave: **root 0 of the default search
    /// path contributes nothing in this repository.**
    ///
    /// The nano-ros root carries its own `.nros-ignore` (issue 0621, so a
    /// vendored checkout stops polluting a consumer's package graph), and
    /// [`crate::provider_scan`] reads `IGNORE_MARKERS` on every directory
    /// INCLUDING the root it was handed. `nros-pkg-index` does not — its walk
    /// exempts depth 0, which the marker file's own header states as the
    /// contract ("It does NOT affect nano-ros's own discovery"). Issue 0809
    /// taught `provider_scan` the `.nros-ignore` spelling and did not carry the
    /// depth-0 exemption across with it, so the two walks agree on the marker
    /// and disagree on the root.
    ///
    /// This test pins the CURRENT behaviour so the day it changes is visible.
    /// It is not a claim that the behaviour is right — a fix belongs with
    /// `provider_scan`, whose rmw/board/platform families are affected exactly
    /// as serdes is, and not with the wave that happened to notice.
    #[test]
    fn the_nano_ros_root_is_pruned_by_its_own_nros_ignore() {
        let root = repo_root();
        assert!(
            root.join(".nros-ignore").is_file(),
            "this test is about that marker; if it is gone, the finding is stale"
        );

        let scan = scan_roots(&[root]).expect("scan the nano-ros root");
        assert!(
            scan.providers.is_empty(),
            "root 0 is pruned at depth 0 — if this now finds providers, the \
             depth-0 exemption landed and `resolve_serdes_in` gains the in-tree \
             providers through the default search path for free: {:?}",
            scan.providers
                .iter()
                .map(|p| &p.package)
                .collect::<Vec<_>>()
        );
        // Which is why the in-tree cross-check above scans `packages/core`
        // directly: below the marker, the same walk finds the same provider.
    }

    #[test]
    fn a_provider_with_no_descriptor_gets_every_default() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().join("ws");
        write_provider(&root.join("bare"), "bare-serdes", "bare", None);

        let scan = scan_roots(&[root]).expect("scan");
        let r = resolve_serdes_in(&scan, "bare").expect("resolves with no descriptor");
        assert_eq!(r.impl_strategy, "schema", "RFC-0088 D7 default");
        assert_eq!(
            r.format_id, None,
            "an unstated discriminant is assigned by the build, not defaulted here"
        );
    }

    #[test]
    fn the_in_tree_provider_resolves_through_the_scan_too() {
        // The nano-ros tree is root 0 of the default search path, not a
        // builtin reached by a different code path (provider_scan's premise).
        let scan = scan_roots(&[repo_root().join("packages/core")]).expect("scan packages/core");
        let r = resolve_serdes_in(&scan, "cdr").expect("cdr resolves through the scan");
        let table = resolve_serdes("cdr").expect("and through the table");
        assert_eq!(r.declared, table.declared);
        assert_eq!(r.crate_name, table.crate_name);
        assert_eq!(r.cargo_feature, table.cargo_feature);
        assert_eq!(r.c_define_token, table.c_define_token);
        assert_eq!(r.impl_strategy, table.impl_strategy);
        assert_eq!(r.format_id, table.format_id);
    }

    #[test]
    fn a_later_root_overlays_an_earlier_one() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let under = tmp.path().join("underlay");
        let over = tmp.path().join("overlay");
        write_provider(
            &under.join("cdr_pkg"),
            "nros-serdes",
            "cdr",
            Some("[serdes]\nimpl = \"codegen\"\nformat_id = 1\n"),
        );
        write_provider(
            &over.join("cdr_pkg"),
            "my-patched-cdr",
            "cdr",
            Some("[serdes]\nimpl = \"schema\"\nformat_id = 1\n"),
        );

        let scan = scan_roots(&[under, over.clone()]).expect("scan both roots");
        let r = resolve_serdes_in(&scan, "cdr").expect("resolves");
        assert_eq!(
            r.crate_name, "my-patched-cdr",
            "the LATER root must win (provider_scan::resolve_unique)"
        );
        assert_eq!(
            r.package_dir.as_deref(),
            Some(over.join("cdr_pkg").as_path())
        );
    }

    #[test]
    fn an_unknown_name_in_a_scan_lists_what_is_available() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().join("ws");
        write_provider(&root.join("flatbuf"), "flatbuf-serdes", "flatbuf", None);

        let scan = scan_roots(&[root]).expect("scan");
        let err = resolve_serdes_in(&scan, "flatbüf").expect_err("typo must not resolve");
        let msg = err.to_string();
        assert!(msg.contains("flatbüf"), "{msg}");
        assert!(
            msg.contains("flatbuf"),
            "must name the available format: {msg}"
        );
    }

    #[test]
    fn a_provider_without_a_cargo_toml_cannot_be_lowered() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().join("ws");
        let dir = root.join("no_crate");
        write_provider(&dir, "no-crate", "nocrate", None);
        fs::remove_file(dir.join("Cargo.toml")).expect("remove Cargo.toml");

        let scan = scan_roots(&[root]).expect("scan");
        let err = resolve_serdes_in(&scan, "nocrate").expect_err("no crate to derive");
        assert!(
            err.to_string().contains("Cargo.toml"),
            "the error must name what is missing: {err}"
        );
    }

    #[test]
    fn a_malformed_descriptor_is_an_error_not_a_default() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().join("ws");
        write_provider(
            &root.join("typo"),
            "typo-serdes",
            "typo",
            Some("[serdes]\nimpl = \"codgen\"\n"),
        );

        let scan = scan_roots(&[root]).expect("scan");
        let err = resolve_serdes_in(&scan, "typo").expect_err("a typo'd impl must not default");
        assert!(err.to_string().contains("codgen"), "{err}");
    }

    // -----------------------------------------------------------------------
    // Consumption
    // -----------------------------------------------------------------------

    fn uses_xml(body: &str) -> String {
        format!(
            r#"<?xml version="1.0"?>
<package format="3">
  <name>consumer</name>
  <version>0.0.0</version>
  <description>d</description>
  <maintainer email="d@example.com">D</maintainer>
  <license>Apache-2.0</license>
  <export>
{body}
  </export>
</package>
"#
        )
    }

    #[test]
    fn a_package_selects_a_serdes_with_nano_ros_uses() {
        let pkg = PackageXml::parse_str(&uses_xml(
            r#"    <nano_ros_uses kind="serdes" name="flatbuf"/>"#,
        ))
        .expect("parses");
        assert_eq!(package_declared_serdes(&pkg), Some("flatbuf"));
    }

    #[test]
    fn a_package_selecting_nothing_declares_nothing() {
        let pkg =
            PackageXml::parse_str(&uses_xml(r#"    <nano_ros_uses kind="rmw" name="zenoh"/>"#))
                .expect("parses");
        assert_eq!(
            package_declared_serdes(&pkg),
            None,
            "an rmw selection is not a serdes selection"
        );
    }

    #[test]
    fn the_ladder_prefers_the_more_specific_declaration() {
        let pkg = PackageXml::parse_str(&uses_xml(
            r#"    <nano_ros_uses kind="serdes" name="flatbuf"/>"#,
        ))
        .expect("parses");

        assert_eq!(declared_serdes(None, None, None), "cdr");
        assert_eq!(declared_serdes(None, None, Some("uorb")), "uorb");
        assert_eq!(declared_serdes(None, Some(&pkg), Some("uorb")), "flatbuf");
        assert_eq!(
            declared_serdes(Some("cdr"), Some(&pkg), Some("uorb")),
            "cdr"
        );
    }
}
