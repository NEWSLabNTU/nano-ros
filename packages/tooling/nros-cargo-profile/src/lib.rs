//! The nano-ros cargo build-profile table (phase-336).
//!
//! ONE implementation of four derivations that used to be spelled separately in
//! bash (`scripts/build/cargo.sh`), Rust (`nros-tests`), just
//! (`just/qemu-baremetal.just`), and cmake (a hardcoded `/release/` path):
//!
//! * `CMAKE_BUILD_TYPE` → cargo profile name  ([`resolve`])
//! * profile name → cargo flags               ([`build_args`], [`nextest_args`])
//! * profile name → `target/` subdirectory    ([`target_dir`])
//! * profile name → its DEFINITION, as environment variables ([`env`])
//!
//! # Who defines a profile
//!
//! The name decides. A name starting with `nros-` belongs to nano-ros: we pass
//! `--profile <name>` AND its definition through `CARGO_PROFILE_*` environment
//! variables, so a user workspace needs no `[profile.*]` block to build with it.
//! Any other name (`dev`, `release`, or the user's own) is the user's: we pass
//! the name only and inject nothing, so their manifest governs and cargo's own
//! `error: profile '<name>' is not defined` points at their file.
//!
//! The rule has to be name-scoped rather than blanket because a
//! `CARGO_PROFILE_*` environment variable OVERRIDES a manifest entry of the same
//! name — injecting unconditionally would silently discard a user's own
//! settings.

/// A cargo profile setting, as it appears both in `[profile.<name>]` TOML and
/// in a `CARGO_PROFILE_<NAME>_<KEY>` environment variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Setting {
    /// TOML key (`opt-level`); the env key is this uppercased with `-` → `_`.
    pub key: &'static str,
    /// Value as cargo parses it. Strings are unquoted here; [`toml_value`]
    /// re-quotes the ones that need it.
    pub value: &'static str,
}

/// A profile nano-ros defines and can inject.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preset {
    pub name: &'static str,
    pub settings: &'static [Setting],
}

const fn s(key: &'static str, value: &'static str) -> Setting {
    Setting { key, value }
}

/// Development default: fast to build, still optimized enough to run.
pub const RELWITHDEBINFO: Preset = Preset {
    name: "nros-relwithdebinfo",
    settings: &[
        s("inherits", "release"),
        s("opt-level", "2"),
        s("debug", "1"),
        s("lto", "off"),
        s("codegen-units", "16"),
        s("incremental", "true"),
        s("panic", "abort"),
    ],
};

/// Size-optimized: what `[profile.release]` meant before phase-336.
pub const MINSIZEREL: Preset = Preset {
    name: "nros-minsizerel",
    settings: &[
        s("inherits", "release"),
        s("opt-level", "s"),
        s("lto", "fat"),
        s("codegen-units", "1"),
        s("panic", "abort"),
    ],
};

/// Every preset nano-ros owns. A name outside this list is the user's.
pub const PRESETS: &[Preset] = &[RELWITHDEBINFO, MINSIZEREL];

/// The profile used when nothing selects one.
pub const DEFAULT_PROFILE: &str = RELWITHDEBINFO.name;

/// The `CMAKE_BUILD_TYPE` → profile map. An empty/absent build type resolves to
/// [`DEFAULT_PROFILE`]; anything not listed is an error rather than a guess.
///
/// Corrosion's own default is `Debug → dev`, everything else → `release`, which
/// maps a `-O0`-intent CMake build onto a fat-LTO cargo build. This table reads
/// the user's intent instead.
pub const BUILD_TYPE_MAP: &[(&str, &str)] = &[
    ("debug", "dev"),
    ("relwithdebinfo", RELWITHDEBINFO.name),
    ("minsizerel", MINSIZEREL.name),
    ("release", "release"),
];

/// Error from [`resolve`]: a `CMAKE_BUILD_TYPE` with no mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownBuildType {
    pub build_type: String,
}

impl std::fmt::Display for UnknownBuildType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "no cargo profile is mapped to CMAKE_BUILD_TYPE `{}` (known: {}). \
             Select one explicitly with NROS_CARGO_PROFILE=<profile>.",
            self.build_type,
            BUILD_TYPE_MAP
                .iter()
                .map(|(t, _)| *t)
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

impl std::error::Error for UnknownBuildType {}

/// Map a `CMAKE_BUILD_TYPE` to a cargo profile name.
///
/// `None` or an empty/whitespace value means "the user did not choose", which
/// resolves to [`DEFAULT_PROFILE`] — CMake emits no `-O` flags for an unset
/// build type, but an unoptimized default would make every nano-ros build
/// unusably slow at run time, so development speed wins here.
pub fn resolve(build_type: Option<&str>) -> Result<&'static str, UnknownBuildType> {
    let raw = build_type.unwrap_or("").trim();
    if raw.is_empty() {
        return Ok(DEFAULT_PROFILE);
    }
    let key = raw.to_ascii_lowercase();
    BUILD_TYPE_MAP
        .iter()
        .find(|(t, _)| *t == key)
        .map(|(_, p)| *p)
        .ok_or(UnknownBuildType {
            build_type: raw.to_string(),
        })
}

/// Is this a profile nano-ros defines (and may therefore inject)?
pub fn is_nros_preset(profile: &str) -> bool {
    preset(profile).is_some()
}

/// The preset behind a name, if nano-ros owns it.
pub fn preset(profile: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.name == profile)
}

/// `cargo build` flags selecting this profile. `dev` is cargo's default and
/// takes no flag; `release` has a dedicated one; everything else is
/// `--profile <name>`.
pub fn build_args(profile: &str) -> Vec<String> {
    match profile {
        "dev" => Vec::new(),
        "release" => vec!["--release".to_string()],
        other => vec!["--profile".to_string(), other.to_string()],
    }
}

/// `cargo nextest` flags selecting this profile. nextest spells it
/// `--cargo-profile` and has no `--release` shorthand.
pub fn nextest_args(profile: &str) -> Vec<String> {
    match profile {
        "dev" => Vec::new(),
        other => vec!["--cargo-profile".to_string(), other.to_string()],
    }
}

/// The `target/` subdirectory cargo writes this profile's artifacts to. Only
/// `dev` renames (to `debug`); every other profile uses its own name.
pub fn target_dir(profile: &str) -> String {
    match profile {
        "dev" => "debug".to_string(),
        other => other.to_string(),
    }
}

/// The profile's definition as `CARGO_PROFILE_<NAME>_<KEY>` pairs — empty for
/// any profile nano-ros does not own, which is what keeps a user's own
/// definition authoritative (env beats manifest).
pub fn env(profile: &str) -> Vec<(String, String)> {
    let Some(preset) = preset(profile) else {
        return Vec::new();
    };
    let name = profile.to_ascii_uppercase().replace('-', "_");
    preset
        .settings
        .iter()
        .map(|setting| {
            let key = setting.key.to_ascii_uppercase().replace('-', "_");
            (
                format!("CARGO_PROFILE_{name}_{key}"),
                setting.value.to_string(),
            )
        })
        .collect()
}

/// The preset's `[profile.<name>]` TOML body, used to check the root
/// `Cargo.toml` against this table rather than maintaining a second copy by
/// hand.
pub fn toml_value(setting: &Setting) -> String {
    let numeric = setting.value.parse::<i64>().is_ok();
    let boolean = matches!(setting.value, "true" | "false");
    if numeric || boolean {
        setting.value.to_string()
    } else {
        format!("\"{}\"", setting.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_build_type_maps_to_a_profile() {
        assert_eq!(resolve(Some("Debug")).unwrap(), "dev");
        assert_eq!(
            resolve(Some("RelWithDebInfo")).unwrap(),
            "nros-relwithdebinfo"
        );
        assert_eq!(resolve(Some("MinSizeRel")).unwrap(), "nros-minsizerel");
        assert_eq!(resolve(Some("Release")).unwrap(), "release");
    }

    #[test]
    fn build_type_match_is_case_insensitive() {
        // The ament verbs pass lowercase; a case-sensitive compare silently
        // took the wrong branch once already (CLAUDE.md, cmake pitfalls).
        assert_eq!(resolve(Some("minsizerel")).unwrap(), "nros-minsizerel");
        assert_eq!(resolve(Some("MINSIZEREL")).unwrap(), "nros-minsizerel");
    }

    #[test]
    fn unset_build_type_is_the_development_default() {
        assert_eq!(resolve(None).unwrap(), DEFAULT_PROFILE);
        assert_eq!(resolve(Some("")).unwrap(), DEFAULT_PROFILE);
        assert_eq!(resolve(Some("   ")).unwrap(), DEFAULT_PROFILE);
    }

    #[test]
    fn unknown_build_type_names_the_escape_hatch() {
        let err = resolve(Some("Coverage")).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Coverage"), "{msg}");
        assert!(msg.contains("NROS_CARGO_PROFILE"), "{msg}");
    }

    #[test]
    fn build_args_use_cargos_own_spelling() {
        assert!(build_args("dev").is_empty());
        assert_eq!(build_args("release"), vec!["--release"]);
        assert_eq!(
            build_args("nros-minsizerel"),
            vec!["--profile", "nros-minsizerel"]
        );
        // A user profile is passed through verbatim — we do not rewrite names.
        assert_eq!(build_args("prod"), vec!["--profile", "prod"]);
    }

    #[test]
    fn nextest_has_no_release_shorthand() {
        assert!(nextest_args("dev").is_empty());
        assert_eq!(nextest_args("release"), vec!["--cargo-profile", "release"]);
    }

    #[test]
    fn only_dev_renames_its_target_dir() {
        assert_eq!(target_dir("dev"), "debug");
        assert_eq!(target_dir("release"), "release");
        assert_eq!(target_dir("nros-relwithdebinfo"), "nros-relwithdebinfo");
        assert_eq!(target_dir("prod"), "prod");
    }

    #[test]
    fn env_defines_our_presets_without_a_manifest_entry() {
        let vars = env("nros-minsizerel");
        assert!(vars.contains(&(
            "CARGO_PROFILE_NROS_MINSIZEREL_INHERITS".to_string(),
            "release".to_string()
        )));
        assert!(vars.contains(&(
            "CARGO_PROFILE_NROS_MINSIZEREL_OPT_LEVEL".to_string(),
            "s".to_string()
        )));
    }

    #[test]
    fn env_is_empty_for_profiles_we_do_not_own() {
        // The ownership rule: injecting here would override the user's own
        // `[profile.prod]`, because env beats manifest.
        assert!(env("prod").is_empty());
        assert!(env("release").is_empty());
        assert!(env("dev").is_empty());
    }

    #[test]
    fn presets_all_inherit_a_builtin() {
        // A custom profile without `inherits` is rejected by cargo, and the
        // env-injected form has no manifest to fall back on.
        for preset in PRESETS {
            assert!(
                preset.settings.iter().any(|s| s.key == "inherits"),
                "preset `{}` has no `inherits`",
                preset.name
            );
            assert!(
                preset.name.starts_with("nros-"),
                "preset `{}` is outside the namespace we own",
                preset.name
            );
        }
    }

    #[test]
    fn toml_values_quote_only_what_needs_it() {
        assert_eq!(toml_value(&s("opt-level", "2")), "2");
        assert_eq!(toml_value(&s("opt-level", "s")), "\"s\"");
        assert_eq!(toml_value(&s("incremental", "true")), "true");
        assert_eq!(toml_value(&s("lto", "off")), "\"off\"");
    }

    /// Read `[profile.<name>]` out of a manifest as `key = value` pairs, with
    /// comments and blank lines dropped. Deliberately a scanner rather than a
    /// TOML parse: this crate has no dependencies, and a new one would move the
    /// root `Cargo.lock` for a test (CLAUDE.md — locks change only when a dev
    /// means it).
    fn profile_block(manifest: &str, name: &str) -> Option<Vec<(String, String)>> {
        let header = format!("[profile.{name}]");
        let mut lines = manifest.lines().skip_while(|l| l.trim() != header);
        lines.next()?;
        let mut out = Vec::new();
        for line in lines {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') {
                break;
            }
            let (k, v) = line.split_once('=')?;
            out.push((k.trim().to_string(), v.trim().to_string()));
        }
        Some(out)
    }

    #[test]
    fn root_manifest_matches_this_table() {
        // The presets exist in TWO places by necessity: here (so cmake, bash
        // and the tests can read them without parsing TOML) and in the root
        // `Cargo.toml` (so a bare `cargo build --profile nros-minsizerel` in
        // this repo works without the environment injection). This test is what
        // makes the second copy safe — the mirror-drift class CLAUDE.md names.
        let manifest = include_str!("../../../../Cargo.toml");
        for preset in PRESETS {
            let block = profile_block(manifest, preset.name)
                .unwrap_or_else(|| panic!("root Cargo.toml has no [profile.{}]", preset.name));
            for setting in preset.settings {
                let want = toml_value(setting);
                let got = block
                    .iter()
                    .find(|(k, _)| k == setting.key)
                    .unwrap_or_else(|| {
                        panic!("[profile.{}] is missing `{}`", preset.name, setting.key)
                    });
                assert_eq!(
                    got.1, want,
                    "[profile.{}] {} = {} in Cargo.toml, {want} in PRESETS",
                    preset.name, setting.key, got.1
                );
            }
            assert_eq!(
                block.len(),
                preset.settings.len(),
                "[profile.{}] in Cargo.toml has settings this table does not: {:?}",
                preset.name,
                block
                    .iter()
                    .map(|(k, _)| k.as_str())
                    .filter(|k| !preset.settings.iter().any(|s| s.key == *k))
                    .collect::<Vec<_>>()
            );
        }
    }
}
