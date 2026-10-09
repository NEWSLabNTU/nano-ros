//! Issue 1766 — what a single-package leaf's `[system] features` asks the
//! entry to WIRE.
//!
//! A leaf states its capability axes in `system.toml`'s `[system] features`,
//! the key a workspace bringup uses ([`crate::leaf_system::LeafSystem::features`],
//! issue 1142). The sizing descriptor already reads that key on the leaf road
//! (the queryable pool counts the parameter and lifecycle servers for it, and
//! issue 1706 carves the parameter store from it). A Form-1 `nros::main!()` —
//! the shape every `examples/*/rust/<role>` leaf uses — did NOT read it: the
//! axis was dropped in silence, so the descriptor sized for services the image
//! never registered. This module is the reading the macro shares with every
//! other leaf consumer, so "which axes exist" and "what an axis lowers to" have
//! one answer on the leaf road.
//!
//! Every declared axis is either HONOURED or REFUSED, never dropped:
//!
//! * an axis the table below does not name is an error naming the known ones
//!   (the CLI's `validate_and_warn_capabilities` refuses the same typo on the
//!   workspace road);
//! * each known axis names the `nros` cargo feature that carries it, so the
//!   entry can const-assert that the feature is compiled in — a leaf, unlike a
//!   generated entry, writes its own `nros` features, and without the feature
//!   the runtime hook is a no-op;
//! * the typed `[param_services]` / `[safety]` blocks a bringup may still carry
//!   (deprecated by phase-261) are refused in a leaf: no leaf consumer reads
//!   them, so honouring one in the entry alone would make the entry and the
//!   descriptor disagree, and ignoring it is the silent drop this module exists
//!   to end.
//!
//! The CLI keeps the full lowering registry (`cargo_nano_ros::capability_resolver`);
//! `nros-cli-core` pins this table to it in both directions
//! (`leaf_capability_axes_match_the_capability_registry`).

use std::path::Path;

/// One capability axis as a leaf entry lowers it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeafAxis {
    /// The declared name, as written in `[system] features`.
    pub declared: &'static str,
    /// The `nros` umbrella feature that carries the axis.
    pub nros_feature: &'static str,
    /// The `nros::__macro_support` const reporting whether THIS `nros` build
    /// carries [`Self::nros_feature`]. A cfg in the entry crate cannot see a
    /// feature of `nros`, so only `nros` itself can answer.
    pub compiled_flag: &'static str,
}

/// The axes a leaf can declare. Mirrors `cargo_nano_ros::capability_resolver::CAPABILITIES`
/// (gated in `nros-cli-core`).
pub const LEAF_CAPABILITY_AXES: &[LeafAxis] = &[
    LeafAxis {
        declared: "safety",
        nros_feature: "safety-e2e",
        compiled_flag: "SAFETY_E2E_ENABLED",
    },
    LeafAxis {
        declared: "param_services",
        nros_feature: "param-services",
        compiled_flag: "PARAM_SERVICES_ENABLED",
    },
    LeafAxis {
        declared: "lifecycle",
        nros_feature: "lifecycle-services",
        compiled_flag: "LIFECYCLE_SERVICES_ENABLED",
    },
    LeafAxis {
        declared: "rosout",
        nros_feature: "rosout",
        compiled_flag: "ROSOUT_ENABLED",
    },
];

/// Look up an axis by its declared name.
pub fn axis(declared: &str) -> Option<&'static LeafAxis> {
    LEAF_CAPABILITY_AXES.iter().find(|a| a.declared == declared)
}

/// What a leaf's declared axes ask the entry to do.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LeafCapabilities {
    /// Every declared axis, in declaration order, de-duplicated. The entry
    /// const-asserts each one's [`LeafAxis::compiled_flag`].
    pub axes: Vec<&'static LeafAxis>,
    /// `param_services` is declared: register the six parameter services.
    pub param_services: bool,
    /// `lifecycle` is declared: register the five REP-2002 services and drive
    /// boot autostart at this level (0 none / 1 configure / 2 active — the
    /// encoding `apply_lifecycle` takes). `[lifecycle] autostart` gives the
    /// level; absent, it is 0, as `autostart = "none"` is on the C/C++ road.
    pub lifecycle_autostart: Option<u8>,
}

/// The typed capability blocks a bringup may carry and a leaf may not.
const TYPED_BLOCKS: &[&str] = &["param_services", "safety"];

/// Read the axes `features` declares, with `doc` the leaf's parsed
/// `system.toml` (for `[lifecycle] autostart` and the refused typed blocks)
/// and `file` its path, for the messages.
pub fn read(
    features: &[String],
    doc: &toml::Table,
    file: &Path,
) -> Result<LeafCapabilities, String> {
    let mut out = LeafCapabilities::default();
    for f in features {
        let a = axis(f).ok_or_else(|| {
            format!(
                "{}: `[system] features` names `{f}`, which is not a capability axis \
                 (known axes: {}). An axis nothing knows would be dropped in silence, \
                 so it is refused (issue 1766).",
                file.display(),
                LEAF_CAPABILITY_AXES
                    .iter()
                    .map(|a| a.declared)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
        if !out.axes.iter().any(|x| x.declared == a.declared) {
            out.axes.push(a);
        }
    }
    for blk in TYPED_BLOCKS {
        if doc.contains_key(*blk) {
            return Err(format!(
                "{}: a leaf declares the `{blk}` axis as `[system] features = [\"{blk}\"]`; \
                 the typed `[{blk}]` block is the deprecated bringup spelling (phase-261), \
                 and nothing on the leaf road reads it (issue 1766).",
                file.display()
            ));
        }
    }
    out.param_services = out.axes.iter().any(|a| a.declared == "param_services");
    let lifecycle = out.axes.iter().any(|a| a.declared == "lifecycle");
    let autostart = match doc.get("lifecycle") {
        None => None,
        Some(v) => {
            let t = v.as_table().ok_or_else(|| {
                format!(
                    "{}: `lifecycle` must be a table (`[lifecycle]`)",
                    file.display()
                )
            })?;
            if !lifecycle {
                return Err(format!(
                    "{}: `[lifecycle]` is present but `[system] features` does not declare \
                     `lifecycle`. On a leaf the axis is the `features` entry — the sizing \
                     descriptor counts the five lifecycle servers from it — and the table \
                     only carries `autostart` (issue 1766).",
                    file.display()
                ));
            }
            match t.get("autostart") {
                None => Some(0),
                Some(v) => Some(match v.as_str() {
                    Some("none") => 0,
                    Some("configure") => 1,
                    Some("active") => 2,
                    _ => {
                        return Err(format!(
                            "{}: `[lifecycle] autostart = {v}` is not one of \
                             \"none\", \"configure\", \"active\"",
                            file.display()
                        ));
                    }
                }),
            }
        }
    };
    if lifecycle {
        out.lifecycle_autostart = Some(autostart.unwrap_or(0));
    }
    Ok(out)
}

/// [`read`] over a leaf `system.toml` on disk.
pub fn read_file(features: &[String], file: &Path) -> Result<LeafCapabilities, String> {
    let raw = std::fs::read_to_string(file).map_err(|e| format!("read {}: {e}", file.display()))?;
    let doc = raw
        .parse::<toml::Table>()
        .map_err(|e| format!("{}: {e}", file.display()))?;
    read(features, &doc, file)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feats(f: &[&str]) -> Vec<String> {
        f.iter().map(|s| s.to_string()).collect()
    }

    fn doc(s: &str) -> toml::Table {
        s.parse().expect("toml")
    }

    const F: &str = "leaf/system.toml";

    #[test]
    fn no_features_wires_nothing() {
        let c = read(&[], &doc("[system]\nname = \"t\"\n"), Path::new(F)).unwrap();
        assert_eq!(c, LeafCapabilities::default());
    }

    #[test]
    fn param_services_is_wired_and_asserted() {
        let c = read(&feats(&["param_services"]), &doc(""), Path::new(F)).unwrap();
        assert!(c.param_services);
        assert_eq!(c.lifecycle_autostart, None);
        assert_eq!(c.axes, vec![axis("param_services").unwrap()]);
        assert_eq!(c.axes[0].compiled_flag, "PARAM_SERVICES_ENABLED");
    }

    #[test]
    fn lifecycle_defaults_to_no_autostart_and_reads_the_table() {
        let c = read(&feats(&["lifecycle"]), &doc(""), Path::new(F)).unwrap();
        assert_eq!(c.lifecycle_autostart, Some(0));
        let c = read(
            &feats(&["lifecycle"]),
            &doc("[lifecycle]\nautostart = \"active\"\n"),
            Path::new(F),
        )
        .unwrap();
        assert_eq!(c.lifecycle_autostart, Some(2));
    }

    #[test]
    fn an_unknown_axis_is_refused_naming_the_known_ones() {
        let e = read(&feats(&["param_service"]), &doc(""), Path::new(F)).unwrap_err();
        assert!(
            e.contains("`param_service`") && e.contains("param_services, lifecycle"),
            "{e}"
        );
    }

    #[test]
    fn a_lifecycle_table_without_the_axis_and_a_typed_block_are_refused() {
        let e = read(
            &[],
            &doc("[lifecycle]\nautostart = \"active\"\n"),
            Path::new(F),
        )
        .unwrap_err();
        assert!(e.contains("does not declare"), "{e}");
        let e = read(
            &[],
            &doc("[param_services]\nenabled = true\n"),
            Path::new(F),
        )
        .unwrap_err();
        assert!(e.contains("deprecated bringup spelling"), "{e}");
        let e = read(
            &feats(&["lifecycle"]),
            &doc("[lifecycle]\nautostart = \"sometimes\"\n"),
            Path::new(F),
        )
        .unwrap_err();
        assert!(e.contains("not one of"), "{e}");
    }
}
