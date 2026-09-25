//! phase-338 W4 — resolve `[arch.*]` compiler flags for a target triple.
//!
//! The `[arch.*]` profiles in `config/<platform>/nros-platform.toml` already
//! carry the `cflags` a target needs (RFC-0049). Before this module every
//! consumer either re-implemented the match or hardcoded one arch's flags and
//! **panicked** on the rest — which is how FreeRTOS+lwIP stayed unbuildable on
//! Cortex-M4F/M7 after `[arch.cortex-m7]` had already been added to the
//! platform config.
//!
//! One spelling of the predicate lives here, beside [`ArchEntry`] itself.
//! `nros_zpico_build::arch_matches` delegates to [`arch_matches`] rather than
//! carrying a second copy (CLAUDE.md: add ONE shared helper, never a second
//! spelling).

use std::path::PathBuf;

use crate::{manifest::ArchEntry, platform_config::PlatformsTree};

/// Returns `true` when the `[arch.*]` predicates admit this target triple.
///
/// Both predicates are substring tests on the triple: `target_match` must be
/// present, `target_exclude` must not. That is what disambiguates Cortex-M3
/// (`thumbv7m`) from Cortex-M4/M7 (`thumbv7em`), which share a prefix.
pub fn arch_matches(arch: &ArchEntry, target: &str) -> bool {
    if let Some(needle) = arch.target_match.as_deref()
        && !target.contains(needle)
    {
        return false;
    }
    if let Some(needle) = arch.target_exclude.as_deref()
        && target.contains(needle)
    {
        return false;
    }
    true
}

/// Walk up from `CARGO_MANIFEST_DIR` to the checkout root (the directory
/// holding `nros-sdk-index.toml`), then return the platform descriptor SEARCH
/// PATH under it, in the loader's own order.
///
/// Returns `None` for an out-of-tree consumer, whose caller must fall back to
/// an explicit env var rather than guessing.
///
/// phase-468 W1 — this was `config_root() -> Option<PathBuf>`, ONE root, and
/// that is issue 1486's defect a third time over. `PlatformsTree` searches
/// `packages/platform` and then `config`; this returned whichever of the two it
/// found a descriptor in FIRST, which is always `packages/platform`, so
/// `config/bare-metal` — the descriptor that answers `bare-metal` AND `esp32` —
/// was invisible to every arch lookup. It happened to cost nothing because the
/// only live caller asks about `freertos`, which lives in the first root; a
/// second caller asking about a bare-metal board would have been told the
/// platform declares no `[arch.*]` profiles, over a file that declares four.
///
/// A reach narrower than the rule reads exactly like a passing check, which is
/// why the fix is to take the loader's path rather than to add the one root the
/// symptom named.
pub fn platform_search_path() -> Option<Vec<PathBuf>> {
    let start = std::env::var("CARGO_MANIFEST_DIR").ok()?;
    let mut dir = PathBuf::from(start);
    loop {
        if dir.join("nros-sdk-index.toml").is_file() {
            // phase-400 W1 — descriptors live beside their crates now, with
            // `config/` kept for the platforms that have no package. Both are
            // roots; a root that holds no descriptor is dropped rather than
            // returned, because `load_search_path` treats an all-missing path
            // as an error and a present-but-empty one as a valid empty tree
            // (issue 0979).
            let roots: Vec<PathBuf> = ["packages/platform", "config"]
                .iter()
                .map(|r| dir.join(r))
                .filter(|cand| {
                    cand.is_dir()
                        && std::fs::read_dir(cand).is_ok_and(|mut e| {
                            e.any(|x| {
                                x.is_ok_and(|x| x.path().join("nros-platform.toml").is_file())
                            })
                        })
                })
                .collect();
            return (!roots.is_empty()).then_some(roots);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// The `cflags` of the first `[arch.*]` profile of `platform` that admits
/// `target`, in the platform manifest's declared `arch = [..]` order.
///
/// `Ok(None)` means the platform declares profiles but none matches — a real
/// answer ("this platform does not claim to support this arch"), which the
/// caller should report rather than paper over with a default.
///
/// An UNKNOWN platform is `Err`, not `Ok(None)` (phase-468 W1). The two used to
/// be the same answer here, because `declared_arch_names` returns an empty list
/// for both, and they are not the same question: "this platform does not build
/// for this arch" is a fact about a descriptor, while "no descriptor answers to
/// this name" is the absence W1 makes fatal everywhere else. Conflating them
/// turned a missing descriptor into the message
/// `no [arch.*] profile of platform 'x' admits TARGET=…`, which sends the
/// reader to add an `[arch.*]` block to a file that is not there.
pub fn cflags_for_target(
    roots: &[PathBuf],
    platform: &str,
    target: &str,
) -> Result<Option<Vec<String>>, String> {
    let tree = PlatformsTree::load_search_path(roots).map_err(|e| e.to_string())?;
    if !tree.all_names().iter().any(|n| n == platform) {
        return Err(format!(
            "no nros-platform.toml answers to platform `{platform}`.\n  searched: {}\n  \
             descriptors there answer to: {}\n  A name is answered by a descriptor's \
             `names = [..]`, not by a directory of that name — see phase-468 W1 and \
             `just check platform-name-answered`.",
            roots
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", "),
            tree.all_names().join(", "),
        ));
    }
    let table = tree.arch_table().clone();
    for name in declared_arch_names(&tree, platform) {
        let Some(entry) = table.get(&name) else {
            // A name in `arch = [..]` with no `[arch.<name>]` block is a
            // manifest bug. Report it rather than silently trying the next
            // profile, which would pick the wrong flags.
            return Err(format!(
                "platform `{platform}` declares arch profile `{name}` but \
                 config/*/nros-platform.toml defines no [arch.{name}] block"
            ));
        };
        if arch_matches(entry, target) {
            return Ok(Some(entry.cflags.clone()));
        }
    }
    Ok(None)
}

/// The arch profile names `platform` declares, in `arch = [..]` order.
///
/// Order matters: the predicates are substring tests, so `cortex-m3`
/// (`thumbv7m`) must be offered before a profile that could also admit the
/// triple. First match wins, as in the zpico include-path resolver.
pub fn declared_arch_names(tree: &PlatformsTree, platform: &str) -> Vec<String> {
    // phase-349 W1 — the manifest is keyed by DIRECTORY, so an alias
    // (`freertos-lwip` after the directory became `freertos`) has to be
    // resolved first. This path does NOT go through `PlatformsTree::chain()`,
    // which is where the other lookups get alias handling for free — the
    // arch table is merged across all files and addressed separately. Caught by
    // `freertos_lwip_resolves_both_declared_arches`, which kept using the alias
    // for exactly this reason.
    let dir = tree.resolve_alias(platform);
    tree.as_platform_manifest()
        .platform
        .get(dir)
        .map(|entry| entry.arch.clone())
        .unwrap_or_default()
}

/// Diagnostic form: the profile names and what each would admit, for an error
/// message that tells the reader what to do rather than only what failed.
pub fn describe_profiles(roots: &[PathBuf], platform: &str) -> String {
    let Ok(tree) = PlatformsTree::load_search_path(roots) else {
        return "<platform config unreadable>".to_string();
    };
    let table = tree.arch_table().clone();
    if !tree.all_names().iter().any(|n| n == platform) {
        // phase-468 W1 — say WHICH absence this is. "declares no [arch.*]
        // profiles" about a platform with no descriptor at all is a true
        // sentence that aims the reader at the wrong file.
        return format!(
            "no descriptor answers to platform `{platform}` (the tree answers to: {})",
            tree.all_names().join(", ")
        );
    }
    let names = declared_arch_names(&tree, platform);
    if names.is_empty() {
        return format!("platform `{platform}` declares no [arch.*] profiles");
    }
    names
        .iter()
        .map(|n| match table.get(n) {
            Some(e) => format!(
                "{n} (match={:?}, exclude={:?})",
                e.target_match.as_deref().unwrap_or("*"),
                e.target_exclude.as_deref().unwrap_or("-")
            ),
            None => format!("{n} (UNDEFINED)"),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shipped `nros-platform-freertos` profiles must resolve for both arches the
    /// platform declares. phase-338 W4: `[arch.cortex-m7]` was added and then
    /// ignored by the consumer, so FreeRTOS+lwIP stayed unbuildable on
    /// Cortex-M4F/M7 — this asserts the declaration is actually reachable.
    ///
    /// phase-349 W1 — deliberately still addressed as `freertos-lwip`, the
    /// ALIAS, after the directory became `freertos`. That keeps the alias path
    /// covered, and it immediately earned it: `declared_arch_names` does not go
    /// through `chain()` and was alias-blind until this test failed.
    #[test]
    fn freertos_lwip_resolves_both_declared_arches() {
        let roots = platform_search_path().expect("in-tree checkout");

        let m3 = cflags_for_target(&roots, "freertos-lwip", "thumbv7m-none-eabi")
            .expect("readable")
            .expect("thumbv7m must select an arch profile");
        assert!(
            m3.iter().any(|f| f == "-mcpu=cortex-m3"),
            "thumbv7m selected {m3:?}, expected the cortex-m3 profile"
        );

        let m7 = cflags_for_target(&roots, "freertos-lwip", "thumbv7em-none-eabihf")
            .expect("readable")
            .expect("thumbv7em-none-eabihf must select an arch profile — the M7 blocker");
        assert!(
            m7.iter().any(|f| f == "-mcpu=cortex-m7"),
            "thumbv7em-none-eabihf selected {m7:?}, expected the cortex-m7 profile"
        );
        assert!(
            m7.iter().any(|f| f == "-mfloat-abi=hard"),
            "the hard-float triple must select hard-float flags, got {m7:?}"
        );
    }

    /// An arch nobody declared returns `None` — a real answer the caller turns
    /// into a message naming what IS declared, never a silent wrong default.
    #[test]
    fn undeclared_arch_is_none_not_a_default() {
        let roots = platform_search_path().expect("in-tree checkout");
        let got =
            cflags_for_target(&roots, "freertos-lwip", "thumbv6m-none-eabi").expect("readable");
        assert!(
            got.is_none(),
            "thumbv6m (Cortex-M0) is not declared by freertos-lwip; got {got:?}"
        );
    }

    /// phase-418 418.3 — the Orin SPE profile resolves for the SOFT-float
    /// ARMv7-R triple, and carries the vendor FSP's `softfp` flags.
    ///
    /// The vendor BSP (`spe-freertos-bsp/rt-aux-cpu-demo-fsp/Makefile`, L4T
    /// 36.4.4) compiles AND links `-mcpu=cortex-r5 -mthumb-interwork
    /// -mfloat-abi=softfp -mfpu=vfpv3-d16`. Our objects join that image, so
    /// the ABI is not ours to choose.
    #[test]
    fn freertos_resolves_cortex_r5_with_the_vendor_softfp_abi() {
        let roots = platform_search_path().expect("in-tree checkout");
        let r5 = cflags_for_target(&roots, "freertos", "armv7r-none-eabi")
            .expect("readable")
            .expect("armv7r-none-eabi must select an arch profile — 418.3");
        assert!(
            r5.iter().any(|f| f == "-mcpu=cortex-r5"),
            "armv7r-none-eabi selected {r5:?}, expected the cortex-r5 profile"
        );
        assert!(
            r5.iter().any(|f| f == "-mfloat-abi=softfp"),
            "the SPE FSP is softfp; got {r5:?}"
        );
        assert!(
            !r5.iter().any(|f| f == "-mfloat-abi=hard"),
            "hard float against a soft-float-ABI FSP passes floats in the \
             wrong registers; got {r5:?}"
        );
    }

    /// `armv7r-none-eabi` is a SUBSTRING of `armv7r-none-eabihf`, so without
    /// `target_exclude = "eabihf"` the soft-float profile would silently claim
    /// the hard-float triple. Selecting nothing is the correct answer: the
    /// FreeRTOS platform does not describe a hard-float ARMv7-R C half.
    #[test]
    fn cortex_r5_does_not_claim_the_hard_float_triple() {
        let roots = platform_search_path().expect("in-tree checkout");
        let got = cflags_for_target(&roots, "freertos", "armv7r-none-eabihf").expect("readable");
        assert!(
            got.is_none(),
            "armv7r-none-eabihf must select no profile, not the softfp one; got {got:?}"
        );
    }

    /// ARMv7-R and ARMv8-R must not claim each other. `armv8r` is the
    /// discriminator and it appears in neither armv7r triple.
    #[test]
    fn cortex_r5_and_r52_do_not_claim_each_others_triples() {
        let roots = platform_search_path().expect("in-tree checkout");
        let r52 = cflags_for_target(&roots, "freertos", "armv8r-none-eabihf")
            .expect("readable")
            .expect("armv8r-none-eabihf must still select cortex-r52");
        assert!(
            r52.iter().any(|f| f == "-mcpu=cortex-r52"),
            "armv8r-none-eabihf selected {r52:?}, expected the cortex-r52 profile"
        );
    }

    /// The search path reaches BOTH descriptor roots, not just the first.
    ///
    /// phase-468 W1 / issue 1486's class. `bare-metal` lives in `config/`, the
    /// SECOND root, and it declares four `[arch.*]` profiles. Under the old
    /// one-root `config_root()` this lookup answered `Ok(None)` — "declares no
    /// profile that admits this triple" — about a file declaring exactly such a
    /// profile, and nothing in the tree asked, because the only live caller
    /// happens to live in the first root.
    ///
    /// `riscv32imc` is the profile `config/bare-metal` carries FOR the ESP32-C3,
    /// which is the same fact that made `esp32` an answerable name.
    #[test]
    fn the_second_descriptor_root_is_searched_too() {
        let roots = platform_search_path().expect("in-tree checkout");
        assert!(
            roots.len() >= 2,
            "the in-tree path is packages/platform then config; got {roots:?}"
        );
        let rv = cflags_for_target(&roots, "bare-metal", "riscv32imc-unknown-none-elf")
            .expect("readable")
            .expect("config/bare-metal declares [arch.riscv32imc] — the ESP32-C3 profile");
        assert!(
            rv.iter().any(|f| f == "-march=rv32imc"),
            "riscv32imc selected {rv:?}, expected the riscv32imc profile"
        );

        // ...and through the ALIAS the same file answers to, which is the name
        // an esp32 board declares.
        let via_alias = cflags_for_target(&roots, "esp32", "riscv32imc-unknown-none-elf")
            .expect("readable")
            .expect("`esp32` is one of config/bare-metal's `names` (phase-468 W1)");
        assert_eq!(
            via_alias, rv,
            "an alias must resolve the same profiles as the canonical name"
        );
    }

    /// An unknown platform is an ERROR, not "declares no matching profile".
    ///
    /// phase-468 W1. The two were one answer until this test existed, and the
    /// message that came out of the conflation told the reader to add an
    /// `[arch.*]` block to a file that does not exist.
    #[test]
    fn an_unanswered_platform_name_is_an_error_not_an_empty_profile_set() {
        let roots = platform_search_path().expect("in-tree checkout");
        let err = cflags_for_target(&roots, "wumpus", "thumbv7m-none-eabi")
            .expect_err("an unanswered name must not read as `no matching profile`");
        assert!(err.contains("wumpus"), "{err}");
        assert!(
            err.contains("no nros-platform.toml answers"),
            "the message must say the descriptor is ABSENT: {err}"
        );
        let described = describe_profiles(&roots, "wumpus");
        assert!(
            described.contains("no descriptor answers"),
            "describe_profiles must not call an absent descriptor an empty one: {described}"
        );
    }

    /// `target_exclude` is what keeps M3 from claiming the M4/M7 triple.
    #[test]
    fn exclude_predicate_separates_m3_from_m7() {
        let m3 = ArchEntry {
            target_match: Some("thumbv7m".into()),
            target_exclude: Some("thumbv7em".into()),
            ..Default::default()
        };
        assert!(arch_matches(&m3, "thumbv7m-none-eabi"));
        assert!(
            !arch_matches(&m3, "thumbv7em-none-eabihf"),
            "cortex-m3 must not claim the M4F/M7 triple — that is the wrong-FPU-ABI bug"
        );
    }
}
