//! Issue 1508 - the priority plan of ONE Zephyr image, read from its `.config`.
//!
//! A derived tier is allocated out of a board's `pool.app` (RFC-0079 §5), and
//! Zephyr's pool is DERIVED per image (§4.1): both ends of the transport's
//! priority chain are Kconfig, so the only honest input is the image's own
//! `.config`. Without one, the derivation falls back to
//! [`PriorityPlan::for_target`]'s Kconfig DEFAULTS projection, which is right
//! for exactly the images that run Zephyr's defaults. Measured on the two that
//! do not:
//!
//! | image | `reserved.transport` | `pool.app` | the projection's rank 0 |
//! | --- | --- | --- | --- |
//! | `CONFIG_NUM_PREEMPT_PRIORITIES=32` | `[7, 7]` | `[8, 31]` | 5 - ABOVE the transport |
//! | `CONFIG_NROS_ZENOH_READ_PRIORITY=100` | `[4, 9]` | `[10, 14]` | 5 - INSIDE the transport |
//!
//! Both roads whose derived table lands in a C/C++ Zephyr image run at cmake
//! CONFIGURE time inside the Zephyr build, after Kconfig has written the
//! `.config` - `nano_ros_add_executable`'s `nros codegen entry` and the Zephyr
//! module's `nros codegen-system` - so both are handed it (`--dotconfig`) and
//! allocate out of the image's own plan. The third road, the `nros::main!`
//! proc-macro, reads the same file through `$DOTCONFIG` (issue 1537).
//!
//! # Where the pieces live
//!
//! This module is now the CLI's thin wrapper (eyre + the operator line). The
//! pieces the macro needs too have ONE spelling each, in crates it can afford:
//!
//! * the transport-band SYMBOLS and their reader -
//!   `nros_entry_lower::zephyr_image` (RFC-0071 D2 keeps them out of the core
//!   crate; issue 0083 keeps the CLI out of the macro);
//! * the classification (resolved / unapplied / not a `.config`) -
//!   `nros_orchestration_ir::priority_plan::zephyr_image_plan`.
//!
//! The board descriptor (`packages/boards/zephyr/nros-board.toml`,
//! `[board.priority_plan] inputs`) is the statement of what the plan depends
//! on, and `the_band_symbols_are_the_descriptors_inputs` below holds the
//! symbol list to it, so a symbol added there cannot be silently missing.

use std::path::Path;

use eyre::{Result, WrapErr, eyre};
pub use nros_entry_lower::zephyr_image::{
    ZEPHYR_KERNEL_PLAN_SYMBOLS, ZEPHYR_TRANSPORT_BAND_SYMBOLS,
};
pub use nros_orchestration_ir::priority_plan::ImagePlan;
use nros_orchestration_ir::priority_plan::{PriorityPlan, ZEPHYR_TRANSPORT_BAND_DEFAULT};

/// The transport bands an image's `.config` states, in
/// [`ZEPHYR_TRANSPORT_BAND_SYMBOLS`] order; an absent symbol is the Kconfig
/// default, which is what the image runs with.
pub fn transport_bands(dotconfig: &str) -> [i64; 2] {
    nros_entry_lower::zephyr_image::transport_bands(dotconfig, ZEPHYR_TRANSPORT_BAND_DEFAULT)
}

/// Resolve the plan of the image whose `.config` text this is.
pub fn zephyr_image_plan_from_text(text: &str, origin: &str) -> Result<ImagePlan> {
    nros_orchestration_ir::priority_plan::zephyr_image_plan(text, &transport_bands(text), origin)
        .map_err(|e| eyre!("--dotconfig {e}"))
}

/// [`zephyr_image_plan_from_text`] over a file.
pub fn zephyr_image_plan(dotconfig: &Path) -> Result<ImagePlan> {
    let text = std::fs::read_to_string(dotconfig)
        .wrap_err_with(|| format!("reading the image .config {}", dotconfig.display()))?;
    zephyr_image_plan_from_text(&text, &dotconfig.display().to_string())
}

/// The plan a derivation for `tier_key` allocates out of, given the image's
/// `.config` if the caller has one. `Ok(None)` means "use the tier key's
/// projection" - no `.config`, or an image that applies no priority (whose
/// note is printed with `who` as the prefix).
///
/// A `.config` handed in for a board that is not Zephyr is refused rather than
/// ignored: it can only mean a caller wired the flag to the wrong image.
pub fn plan_for_image(
    tier_key: &str,
    dotconfig: Option<&Path>,
    who: &str,
) -> Result<Option<PriorityPlan>> {
    let Some(path) = dotconfig else {
        return Ok(None);
    };
    if tier_key != "zephyr" {
        return Err(eyre!(
            "{who}: --dotconfig {} was given for a board whose tier key is {tier_key:?}; \
             only a Zephyr image derives its priority plan from a .config",
            path.display()
        ));
    }
    match zephyr_image_plan(path)? {
        ImagePlan::Resolved(plan) => {
            eprintln!(
                "{who}: derived tier priorities allocate out of this image's plan - {} ({})",
                plan.describe_allocation(),
                path.display()
            );
            Ok(Some(plan))
        }
        ImagePlan::Unapplied(note) => {
            eprintln!("{who}: note - {note}");
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nros_orchestration_ir::priority_plan::Band;

    const GATES: &str = "CONFIG_NUM_COOP_PRIORITIES=16\n\
                         CONFIG_POSIX_PRIORITY_SCHEDULING=y\n\
                         CONFIG_PREEMPT_ENABLED=y\n";

    fn resolved(text: &str) -> PriorityPlan {
        match zephyr_image_plan_from_text(text, "test").expect("resolves") {
            ImagePlan::Resolved(p) => p,
            ImagePlan::Unapplied(n) => panic!("unexpectedly unapplied: {n}"),
        }
    }

    /// The two shapes issue 1508 measured, and the default the projection
    /// agrees with.
    #[test]
    fn the_image_plan_is_read_from_its_own_kconfig() {
        let np32 = resolved(&format!("CONFIG_NUM_PREEMPT_PRIORITIES=32\n{GATES}"));
        assert_eq!(np32.reserved["transport"], Band::new(7, 7));
        assert_eq!(np32.app, Band::new(8, 31));

        let read100 = resolved(&format!(
            "CONFIG_NUM_PREEMPT_PRIORITIES=15\n{GATES}CONFIG_NROS_ZENOH_READ_PRIORITY=100\n\
             CONFIG_NROS_ZENOH_LEASE_PRIORITY=200\n"
        ));
        assert_eq!(read100.reserved["transport"], Band::new(4, 9));
        assert_eq!(read100.app, Band::new(10, 14));

        // Absent band symbols are the Kconfig default: the projection's image.
        let default = resolved(&format!("CONFIG_NUM_PREEMPT_PRIORITIES=15\n{GATES}"));
        let projection = PriorityPlan::for_target("zephyr");
        assert_eq!(default.reserved, projection.reserved);
        assert_eq!(default.app, projection.app);
        assert_eq!(default.range, projection.range);
    }

    #[test]
    fn an_image_that_applies_no_priority_keeps_the_projection_and_says_so() {
        let text = "CONFIG_NUM_PREEMPT_PRIORITIES=15\nCONFIG_PREEMPT_ENABLED=y\n";
        match zephyr_image_plan_from_text(text, "x/.config").expect("not an error") {
            ImagePlan::Unapplied(note) => {
                assert!(note.contains("CONFIG_POSIX_PRIORITY_SCHEDULING"), "{note}");
                assert!(note.contains("NOT judged"), "{note}");
            }
            ImagePlan::Resolved(p) => panic!("resolved an unapplied image: {p:?}"),
        }
    }

    #[test]
    fn a_file_that_is_not_a_zephyr_config_is_refused_by_name() {
        let err = zephyr_image_plan_from_text("FOO=1\n", "bogus").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("CONFIG_NUM_PREEMPT_PRIORITIES"), "{msg}");
        assert!(msg.contains("bogus"), "{msg}");
    }

    #[test]
    fn a_dotconfig_for_a_non_zephyr_board_is_refused() {
        let err = plan_for_image("freertos", Some(Path::new("/x/.config")), "t").unwrap_err();
        assert!(format!("{err}").contains("freertos"), "{err}");
        assert!(plan_for_image("freertos", None, "t").unwrap().is_none());
    }

    /// The symbol list above is a projection of the board descriptor's
    /// `[board.priority_plan] inputs`; this is what keeps it one.
    #[test]
    fn the_band_symbols_are_the_descriptors_inputs() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../boards/zephyr/nros-board.toml"
        );
        let raw = std::fs::read_to_string(path).expect("zephyr board descriptor");
        let v: toml::Value = toml::from_str(&raw).expect("parses");
        let boards: Vec<&toml::Value> = match &v["board"] {
            toml::Value::Array(a) => a.iter().collect(),
            t => vec![t],
        };
        let inputs: std::collections::BTreeSet<&str> = boards
            .into_iter()
            .filter_map(|b| b.get("priority_plan"))
            .flat_map(|p| p["inputs"].as_array().expect("inputs").iter())
            .map(|s| s.as_str().expect("string input"))
            .collect();
        assert!(
            !inputs.is_empty(),
            "no [board.priority_plan] inputs in {path}"
        );
        let ours: std::collections::BTreeSet<&str> = ZEPHYR_TRANSPORT_BAND_SYMBOLS
            .iter()
            .chain(ZEPHYR_KERNEL_PLAN_SYMBOLS.iter())
            .copied()
            .collect();
        assert_eq!(inputs, ours, "descriptor inputs vs this reader's symbols");
    }
}
