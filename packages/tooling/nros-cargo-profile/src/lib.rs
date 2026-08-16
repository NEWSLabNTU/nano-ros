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
///
/// # Why `incremental` is absent (phase-340 W1)
///
/// It was `incremental = true` from phase-336 until 2026-08-06. Measured on
/// `build-test-fixtures lane=native`, four alternating runs with every target
/// dir wiped between them:
///
/// * **disk −27.6 %** (43643 -> 31600 MiB), identical to the MiB across reps.
///   Unconditional, and the reason this changed.
/// * **wall-clock −12.2 %** once the sccache cache is warm, **+2.4 %** on the
///   very first cold-cache run. Incremental keeps units away from sccache
///   (11514 requests vs 18780), so the incremental arm both populates less and
///   benefits less; the cold run is where that briefly looks like a win.
///
/// The lanes build each leaf ONCE into a per-leaf target dir, which is the case
/// where incremental state is written and never read. Local iteration — the case
/// it does serve — is [`ITERATE`], opt-in by name.
///
/// Do NOT re-enable it by setting `CARGO_INCREMENTAL=1`: sccache 0.8.2 aborts
/// the whole build during its `rustc -vV` probe with
/// `sccache: increment compilation is prohibited`. A profile setting is a
/// different input (cargo passes `-C incremental=<dir>` and never sets that
/// variable), which is why the profile below works and the env var does not.
pub const RELWITHDEBINFO: Preset = Preset {
    name: "nros-relwithdebinfo",
    settings: &[
        s("inherits", "release"),
        s("opt-level", "2"),
        s("debug", "1"),
        s("lto", "off"),
        s("codegen-units", "16"),
        s("panic", "abort"),
    ],
};

/// [`RELWITHDEBINFO`] plus incremental compilation, for LOCAL ITERATION.
///
/// Incremental pays off when the same target dir is rebuilt after an edit, which
/// is what a human does and what the fixture lanes never do. Select it by name:
///
/// ```console
/// NROS_CARGO_PROFILE=nros-iterate just build
/// ```
///
/// It costs ~28 % more disk than the default (phase-340 W1), so it is opt-in
/// rather than the ambient setting.
///
/// # Why the settings are repeated instead of `inherits = "nros-relwithdebinfo"`
///
/// Two independent reasons, both found by trying the chain first:
///
/// 1. [`env`] injects ONE profile's settings. A chained parent is not injected
///    with it, so in a workspace outside this checkout — where the
///    `.cargo/config.toml` walk-up does not reach — cargo fails with
///    `profile 'nros-relwithdebinfo' is not defined`.
/// 2. The name would be AMBIGUOUS. A profile called
///    `nros-relwithdebinfo-incremental` uppercases to the same
///    `CARGO_PROFILE_NROS_RELWITHDEBINFO_INCREMENTAL` prefix as the `incremental`
///    KEY of profile `nros-relwithdebinfo`, and cargo resolves it as the latter:
///    `could not load config key profile.nros-relwithdebinfo`. Any preset name
///    that extends another preset's name with a word that is also a cargo
///    profile key collides this way — which is why this one is `nros-iterate`.
///
/// Repetition is therefore load-bearing, and `assert_mirrors` keeps the three
/// TOML copies honest about it.
pub const ITERATE: Preset = Preset {
    name: "nros-iterate",
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
pub const PRESETS: &[Preset] = &[RELWITHDEBINFO, ITERATE, MINSIZEREL];

/// The profile NuttX Rust images must be built at.
///
/// Phase 177.8.c: at `lto = "off"` a non-deterministic `armv7a-nuttx-eabihf`
/// cross-CGU miscompile corrupts the std `lang_start` main-closure fat pointer
/// and the image reboots before `main` with no console output. Fat LTO merges
/// the codegen units and the bug disappears. Never root-caused (phase-285 W5
/// rode the same dodge for nuttx-riscv).
///
/// It is a constant rather than a literal because the builder and THREE test-side
/// resolvers have to agree on it — when they disagreed, a test looked for the
/// binary in a directory the builder never wrote to and reported the fixture
/// missing (#156).
pub const NUTTX_RUST_PROFILE: &str = MINSIZEREL.name;

/// The profile FreeRTOS QEMU (Cortex-M3) images must be built at.
///
/// Not a correctness bug like the NuttX one — a timing floor. `qemu-system-arm`
/// emulating an M3 is slow enough that a lightly-optimized zenoh-pico misses its
/// session handshake window, and the image "boots but never connects". Issue
/// #126 is the C-side face of the same constraint (the fixture rows pin
/// `CMAKE_BUILD_TYPE=Release`).
pub const FREERTOS_QEMU_PROFILE: &str = MINSIZEREL.name;

/// The profile the generated C++ FFI glue staticlib must be built at.
///
/// It is a SECOND Rust staticlib in a link that already contains
/// `libnros_cpp.a`, and at `lto = "off"` each one carries its own copy of
/// std's panicking codegen unit — so the link fails with
/// `multiple definition of __rustc::rust_begin_unwind`. Fat LTO internalizes
/// that symbol, which is why the glue crate's generated manifest pinned
/// `lto = true` back when it was always built `--release`.
///
/// nano-ros deliberately does NOT paper over this with
/// `--allow-multiple-definition` (RFC-0042 D3 removed that flag), so the glue
/// keeps a profile that leaves one definition standing.
pub const CPP_FFI_GLUE_PROFILE: &str = MINSIZEREL.name;

/// Platforms that cannot use the ambient profile, and what they use instead.
/// Reachable by name so the shell builders read the same value the Rust
/// resolvers do.
pub const CARVE_OUTS: &[(&str, &str)] = &[
    ("nuttx-rust", NUTTX_RUST_PROFILE),
    ("freertos-qemu", FREERTOS_QEMU_PROFILE),
    ("cpp-ffi-glue", CPP_FFI_GLUE_PROFILE),
];

/// The profile a named carve-out forces, if there is one.
pub fn carve_out(name: &str) -> Option<&'static str> {
    CARVE_OUTS.iter().find(|(n, _)| *n == name).map(|(_, p)| *p)
}

/// The profile a PLATFORM's cargo fixtures are built at, when it is not the
/// ambient one. The Rust twin of `nros_cargo_platform_profile` in
/// `scripts/build/cargo.sh`, keyed on the same coordinate `platform` values the
/// fixture manifest emits (`freertos`, `nuttx`, `nuttx-riscv`).
///
/// Issue 0608 — `CARVE_OUTS` is keyed by carve-out NAME (`nuttx-rust`), which no
/// caller holds; what a resolver has is the row's coordinate. Each site
/// therefore rebuilt the platform→profile mapping itself, and the group-row
/// resolver simply never did — so every group-built NuttX row was looked up
/// under `nros-relwithdebinfo` while the builder wrote `nros-minsizerel`.
/// CLAUDE.md's rule for a platform's fixture profile already says there is ONE
/// derivation; this is it, on the Rust side.
pub fn platform_profile(platform: &str) -> Option<&'static str> {
    match platform {
        "freertos" => Some(FREERTOS_QEMU_PROFILE),
        "nuttx" | "nuttx-riscv" => Some(NUTTX_RUST_PROFILE),
        _ => None,
    }
}

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

/// The profile named by an env var, treating EMPTY as unset.
///
/// The justfile exports `NROS_CARGO_PROFILE := ""` so this table owns the
/// default rather than a literal evaluated at justfile parse time. Rust's
/// `env::var` returns `Ok("")` for a set-but-empty variable, so a plain
/// `unwrap_or(DEFAULT)` silently yields `""` — and an empty profile name builds
/// paths like `target//<binary>`, which is how 110 tests once reported their
/// binaries as missing. Shell and cmake callers already treat empty as unset;
/// this is that rule for Rust, in one place.
pub fn profile_or_default(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or(DEFAULT_PROFILE)
        .to_string()
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

/// `cargo build` flags for a profile identified by its `target/` DIRECTORY
/// name rather than its profile name — the form a build script can recover from
/// `OUT_DIR` when cargo's own `PROFILE` variable cannot help (it only ever says
/// `debug` or `release`, never the custom name that is actually in effect).
pub fn build_args_for_dir(target_dir_name: &str) -> Vec<String> {
    match target_dir_name {
        "debug" => Vec::new(),
        other => build_args(other),
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
    /// Issue 0608 — `platform_profile` mirrors `nros_cargo_platform_profile` in
    /// `scripts/build/cargo.sh`, and a mirror that nobody compares is how the
    /// builder and the resolver came to disagree in the first place. So READ the
    /// shell's switch and assert the Rust table answers the same for every arm,
    /// rather than restating the constants here (which would agree with itself
    /// forever).
    #[test]
    fn platform_profile_agrees_with_the_shell_builder() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .expect("repo root");
        let sh = root.join("scripts/build/cargo.sh");
        let text = std::fs::read_to_string(&sh)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", sh.display()));

        let body = text
            .split_once("nros_cargo_platform_profile() {")
            .expect(
                "nros_cargo_platform_profile is gone from cargo.sh — this test \
                     guards a mirror; find where the rule moved and re-point it",
            )
            .1
            .split_once('}')
            .expect("unterminated function body")
            .0;

        let mut arms = 0usize;
        for line in body.lines() {
            let line = line.trim();
            let Some((pats, action)) = line.split_once(')') else {
                continue;
            };
            if !action.contains(";;") {
                continue;
            }
            for pat in pats.split('|').map(str::trim) {
                if pat.is_empty() {
                    continue;
                }
                if pat == "*" {
                    // the ambient default — Rust spells it `None`
                    assert_eq!(
                        super::platform_profile("some-platform-with-no-carve-out"),
                        None,
                        "shell falls through to the ambient profile; Rust must too"
                    );
                    continue;
                }
                arms += 1;
                let want = if action.contains("nuttx") {
                    super::NUTTX_RUST_PROFILE
                } else if action.contains("freertos") {
                    super::FREERTOS_QEMU_PROFILE
                } else {
                    panic!("cargo.sh arm {pat:?} -> {action:?} has no Rust counterpart");
                };
                assert_eq!(
                    super::platform_profile(pat),
                    Some(want),
                    "cargo.sh maps platform {pat:?} to {action:?}; platform_profile disagrees"
                );
            }
        }
        assert!(
            arms >= 3,
            "parsed only {arms} carve-out arms from cargo.sh — the switch shape \
             changed and this test stopped checking anything"
        );
    }

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
    fn empty_env_means_unset_not_an_empty_profile() {
        assert_eq!(profile_or_default(None), DEFAULT_PROFILE);
        assert_eq!(profile_or_default(Some("")), DEFAULT_PROFILE);
        assert_eq!(profile_or_default(Some("   ")), DEFAULT_PROFILE);
        assert_eq!(profile_or_default(Some("release")), "release");
        // An empty name would build `target//<bin>`; assert the dir is never
        // empty for the value this returns.
        assert!(!target_dir(&profile_or_default(Some(""))).is_empty());
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
    fn dir_names_round_trip_to_flags() {
        // `debug` is `dev`'s directory, so it takes no flag; every other
        // directory name IS the profile name.
        assert!(build_args_for_dir("debug").is_empty());
        assert_eq!(build_args_for_dir("release"), vec!["--release"]);
        assert_eq!(
            build_args_for_dir("nros-relwithdebinfo"),
            vec!["--profile", "nros-relwithdebinfo"]
        );
        for preset in PRESETS {
            assert_eq!(
                build_args_for_dir(&target_dir(preset.name)),
                build_args(preset.name),
                "dir round-trip broke for {}",
                preset.name
            );
        }
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
        //
        // The `a builtin` half of this test's name was not enforced until
        // phase-340 W1: it checked only that SOME `inherits` was present, so a
        // preset chaining another preset passed. `env` injects one profile's
        // settings, so outside this checkout — where the `.cargo/config.toml`
        // walk-up does not reach — cargo fails `profile '<parent>' is not
        // defined`. Presets must bottom out in a cargo builtin.
        const BUILTIN: &[&str] = &["dev", "release", "test", "bench"];
        for preset in PRESETS {
            let inherits = preset
                .settings
                .iter()
                .find(|s| s.key == "inherits")
                .unwrap_or_else(|| panic!("preset `{}` has no `inherits`", preset.name));
            assert!(
                BUILTIN.contains(&inherits.value),
                "preset `{}` inherits `{}`, which is not a cargo builtin ({BUILTIN:?}). \
                 `env` injects one profile only, so a chained parent is undefined \
                 outside this checkout.",
                preset.name,
                inherits.value
            );
            assert!(
                preset.name.starts_with("nros-"),
                "preset `{}` is outside the namespace we own",
                preset.name
            );
        }
    }

    #[test]
    fn preset_names_cannot_collide_with_another_preset_key() {
        // `CARGO_PROFILE_<NAME>_<KEY>` flattens both halves the same way, so a
        // preset whose name extends another preset's name with a word that is
        // also a cargo profile KEY produces an AMBIGUOUS variable.
        // `nros-relwithdebinfo-incremental` + `inherits` and `nros-relwithdebinfo`
        // + `incremental_inherits` are the same string; cargo picks the latter
        // and dies with `could not load config key profile.nros-relwithdebinfo`.
        // Found in phase-340 W1 by shipping exactly that name.
        //
        // Compared against EVERY cargo profile key, not just the ones a preset
        // declares — cargo parses the variable against its own key namespace, so
        // dropping `incremental` from a preset does not make the name safe. The
        // first version of this test checked declared keys only and passed the
        // very name that had just failed.
        const PROFILE_KEYS: &[&str] = &[
            "opt-level",
            "debug",
            "split-debuginfo",
            "strip",
            "debug-assertions",
            "overflow-checks",
            "lto",
            "panic",
            "incremental",
            "codegen-units",
            "rpath",
            "inherits",
        ];
        let envify = |s: &str| s.to_uppercase().replace('-', "_");
        for a in PRESETS {
            for b in PRESETS {
                if a.name == b.name {
                    continue;
                }
                for key in PROFILE_KEYS {
                    assert_ne!(
                        envify(a.name),
                        format!("{}_{}", envify(b.name), envify(key)),
                        "preset `{}` is ambiguous with `{}` + the `{}` key once \
                         flattened to CARGO_PROFILE_*; cargo resolves it as the latter",
                        a.name,
                        b.name,
                        key
                    );
                }
            }
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

    /// Assert a manifest's `[profile.<preset>]` blocks say exactly what this
    /// table says — same keys, same values, no extras.
    fn assert_mirrors(manifest: &str, what: &str) {
        for preset in PRESETS {
            let block = profile_block(manifest, preset.name)
                .unwrap_or_else(|| panic!("{what} has no [profile.{}]", preset.name));
            for setting in preset.settings {
                let want = toml_value(setting);
                let got = block
                    .iter()
                    .find(|(k, _)| k == setting.key)
                    .unwrap_or_else(|| {
                        panic!(
                            "{what} [profile.{}] is missing `{}`",
                            preset.name, setting.key
                        )
                    });
                assert_eq!(
                    got.1, want,
                    "{what} [profile.{}] {} = {} , PRESETS says {want}",
                    preset.name, setting.key, got.1
                );
            }
            assert_eq!(
                block.len(),
                preset.settings.len(),
                "{what} [profile.{}] has settings this table does not: {:?}",
                preset.name,
                block
                    .iter()
                    .map(|(k, _)| k.as_str())
                    .filter(|k| !preset.settings.iter().any(|s| s.key == *k))
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn config_toml_matches_this_table() {
        // `.cargo/config.toml` carries the presets so a bare `cargo build
        // --profile nros-…` in any leaf under this checkout resolves through
        // cargo's config walk-up, without the env injection the build scripts
        // add. Third copy, same gate.
        assert_mirrors(
            include_str!("../../../../.cargo/config.toml"),
            ".cargo/config.toml",
        );
    }

    #[test]
    fn root_manifest_matches_this_table() {
        // The presets exist in TWO places by necessity: here (so cmake, bash
        // and the tests can read them without parsing TOML) and in the root
        // `Cargo.toml` (so a bare `cargo build --profile nros-minsizerel` in
        // this repo works without the environment injection). This test is what
        // makes the second copy safe — the mirror-drift class CLAUDE.md names.
        assert_mirrors(include_str!("../../../../Cargo.toml"), "root Cargo.toml");
    }
}
