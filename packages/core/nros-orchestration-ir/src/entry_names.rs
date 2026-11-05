//! The names an image's entry package and an owned entry's adapter crate go by
//! (RFC-0071 D13, phase-531 W5).
//!
//! Both rules live here, not in the CLI's builder, because `nros::main!`
//! cannot depend on the CLI and must compute the SAME names the builder and
//! `nros configure` write: the entry's name maps a generated entry back to its
//! image (`leaf_system::for_entry`), and the adapter's name is the crate the
//! macro imports the registration macros from. Two copies of either rule
//! would be an entry that cannot find its image, or an import of a crate that
//! does not exist.

/// The package name `nros build` gives the entry it generates for
/// `[image.<id>]` — `linux_robot1` → `linux_robot1_entry`.
#[must_use]
pub fn entry_package_name(image_id: &str) -> String {
    format!("{}_entry", image_id.replace(['-', '.', '/'], "_"))
}

/// The adapter crate an OWNED entry depends on — `demo_entry` →
/// `demo_entry_nros_derived` (RFC-0071 D13).
///
/// An entry a person owns (one from `nros materialize`, one from `nros new
/// entry`) cannot be regenerated per image, so the features its image's
/// declarations imply (edition, capabilities, the RMW on the board crate) and
/// the backend registration macros reach it through this generated crate,
/// which `nros configure` rewrites. A generated entry states both itself and
/// names no adapter. The name is the crate IDENT form (hyphens folded), so the
/// manifest key, the package name and the path `nros::main!` imports from are
/// one spelling.
#[must_use]
pub fn adapter_crate_name(entry: &str) -> String {
    format!("{}_nros_derived", entry.replace('-', "_"))
}

/// Where the adapters live, under the workspace's generated root
/// (`<ws>/generated/nros-derived/<entry>/`).
pub const ADAPTER_DIR: &str = "nros-derived";

/// The adapter's previous spelling, `<entry>_nros_selection` — kept ONLY so an
/// owned entry whose manifest still names it keeps building while it migrates
/// (`nros configure` writes the adapter under whichever spelling the manifest
/// names, and warns about the old one).
#[must_use]
pub fn legacy_adapter_crate_name(entry: &str) -> String {
    format!("{}_nros_selection", entry.replace('-', "_"))
}

/// The directory [`legacy_adapter_crate_name`] lived in.
pub const LEGACY_ADAPTER_DIR: &str = "nros-selection";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_names_fold_to_one_crate_ident() {
        assert_eq!(entry_package_name("linux-robot.1"), "linux_robot_1_entry");
        assert_eq!(adapter_crate_name("demo-entry"), "demo_entry_nros_derived");
        assert_eq!(
            legacy_adapter_crate_name("demo-entry"),
            "demo_entry_nros_selection"
        );
    }
}
