//! ROS 2 name expansion + launch remap resolution (issue 0255 / phase-306 W3).
//!
//! The ONE resolution seam both entry-codegen twins funnel through: the Rust
//! `ExecutorSink` (nros `node_runtime`) and the C-ABI registration paths
//! (nros-c / nros-cpp via the executor-side remap table) call these functions
//! so the two languages can never drift on name semantics.
//!
//! Scope: **basic name remapping only** — a rule matches when its expanded
//! `from` equals the expanded source name (exact FQN comparison, no
//! wildcards, no node-name prefixes). First matching rule wins.
//!
//! ## Two stores, one seam
//!
//! The rules themselves are held in two different places, and this module is
//! the only thing both reach: the C and C++ roads store
//! `executor::spin::RemapRule` in the executor's fixed table (filled by
//! `Executor::declare_remap`, which the generated entry calls before the
//! component configure), while the Rust road stores a bare
//! `&[(&str, &str)]` on `nros_platform::RuntimeCtx` that `nros::main!` bakes
//! per component and `ExecutorSink` threads to `create_entity`. Neither road
//! reads the other's store. So anything about PRECEDENCE has to live here to
//! be true of both — see [`resolve_name_layered`].

/// Maximum bytes in a fully-qualified resolved entity name. Matches
/// `nros::node_metadata::METADATA_STRING_CAPACITY` (the source-name bound
/// entering the resolution seam).
pub const MAX_RESOLVED_NAME_LEN: usize = 128;

/// Owned storage for a resolved (fully-qualified) entity name.
pub type ResolvedName = heapless::String<MAX_RESOLVED_NAME_LEN>;

/// A node's fully-qualified name, joined from its two halves.
///
/// `/robot` + `talker` -> `/robot/talker`; the root namespace collapses, so
/// `/` + `talker` is `/talker` and never `//talker`. The namespace may be given
/// with or without a leading `/`, and empty means root — the same input rules
/// [`expand_name`] already accepts, because this IS `expand_name`'s private-name
/// branch under the name people look for.
///
/// # Why a named function for one call
///
/// The join is three lines and the tree already contained THREE of them —
/// `Node::fully_qualified_name`, the inline build in
/// `Executor::register_parameter_services`, and this branch — which is the
/// "one shared helper, never a second spelling" rule pointing at itself. It is
/// also not discoverable: nothing says that `expand_name("~", …)` is how you
/// ask for a node's own name, so the fourth caller writes a fourth copy and
/// gets the root case wrong. `//talker` is the failure this exists to stop.
///
/// The counterpart in ROS 2 is `rcl_node_get_fully_qualified_name` /
/// `rclcpp::Node::get_fully_qualified_name`, but both answer only for the
/// CALLER'S OWN node. Neither project has this one — the join for a node you
/// DISCOVERED — because their graph APIs hand back two parallel arrays and
/// leave correlating them to you. Ours delivers both halves in one visitor
/// call, so the join is the only step left.
#[allow(clippy::result_unit_err)] // as `expand_name`, which this is — no_std, no Error type
pub fn fully_qualified_name(node_name: &str, namespace: &str) -> Result<ResolvedName, ()> {
    expand_name("~", node_name, namespace)
}

/// Expand a source-level ROS name to its fully-qualified form (ROS 2 name
/// expansion rules):
///
/// - `/absolute/name` → unchanged.
/// - `~` / `~/rest` (private) → `/<ns>/<node>` / `/<ns>/<node>/rest`
///   (`ns == "/"` collapses: `/<node>/rest`).
/// - `relative/name` → `/<ns>/relative/name` (`ns == "/"` collapses:
///   `/relative/name`).
///
/// `namespace` may be given with or without a leading `/`; empty means root.
/// Errors on: empty `source`, a private name with an empty `node_name`, or a
/// result exceeding [`MAX_RESOLVED_NAME_LEN`].
#[allow(clippy::result_unit_err)] // matches the RuntimeCtx seam precedent — no_std, no Error type
pub fn expand_name(source: &str, node_name: &str, namespace: &str) -> Result<ResolvedName, ()> {
    if source.is_empty() {
        return Err(());
    }
    let mut out = ResolvedName::new();
    if source.starts_with('/') {
        out.push_str(source)?;
        return Ok(out);
    }
    push_namespace(&mut out, namespace)?;
    if let Some(rest) = source.strip_prefix('~') {
        if node_name.is_empty() {
            return Err(());
        }
        out.push('/')?;
        out.push_str(node_name)?;
        let rest = rest.strip_prefix('/').unwrap_or(rest);
        if !rest.is_empty() {
            out.push('/')?;
            out.push_str(rest)?;
        }
    } else {
        out.push('/')?;
        out.push_str(source)?;
    }
    Ok(out)
}

/// Append a normalized namespace: leading `/` guaranteed, trailing `/`
/// stripped, root (`""` / `"/"`) appends nothing (the caller's `/` before the
/// next segment is the only separator — the "ns=/ collapse").
fn push_namespace(out: &mut ResolvedName, namespace: &str) -> Result<(), ()> {
    let ns = namespace.trim_end_matches('/');
    if ns.is_empty() {
        return Ok(());
    }
    if !ns.starts_with('/') {
        out.push('/')?;
    }
    out.push_str(ns)
}

/// The first rule in `rules` whose expanded `from` equals `expanded`, as its
/// expanded `to`. A rule whose `from`/`to` fails to expand is skipped (never
/// masks the name expansion itself).
fn first_substitution<'a, I>(
    expanded: &ResolvedName,
    node_name: &str,
    namespace: &str,
    rules: I,
) -> Option<ResolvedName>
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    for (from, to) in rules {
        if let Ok(from_fq) = expand_name(from, node_name, namespace)
            && &from_fq == expanded
            && let Ok(to_fq) = expand_name(to, node_name, namespace)
        {
            return Some(to_fq);
        }
    }
    None
}

/// Resolve a source-level entity name through launch remap rules: expand the
/// source name AND each rule's `from` to fully-qualified form, compare exact,
/// substitute the (also expanded) `to` of the first matching rule; no match →
/// the expanded source name. A rule whose `from`/`to` fails to expand is
/// skipped (never masks the name expansion itself).
///
/// This is the single-tier spelling: **everything passed here is
/// AUTHORITATIVE**, and precedence among the rules is iterator order. When a
/// caller has rules from more than one channel, it must say which channel
/// outranks which — [`resolve_name_layered`] is that call, and it is the only
/// place a precedence between channels may be expressed.
#[allow(clippy::result_unit_err)]
pub fn resolve_name<'a, I>(
    source: &str,
    node_name: &str,
    namespace: &str,
    remaps: I,
) -> Result<ResolvedName, ()>
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    resolve_name_layered(
        source,
        node_name,
        namespace,
        remaps,
        core::iter::empty::<(&str, &str)>(),
    )
}

/// [`resolve_name`] with the rules split into two PROVENANCE TIERS: every
/// `authoritative` rule is consulted before any `fallback` rule, whatever
/// order either was declared in.
///
/// # Why a tier and not a field on the rule
///
/// A remap's provenance only ever decides one thing — which rule wins when two
/// channels name the same entity — and the tree has no single rule TYPE to hang
/// it on: the C and C++ roads store `executor::spin::RemapRule`, the Rust road
/// stores a bare `&[(&str, &str)]` on `RuntimeCtx`. The one thing both roads
/// share is this function, so the tier is an ARGUMENT here rather than a field
/// there. That also makes the guarantee structural instead of documentary: a
/// caller cannot promote a fallback rule by declaring it first, because the two
/// tiers are different parameters and the loop below can never interleave them.
///
/// # The rule this encodes
///
/// **Launch is authoritative; anything else is a fallback for a name the launch
/// projected no rule for.** That is RFC-0046's precedence for node identity
/// (launch overrides the code-provided default) applied to the remap half of
/// the same question, and it is rcl's own arrangement: `rcl_remap_topic_name`
/// checks the node's LOCAL arguments (how a launch file or component container
/// hands a node its rules) and only on no match the process's GLOBAL ones (the
/// argv given to `rcl_init`) — Humble `rcl/remap.h`.
///
/// The producers: both roads pass their codegen-projected launch rules as
/// `authoritative`; the fallback is the `--ros-args` parse
/// ([`crate::ros_args`]), which `nros::init_with_args` carries on its
/// `Context` into every executor that context creates.
#[allow(clippy::result_unit_err)]
pub fn resolve_name_layered<'a, 'b, A, F>(
    source: &str,
    node_name: &str,
    namespace: &str,
    authoritative: A,
    fallback: F,
) -> Result<ResolvedName, ()>
where
    A: IntoIterator<Item = (&'a str, &'a str)>,
    F: IntoIterator<Item = (&'b str, &'b str)>,
{
    let expanded = expand_name(source, node_name, namespace)?;
    Ok(
        substitute_layered(&expanded, node_name, namespace, authoritative, fallback)
            .unwrap_or(expanded),
    )
}

fn substitute_layered<'a, 'b, A, F>(
    expanded: &ResolvedName,
    node_name: &str,
    namespace: &str,
    authoritative: A,
    fallback: F,
) -> Option<ResolvedName>
where
    A: IntoIterator<Item = (&'a str, &'a str)>,
    F: IntoIterator<Item = (&'b str, &'b str)>,
{
    first_substitution(expanded, node_name, namespace, authoritative)
        .or_else(|| first_substitution(expanded, node_name, namespace, fallback))
}

/// [`resolve_name_layered`]'s SUBSTITUTION, without its expansion: the name the
/// first matching rule (authoritative tier first) sends `source` to, or `None`
/// when no rule matches or `source` does not expand.
///
/// For a caller that has never expanded names and must not start: on `None` it
/// passes `source` through exactly as before, so a node with no matching rule
/// sees no change at all. `NodeHandle` is that caller.
pub fn remap_layered<'a, 'b, A, F>(
    source: &str,
    node_name: &str,
    namespace: &str,
    authoritative: A,
    fallback: F,
) -> Option<ResolvedName>
where
    A: IntoIterator<Item = (&'a str, &'a str)>,
    F: IntoIterator<Item = (&'b str, &'b str)>,
{
    let expanded = expand_name(source, node_name, namespace).ok()?;
    substitute_layered(&expanded, node_name, namespace, authoritative, fallback)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_passes_through() {
        assert_eq!(
            expand_name("/scan", "lidar", "/sensing").unwrap().as_str(),
            "/scan"
        );
    }

    #[test]
    fn relative_expands_against_namespace() {
        assert_eq!(
            expand_name("scan", "lidar", "/sensing").unwrap().as_str(),
            "/sensing/scan"
        );
        // Namespace without a leading slash normalizes.
        assert_eq!(
            expand_name("scan", "lidar", "sensing").unwrap().as_str(),
            "/sensing/scan"
        );
    }

    #[test]
    fn relative_root_namespace_collapses() {
        assert_eq!(expand_name("scan", "lidar", "/").unwrap().as_str(), "/scan");
        assert_eq!(expand_name("scan", "lidar", "").unwrap().as_str(), "/scan");
    }

    #[test]
    fn private_expands_against_node_fqn() {
        assert_eq!(
            expand_name("~/input/points", "filter", "/sensing")
                .unwrap()
                .as_str(),
            "/sensing/filter/input/points"
        );
        // ns=/ collapse.
        assert_eq!(
            expand_name("~/status", "filter", "/").unwrap().as_str(),
            "/filter/status"
        );
        // Bare `~` names the node itself.
        assert_eq!(
            expand_name("~", "filter", "/sensing").unwrap().as_str(),
            "/sensing/filter"
        );
    }

    #[test]
    fn private_without_node_name_errors() {
        assert!(expand_name("~/x", "", "/").is_err());
    }

    #[test]
    fn empty_source_errors() {
        assert!(expand_name("", "n", "/").is_err());
    }

    #[test]
    fn oversized_result_errors() {
        let long = "x".repeat(MAX_RESOLVED_NAME_LEN);
        assert!(expand_name(&long, "n", "/ns").is_err());
    }

    #[test]
    fn remap_matches_on_expanded_fqn() {
        // `from` written relative, source written private — both expand to the
        // same FQN, so the rule fires; `to` expands as well.
        let remaps = [("filter/input/points", "/sensing/points_raw")];
        let r = resolve_name("~/input/points", "filter", "/", remaps).unwrap();
        assert_eq!(r.as_str(), "/sensing/points_raw");
    }

    #[test]
    fn first_matching_rule_wins() {
        let remaps = [("/a", "/first"), ("/a", "/second")];
        assert_eq!(
            resolve_name("/a", "n", "/", remaps).unwrap().as_str(),
            "/first"
        );
    }

    // ---------------------------------------------------------------------
    // The precedence between remap CHANNELS (`resolve_name_layered`).
    //
    // `first_matching_rule_wins` above is precedence WITHIN one channel, and
    // it is iterator order. These three pin the other axis: a fallback rule
    // can never outrank an authoritative one, and declaration order cannot
    // change that.
    // ---------------------------------------------------------------------

    #[test]
    fn authoritative_rule_wins_over_a_fallback_for_the_same_name() {
        // The mutation this refuses: concatenating the two channels into one
        // flat iterator. Written that way the fallback rule is FIRST, so
        // `first_matching_rule_wins` would hand it the name.
        let launch = [("/a", "/from_launch")];
        let argv = [("/a", "/from_argv")];
        assert_eq!(
            resolve_name_layered("/a", "n", "/", launch, argv)
                .unwrap()
                .as_str(),
            "/from_launch"
        );
        // Same assertion with the flat concatenation spelled out, so the test
        // says what it is protecting against rather than only what it wants.
        assert_eq!(
            resolve_name("/a", "n", "/", argv.into_iter().chain(launch))
                .unwrap()
                .as_str(),
            "/from_argv"
        );
    }

    #[test]
    fn fallback_applies_where_the_authoritative_tier_has_no_rule() {
        let launch = [("/b", "/from_launch")];
        let argv = [("/a", "/from_argv")];
        assert_eq!(
            resolve_name_layered("/a", "n", "/", launch, argv)
                .unwrap()
                .as_str(),
            "/from_argv"
        );
    }

    #[test]
    fn resolve_name_is_the_all_authoritative_case() {
        let remaps = [("/a", "/one")];
        assert_eq!(
            resolve_name("/a", "n", "/", remaps).unwrap().as_str(),
            resolve_name_layered("/a", "n", "/", remaps, core::iter::empty::<(&str, &str)>())
                .unwrap()
                .as_str(),
        );
    }

    #[test]
    fn no_match_returns_expansion() {
        let remaps = [("/other", "/elsewhere")];
        assert_eq!(
            resolve_name("chatter", "n", "/ns", remaps)
                .unwrap()
                .as_str(),
            "/ns/chatter"
        );
    }

    #[test]
    fn relative_to_expands_against_namespace() {
        let remaps = [("chatter", "chatter_remapped")];
        assert_eq!(
            resolve_name("chatter", "n", "/ns", remaps)
                .unwrap()
                .as_str(),
            "/ns/chatter_remapped"
        );
    }
}
