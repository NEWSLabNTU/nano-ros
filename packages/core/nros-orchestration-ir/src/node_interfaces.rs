//! phase-556 W13 -- which node interfaces an image gets.
//!
//! An upstream rclcpp node serves the six parameter services and publishes
//! `/rosout` without its author asking. nano-ros builds those as capability
//! axes (`param_services`, `rosout`) a bringup DECLARES in `features`. This
//! module is the ONE place that turns the declared list into the list an image
//! is built with: every reader of a bringup's or a leaf's `features` (the
//! overlay beside a model, the leaf reader, the CLI's strict `SystemToml`, and
//! everything downstream of them -- the sizing descriptor's `infra_families`,
//! the Zephyr Kconfig symbols, `NROS_FEATURES`) goes through
//! [`effective_features`], so changing the default is one constant here
//! ([`DEFAULT_NODE_INTERFACES`]). `check-node-interface-readers` refuses a
//! read of the declared key that bypasses it.
//!
//! The per-image switch is `node_interfaces = "stock" | "none"`, stated in
//! `[system]`, `[image_defaults]` or `[image.<id>]`; the most specific one
//! wins ([`resolve`]). It has no environment carrier: it travels only as the
//! effective feature list the carriers above already hold.

use serde::{Deserialize, Serialize};

/// The stock node interfaces, as capability axes: the six parameter services
/// and the `/rosout` publisher an rclcpp node has by default.
pub const STOCK: &[&str] = &["param_services", "rosout"];

/// The `system.toml` key the switch is written under.
pub const KEY: &str = "node_interfaces";

/// The per-image switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeInterfaces {
    /// The stock set is added to whatever the image declares.
    Stock,
    /// The stock set is removed, even where a broader table declared it.
    None,
}

/// What an image that states no switch gets.
///
/// `None` here means "as declared": no stock interface is added or removed.
/// phase-556 W13 step 3 flips this to `Some(NodeInterfaces::Stock)`, which
/// makes every image serve the stock set unless it opts out with
/// `node_interfaces = "none"`.
pub const DEFAULT_NODE_INTERFACES: Option<NodeInterfaces> = None;

impl NodeInterfaces {
    /// The spelling in `system.toml` and in the overlay.
    pub fn as_str(self) -> &'static str {
        match self {
            NodeInterfaces::Stock => "stock",
            NodeInterfaces::None => "none",
        }
    }

    /// Parse the spelling; anything else is an error naming both.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "stock" => Ok(NodeInterfaces::Stock),
            "none" => Ok(NodeInterfaces::None),
            other => Err(format!(
                "`{KEY} = \"{other}\"` is not one of \"stock\", \"none\""
            )),
        }
    }
}

/// THE function: the capability axes an image is built with, from what it
/// declares and the switch that applies to it (already resolved by
/// [`resolve`]; `None` = the image states no switch, so
/// [`DEFAULT_NODE_INTERFACES`] applies).
///
/// `Stock` appends each stock axis the declaration lacks; `None` removes every
/// stock axis, declared or not, so an image can opt out of a set a broader
/// table (`[system]`) declared for every image. Order is preserved, the first
/// spelling kept.
pub fn effective_features<S: AsRef<str>>(
    declared: &[S],
    switch: Option<NodeInterfaces>,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for f in declared {
        let f = f.as_ref();
        if !out.iter().any(|o| o == f) {
            out.push(f.to_string());
        }
    }
    match switch.or(DEFAULT_NODE_INTERFACES) {
        None => {}
        Some(NodeInterfaces::Stock) => {
            for s in STOCK {
                if !out.iter().any(|o| o == s) {
                    out.push((*s).to_string());
                }
            }
        }
        Some(NodeInterfaces::None) => out.retain(|f| !STOCK.contains(&f.as_str())),
    }
    out
}

/// The switch that applies: the most specific table that states one --
/// `[image.<id>]`, then `[image_defaults]`, then `[system]`.
pub fn resolve(
    image: Option<NodeInterfaces>,
    image_defaults: Option<NodeInterfaces>,
    system: Option<NodeInterfaces>,
) -> Option<NodeInterfaces> {
    image.or(image_defaults).or(system)
}

/// Read the switch from one parsed `system.toml` table (`[system]`,
/// `[image_defaults]` or `[image.<id>]`). `what` names the table in the
/// error. Absent table or key = `None`.
pub fn from_table(t: Option<&toml::Value>, what: &str) -> Result<Option<NodeInterfaces>, String> {
    let Some(v) = t.and_then(|t| t.get(KEY)) else {
        return Ok(None);
    };
    let s = v
        .as_str()
        .ok_or_else(|| format!("`{what} {KEY}` must be a string, \"stock\" or \"none\""))?;
    NodeInterfaces::parse(s)
        .map(Some)
        .map_err(|e| format!("`{what}`: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_as_declared_until_the_flip() {
        assert_eq!(DEFAULT_NODE_INTERFACES, None);
        let none: [&str; 0] = [];
        assert!(effective_features(&none, None).is_empty());
        assert_eq!(
            effective_features(&["lifecycle", "param_services"], None),
            vec!["lifecycle", "param_services"]
        );
    }

    #[test]
    fn stock_adds_the_stock_set_once() {
        let none: [&str; 0] = [];
        assert_eq!(
            effective_features(&none, Some(NodeInterfaces::Stock)),
            vec!["param_services", "rosout"]
        );
        assert_eq!(
            effective_features(&["rosout", "lifecycle"], Some(NodeInterfaces::Stock)),
            vec!["rosout", "lifecycle", "param_services"]
        );
    }

    #[test]
    fn none_removes_the_stock_set_and_keeps_the_rest() {
        assert_eq!(
            effective_features(
                &["param_services", "lifecycle", "rosout", "safety"],
                Some(NodeInterfaces::None)
            ),
            vec!["lifecycle", "safety"]
        );
    }

    #[test]
    fn the_most_specific_table_wins() {
        use NodeInterfaces::{None as Off, Stock};
        assert_eq!(resolve(Some(Off), Some(Stock), Some(Stock)), Some(Off));
        assert_eq!(resolve(None, Some(Off), Some(Stock)), Some(Off));
        assert_eq!(resolve(None, None, Some(Stock)), Some(Stock));
        assert_eq!(resolve(None, None, None), None);
    }

    #[test]
    fn the_key_refuses_an_unknown_word_and_a_non_string() {
        let t: toml::Value = toml::from_str("node_interfaces = \"all\"").unwrap();
        let e = from_table(Some(&t), "[system]").unwrap_err();
        assert!(e.contains("\"stock\", \"none\""), "{e}");
        let t: toml::Value = toml::from_str("node_interfaces = true").unwrap();
        assert!(from_table(Some(&t), "[system]").is_err());
        let t: toml::Value = toml::from_str("node_interfaces = \"stock\"").unwrap();
        assert_eq!(
            from_table(Some(&t), "[system]").unwrap(),
            Some(NodeInterfaces::Stock)
        );
        assert_eq!(from_table(None, "[system]").unwrap(), None);
    }
}
