//! Issue 2174 -- service contract facts the selected backend cannot carry.
//!
//! A server contract may state `stale_after:` (rlm v0.2.5, play_launch phase
//! 95): the server drops a request older than the bound. The server can do that
//! only if the request tells it when it was sent, i.e. if the backend fills
//! `ServiceInfo.source_timestamp` (issue 1942). That is a property of the
//! backend's wire, and core may not name a backend (RFC-0071), so the backend
//! states it in its `nros-rmw.toml` (`[rmw.facts] request_source_timestamp`)
//! and this check asks for the fact by key
//! (`cargo_nano_ros::rmw_resolver::backend_states_fact`).
//!
//! A WARNING, not a refusal, on the `[safety]` precedent in
//! `planner::collect_plan_warnings`: the contract states something the backend
//! silently cannot carry, and play_launch still charges the fact in its own
//! arithmetic. A backend that states the fact `true`, or does not state it at
//! all (unknown), gets no warning: unknown is never read as false.
//!
//! Nothing here derives a deadline or a monitor from `stale_after` (RFC-0052's
//! contract table): the image judges no staleness on any backend. The warning
//! says that on THIS backend the server's own code cannot either.

use ros_launch_manifest_model::SystemModel;

/// The `[rmw.facts]` key this check reads.
pub const REQUEST_SOURCE_TIMESTAMP: &str = "request_source_timestamp";

/// The warnings for one image, using the real backend descriptors.
///
/// `members` limits the check to the servers whose node this image runs (a
/// bare node name, as the bake roads key their tables); `None` checks every
/// server in the model. A server's backend is its node's
/// `execution.deploy.<fqn>.rmw` when the model states one, else `image_rmw`;
/// with neither, the server is not checked.
pub fn stale_after_warnings(
    model: &SystemModel,
    members: Option<&[&str]>,
    image_rmw: Option<&str>,
) -> Vec<String> {
    stale_after_warnings_with(model, members, image_rmw, |rmw| {
        let canonical = cargo_nano_ros::rmw_resolver::canonical_rmw(rmw)?;
        cargo_nano_ros::rmw_resolver::backend_states_fact(canonical, REQUEST_SOURCE_TIMESTAMP)
    })
}

/// [`stale_after_warnings`] with the fact lookup injected, for tests.
pub fn stale_after_warnings_with(
    model: &SystemModel,
    members: Option<&[&str]>,
    image_rmw: Option<&str>,
    fills_source_timestamp: impl Fn(&str) -> Option<bool>,
) -> Vec<String> {
    let mut out = Vec::new();
    for (endpoint, srv) in &model.contracts.srv_endpoints {
        let Some(stale_after_ms) = srv.stale_after_ms else {
            continue;
        };
        let node_fqn = endpoint.rsplit_once('/').map_or("", |(n, _)| n);
        let bare = node_fqn.rsplit('/').next().unwrap_or(node_fqn);
        if let Some(members) = members
            && !members.contains(&bare)
        {
            continue;
        }
        let deploy_rmw = model
            .execution
            .deploy
            .get(node_fqn)
            .and_then(|d| d.rmw.as_deref())
            .filter(|r| !r.is_empty());
        let Some(rmw) = deploy_rmw.or(image_rmw).filter(|r| !r.is_empty()) else {
            continue;
        };
        if fills_source_timestamp(rmw) == Some(false) {
            out.push(format!(
                "server `{endpoint}` declares `stale_after: {stale_after_ms} ms`, but the \
                 resolved RMW `{rmw}` states `[rmw.facts] {REQUEST_SOURCE_TIMESTAMP} = false`: \
                 a request reaches the server with no source timestamp, so the image cannot \
                 judge staleness on this backend and the server cannot honour `stale_after`. \
                 play_launch still charges it in its fault-reaction arithmetic -- switch the \
                 RMW or drop `stale_after`."
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(stale_after: &str, deploy_rmw: &str) -> SystemModel {
        let deploy = if deploy_rmw.is_empty() {
            String::new()
        } else {
            format!("execution:\n  deploy:\n    /srv_node:\n      rmw: {deploy_rmw}\n")
        };
        let yaml = format!(
            "meta:\n  version: 1\nstructure:\n  scopes:\n    d.launch.xml:\n      \
             file: d.launch.xml\n  nodes:\n    /srv_node:\n      scope: d.launch.xml\n      \
             pkg: p\n      exec: srv_node\n      node_name: srv_node\n      namespace: /\n\
             contracts:\n  srv_endpoints:\n    /srv_node/lookup:\n{stale_after}{deploy}"
        );
        SystemModel::from_yaml_str(&yaml).expect("fixture parses")
    }

    const STALE: &str = "      stale_after_ms: 50.0\n";

    #[test]
    fn a_backend_that_states_false_warns_naming_the_server_and_the_key() {
        let w = stale_after_warnings_with(&model(STALE, ""), None, Some("acme"), |_| Some(false));
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("/srv_node/lookup"), "{}", w[0]);
        assert!(
            w[0].contains("request_source_timestamp = false"),
            "{}",
            w[0]
        );
        assert!(w[0].contains("cannot judge staleness"), "{}", w[0]);
    }

    #[test]
    fn a_backend_that_states_true_or_nothing_is_silent() {
        let m = model(STALE, "");
        assert!(stale_after_warnings_with(&m, None, Some("acme"), |_| Some(true)).is_empty());
        assert!(stale_after_warnings_with(&m, None, Some("acme"), |_| None).is_empty());
    }

    #[test]
    fn a_server_without_stale_after_or_without_a_backend_is_silent() {
        let none = model("      idempotent: true\n", "");
        assert!(stale_after_warnings_with(&none, None, Some("acme"), |_| Some(false)).is_empty());
        let m = model(STALE, "");
        assert!(stale_after_warnings_with(&m, None, None, |_| Some(false)).is_empty());
        // Another image's server is not this image's warning.
        assert!(
            stale_after_warnings_with(&m, Some(&["other"]), Some("acme"), |_| Some(false))
                .is_empty()
        );
        assert_eq!(
            stale_after_warnings_with(&m, Some(&["srv_node"]), Some("acme"), |_| Some(false)).len(),
            1
        );
    }

    /// The node's deploy RMW wins over the image's.
    #[test]
    fn the_deploy_rmw_is_the_servers_backend() {
        let m = model(STALE, "bad");
        let w = stale_after_warnings_with(&m, None, Some("good"), |r| Some(r != "bad"));
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("`bad`"), "{}", w[0]);
    }

    /// The real descriptors: XRCE-DDS states `false`, zenoh and Cyclone DDS
    /// `true`, and uORB states nothing (unknown, so silent).
    #[test]
    fn the_in_tree_descriptors_state_the_fact() {
        let m = model(STALE, "");
        assert_eq!(stale_after_warnings(&m, None, Some("xrce")).len(), 1);
        assert!(stale_after_warnings(&m, None, Some("zenoh")).is_empty());
        assert!(stale_after_warnings(&m, None, Some("cyclonedds")).is_empty());
        assert_eq!(
            cargo_nano_ros::rmw_resolver::backend_states_fact("uorb", REQUEST_SOURCE_TIMESTAMP),
            None
        );
        assert!(stale_after_warnings(&m, None, Some("uorb")).is_empty());
    }
}
