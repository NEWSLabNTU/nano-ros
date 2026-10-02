//! `--ros-args` parsing: the producer of the remap FALLBACK tier.
//!
//! [`crate::names::resolve_name_layered`] resolves a name through two tiers —
//! the launch projection (authoritative) and a fallback. This module is the
//! fallback's producer: it reads a process argument vector the way rcl does
//! and yields the remap rules it carries.
//!
//! # Whose grammar
//!
//! rcl's, as documented in ROS 2 Humble's installed headers
//! (`rcl/arguments.h`, `rcl_parse_arguments`; `rcl/remap.h`,
//! `rcl_remap_topic_name`):
//!
//! * ROS arguments are scoped by `--ros-args` and end at `--` or at the end of
//!   the vector. A vector may open several scopes. Everything outside a scope
//!   is a non-ROS argument and is left alone.
//! * `-r` / `--remap` is followed by ONE token, `[node:]from:=to`. A `node:`
//!   prefix restricts the rule to the node of that NAME.
//! * Rules are kept in argv order, and the first rule that matches a name wins
//!   — `foo:=bar bar:=baz` sends `foo` to `bar`, never `baz`.
//! * A `-r` followed by anything but a valid rule fails the parse.
//!
//! # What is REFUSED, and why each one
//!
//! Every flag this parser does not honour is a [`RosArgsError`], never a skip.
//! Silently dropping a ROS argument is the defect `nros::init_with_args` was
//! written to stop (a `-r chatter:=/other` that compiles and then publishes on
//! `chatter`). rclcpp is no more lenient: `rclcpp::init` throws
//! `UnknownROSArgsError` for any ROS argument rcl left unparsed.
//!
//! * `-p` / `--param` / `--params-file` — parameter overrides. RFC-0015 §9
//!   owns the runtime-parameter channel (`nros_runtime_args_get`,
//!   `apply_runtime_param_overrides`, "writes into the same parameter store
//!   that `~/set_parameters` reads"); building a second answer here would be
//!   the thing that RFC's open §9.6 has not decided yet.
//! * `__node` / `__name` / `__ns` remaps — node IDENTITY. RFC-0046 makes the
//!   launch the authority on a node's name and namespace, with the code's
//!   default beneath it; an argv identity rung has not been ruled on.
//! * `rostopic://` / `rosservice://` rules — kind-qualified remaps, which rcl
//!   itself marks unfinished (`TODO(sloretz) add documentation about
//!   rostopic:// when it is supported`).
//! * `-e`/`--enclave`, `--log-level`, `--log-config-file`, the
//!   `--{enable,disable}-*-logs` switches, and any other token inside a scope.
//!
//! # `no_std`, no allocation
//!
//! The parse borrows from the arguments it is given and reports each rule
//! through a callback, so a board that reads its arguments from flash or a
//! settings store (RFC-0015 §9.4's other rows) can drive it without an
//! allocator.

/// The `-r`/`--remap` rule one argument carried: `[node:]from:=to`, borrowed
/// from the argument it was parsed out of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemapArg<'a> {
    /// `Some(name)` for a `name:from:=to` rule, which applies only to the node
    /// of that name (rcl matches the node NAME, not its namespace). `None`
    /// applies to every node in the process.
    pub node: Option<&'a str>,
    /// The name being remapped, as written (expanded at resolution time).
    pub from: &'a str,
    /// The replacement, as written (expanded at resolution time).
    pub to: &'a str,
}

/// Why an argument vector was refused. Each variant names the offending token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RosArgsError<'a> {
    /// A recognised ROS flag nano-ros does not honour (see the module docs for
    /// the reason each one is refused).
    Unsupported {
        /// The flag as written.
        flag: &'a str,
    },
    /// A token inside a `--ros-args` scope that is not a ROS flag at all.
    Unknown {
        /// The token as written.
        token: &'a str,
    },
    /// `-r`/`--remap` was the last argument.
    MissingRule {
        /// The flag as written.
        flag: &'a str,
    },
    /// `-r`/`--remap` was followed by something that is not `[node:]from:=to`.
    InvalidRule {
        /// The rule as written.
        rule: &'a str,
    },
    /// A node-identity remap (`__node`, `__name`, `__ns`, or any `__` rule).
    IdentityRemap {
        /// The rule as written.
        rule: &'a str,
    },
}

impl core::fmt::Display for RosArgsError<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Unsupported { flag } => write!(
                f,
                "`{flag}` is a ROS argument nano-ros does not honour (parameter overrides belong \
                 to RFC-0015 section 9's runtime-parameter channel; enclaves and log \
                 configuration have no counterpart)"
            ),
            Self::Unknown { token } => {
                write!(
                    f,
                    "`{token}` is not a ROS argument (rclcpp refuses it too, as UnknownROSArgsError)"
                )
            }
            Self::MissingRule { flag } => {
                write!(f, "`{flag}` must be followed by a rule `[node:]from:=to`")
            }
            Self::InvalidRule { rule } => {
                write!(
                    f,
                    "`{rule}` is not a remap rule; expected `[node:]from:=to`"
                )
            }
            Self::IdentityRemap { rule } => write!(
                f,
                "`{rule}` remaps a node's identity; the launch file is the authority on a node's \
                 name and namespace (RFC-0046), and an argv rung beneath it is not implemented"
            ),
        }
    }
}

/// Flags rcl defines that this parser recognises and refuses. Kept as data so
/// the refusal names the flag rather than calling it unknown.
const UNSUPPORTED_FLAGS: &[&str] = &[
    "-p",
    "--param",
    "--params-file",
    "-e",
    "--enclave",
    "--log-level",
    "--log-config-file",
];

/// rcl's log-switch flags are `--enable-<suffix>` / `--disable-<suffix>` over
/// these three suffixes (`RCL_LOG_*_FLAG_SUFFIX`).
const LOG_SWITCH_SUFFIXES: &[&str] = &["stdout-logs", "rosout-logs", "external-lib-logs"];

fn is_log_switch(token: &str) -> bool {
    let rest = token
        .strip_prefix("--enable-")
        .or_else(|| token.strip_prefix("--disable-"));
    matches!(rest, Some(s) if LOG_SWITCH_SUFFIXES.contains(&s))
}

/// Parse one `[node:]from:=to` rule.
pub fn parse_remap_rule(rule: &str) -> Result<RemapArg<'_>, RosArgsError<'_>> {
    let invalid = RosArgsError::InvalidRule { rule };
    let (lhs, to) = rule.split_once(":=").ok_or(invalid)?;
    if lhs.contains("://") || to.contains("://") {
        // `rostopic://` / `rosservice://` — rcl's own unfinished form.
        return Err(RosArgsError::Unsupported { flag: rule });
    }
    let (node, from) = match lhs.split_once(':') {
        Some((node, from)) => (Some(node), from),
        None => (None, lhs),
    };
    if from.starts_with("__") {
        return Err(RosArgsError::IdentityRemap { rule });
    }
    let bad = |s: &str| s.is_empty() || s.contains(':') || s.chars().any(char::is_whitespace);
    if bad(from) || bad(to) {
        return Err(invalid);
    }
    if let Some(n) = node
        && (bad(n) || n.contains('/') || n.contains('~'))
    {
        return Err(invalid);
    }
    Ok(RemapArg { node, from, to })
}

/// Parse `args` the way `rcl_parse_arguments` does, reporting each remap rule
/// in argv order through `on_remap`.
///
/// Returns the first refusal, naming its token; `on_remap` may have been
/// called for rules before it, so a caller that must be all-or-nothing should
/// collect and only commit on `Ok`. Arguments outside every `--ros-args` scope
/// are ignored, so a vector with no `--ros-args` reports nothing and succeeds.
pub fn parse_ros_args<'a, I, F>(args: I, mut on_remap: F) -> Result<(), RosArgsError<'a>>
where
    I: IntoIterator<Item = &'a str>,
    F: FnMut(RemapArg<'a>),
{
    let mut in_scope = false;
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        if arg == "--ros-args" {
            in_scope = true;
            continue;
        }
        if !in_scope {
            continue;
        }
        match arg {
            "--" => in_scope = false,
            "-r" | "--remap" => {
                let rule = it.next().ok_or(RosArgsError::MissingRule { flag: arg })?;
                on_remap(parse_remap_rule(rule)?);
            }
            flag if UNSUPPORTED_FLAGS.contains(&flag) || is_log_switch(flag) => {
                return Err(RosArgsError::Unsupported { flag });
            }
            token => return Err(RosArgsError::Unknown { token }),
        }
    }
    Ok(())
}

#[cfg(any(has_rmw, test))]
/// Install parsed `-r` rules in `executor`'s remap table as its FALLBACK tier.
///
/// They then reach every entity the executor creates on every road —
/// `Executor::resolve_entity_name_for` (C, C++), the Rust component sink
/// (through [`argv_fallback`]) and `Executor::create_node`'s handle — and in
/// each, a launch rule for the same name still wins.
///
/// All or nothing: on failure nothing is installed and the rule that did not
/// fit is returned (a full table — `MAX_REMAPS` slots are shared with launch
/// rules — or a name past its slot's length). Silently installing a prefix of
/// the rules would be a partial version of the drop this exists to prevent.
pub fn install_argv_remaps<'r, I>(
    executor: &mut crate::executor::Executor<'_>,
    rules: I,
) -> Result<(), RemapArg<'r>>
where
    I: IntoIterator<Item = RemapArg<'r>>,
    I::IntoIter: Clone,
{
    use crate::executor::spin::{RemapRule, RemapTier};
    let rules = rules.into_iter();
    let start = executor.remap_len;
    for (i, r) in rules.clone().enumerate() {
        let mut rule = RemapRule {
            node_name: heapless::String::new(),
            namespace: heapless::String::new(),
            from: heapless::String::new(),
            to: heapless::String::new(),
            tier: RemapTier::Argv,
        };
        let fits = rule.node_name.push_str(r.node.unwrap_or("")).is_ok()
            && rule.from.push_str(r.from).is_ok()
            && rule.to.push_str(r.to).is_ok();
        let slot = executor.remap_table.get_mut(start + i);
        match (fits, slot) {
            (true, Some(slot)) => *slot = Some(rule),
            _ => {
                for s in &mut executor.remap_table[start..start + i] {
                    *s = None;
                }
                return Err(r);
            }
        }
    }
    executor.remap_len = start + rules.count();
    Ok(())
}

#[cfg(any(has_rmw, test))]
/// The installed argv rules that apply to the node named `node_name` (in
/// `namespace`), as a fallback tier for [`crate::names::resolve_name_layered`].
///
/// For a resolver that keeps its AUTHORITATIVE rules somewhere other than the
/// executor's table — the Rust component sink holds its launch rules as a
/// slice on `RuntimeCtx` — and so cannot use
/// `Executor::resolve_entity_name_for` directly.
pub fn argv_fallback<'e>(
    executor: &'e crate::executor::Executor<'_>,
    node_name: &'e str,
    namespace: &'e str,
) -> impl Iterator<Item = (&'e str, &'e str)> + 'e {
    use crate::executor::spin::{RemapTier, tier_rules};
    tier_rules(
        &executor.remap_table[..executor.remap_len],
        RemapTier::Argv,
        node_name,
        namespace,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules<'a>(args: &[&'a str]) -> Result<heapless::Vec<RemapArg<'a>, 8>, RosArgsError<'a>> {
        let mut out = heapless::Vec::new();
        parse_ros_args(args.iter().copied(), |r| out.push(r).unwrap())?;
        Ok(out)
    }

    fn rule<'a>(node: Option<&'a str>, from: &'a str, to: &'a str) -> RemapArg<'a> {
        RemapArg { node, from, to }
    }

    #[test]
    fn no_scope_reports_nothing_and_leaves_user_arguments_alone() {
        assert!(
            rules(&["prog", "-r", "a:=b", "--verbose"])
                .unwrap()
                .is_empty()
        );
        assert!(rules(&[]).unwrap().is_empty());
    }

    #[test]
    fn short_and_long_remap_flags_keep_argv_order() {
        let got = rules(&[
            "prog",
            "--ros-args",
            "-r",
            "foo:=bar",
            "--remap",
            "bar:=baz",
        ])
        .unwrap();
        assert_eq!(
            got.as_slice(),
            &[rule(None, "foo", "bar"), rule(None, "bar", "baz")]
        );
    }

    #[test]
    fn a_node_prefix_scopes_the_rule() {
        let got = rules(&["--ros-args", "-r", "alice:foo:=~/bar"]).unwrap();
        assert_eq!(got.as_slice(), &[rule(Some("alice"), "foo", "~/bar")]);
    }

    #[test]
    fn double_dash_closes_the_scope_and_a_second_scope_reopens_it() {
        let got = rules(&[
            "--ros-args",
            "-r",
            "a:=b",
            "--",
            "-r",
            "user:=arg",
            "--ros-args",
            "-r",
            "c:=d",
        ])
        .unwrap();
        assert_eq!(
            got.as_slice(),
            &[rule(None, "a", "b"), rule(None, "c", "d")]
        );
    }

    #[test]
    fn parameter_overrides_are_refused_by_name() {
        for flag in ["-p", "--param", "--params-file"] {
            assert_eq!(
                rules(&["--ros-args", flag, "x:=1"]),
                Err(RosArgsError::Unsupported { flag })
            );
        }
    }

    #[test]
    fn enclave_and_log_flags_are_refused_by_name() {
        for flag in [
            "-e",
            "--enclave",
            "--log-level",
            "--log-config-file",
            "--disable-rosout-logs",
        ] {
            assert_eq!(
                rules(&["--ros-args", flag]),
                Err(RosArgsError::Unsupported { flag })
            );
        }
    }

    #[test]
    fn an_unknown_token_inside_a_scope_is_refused_not_skipped() {
        assert_eq!(
            rules(&["--ros-args", "-r", "a:=b", "--frobnicate"]),
            Err(RosArgsError::Unknown {
                token: "--frobnicate"
            })
        );
        // A look-alike of a real log switch is still unknown, not a switch.
        assert_eq!(
            rules(&["--ros-args", "--enable-everything"]),
            Err(RosArgsError::Unknown {
                token: "--enable-everything"
            })
        );
    }

    #[test]
    fn a_remap_flag_needs_a_valid_rule_after_it() {
        assert_eq!(
            rules(&["--ros-args", "-r"]),
            Err(RosArgsError::MissingRule { flag: "-r" })
        );
        for bad in [
            "nocolon", ":=b", "a:=", "a b:=c", ":a:=b", "n/s:a:=b", "a:=b:c",
        ] {
            assert_eq!(
                rules(&["--ros-args", "--remap", bad]),
                Err(RosArgsError::InvalidRule { rule: bad }),
                "{bad}"
            );
        }
    }

    #[test]
    fn identity_remaps_are_refused() {
        for r in ["__node:=x", "__name:=x", "__ns:=/x", "talker:__ns:=/x"] {
            assert_eq!(
                rules(&["--ros-args", "-r", r]),
                Err(RosArgsError::IdentityRemap { rule: r })
            );
        }
    }

    // ---------------------------------------------------------------------
    // The producer feeding the seam: real argv -> parse -> install -> resolve.
    //
    // `names.rs`'s precedence tests hand `resolve_name_layered` two arrays;
    // these run the whole chain a hosted `init_with_args` caller gets, against
    // the executor's own table and resolver.
    // ---------------------------------------------------------------------

    /// Same gate as `executor::node`'s tests: `Executor::from_session` over a
    /// `MockSession` needs `std` and no linked backend.
    #[cfg(all(feature = "std", not(feature = "rmw-cffi")))]
    mod chain {
        use super::super::*;
        extern crate std;
        use crate::{executor::Executor, mock::MockSession};

        fn executor() -> Executor<'static> {
            Executor::from_session(MockSession::new())
        }

        /// Parse `argv` and install what it carries, the way
        /// `nros::Context::create_executor` does.
        fn install_argv(exec: &mut Executor<'_>, argv: &[&'static str]) {
            let mut parsed: std::vec::Vec<RemapArg<'static>> = std::vec::Vec::new();
            parse_ros_args(argv.iter().copied(), |r| parsed.push(r)).expect("argv parses");
            install_argv_remaps(exec, parsed.iter().copied()).expect("rules fit");
        }

        fn resolve(exec: &Executor<'_>, node: &str, source: &str) -> std::string::String {
            std::string::String::from(
                exec.resolve_entity_name_for(node, "/", source)
                    .unwrap()
                    .as_str(),
            )
        }

        #[test]
        fn a_launch_rule_wins_over_argv_even_when_argv_was_installed_first() {
            // Declaration order argues for the WRONG winner here: the argv rule
            // sits in the table first. A flat first-match over the table (the
            // shape before tiers) hands it the name.
            let mut exec = executor();
            install_argv(
                &mut exec,
                &["prog", "--ros-args", "-r", "chatter:=/from_argv"],
            );
            exec.declare_remap("talker", "/", "chatter", "/from_launch")
                .unwrap();
            assert_eq!(resolve(&exec, "talker", "chatter"), "/from_launch");
        }

        #[test]
        fn an_argv_rule_applies_where_the_launch_projected_none() {
            let mut exec = executor();
            install_argv(
                &mut exec,
                &["prog", "--ros-args", "-r", "chatter:=/from_argv"],
            );
            exec.declare_remap("talker", "/", "chatter", "/from_launch")
                .unwrap();
            // `listener` has no launch rule for `chatter`, so the fallback answers.
            assert_eq!(resolve(&exec, "listener", "chatter"), "/from_argv");
            // ...and a name no rule mentions is only expanded.
            assert_eq!(resolve(&exec, "talker", "other"), "/other");
        }

        #[test]
        fn a_node_prefixed_argv_rule_reaches_only_that_node() {
            let mut exec = executor();
            install_argv(
                &mut exec,
                &["--ros-args", "-r", "talker:chatter:=/only_talker"],
            );
            assert_eq!(resolve(&exec, "talker", "chatter"), "/only_talker");
            assert_eq!(resolve(&exec, "listener", "chatter"), "/chatter");
        }

        #[test]
        fn within_argv_the_first_rule_wins() {
            // rcl: `foo:=bar bar:=baz` sends `foo` to `bar`, never `baz`.
            let mut exec = executor();
            install_argv(
                &mut exec,
                &["--ros-args", "-r", "foo:=bar", "-r", "bar:=baz"],
            );
            assert_eq!(resolve(&exec, "n", "foo"), "/bar");
        }

        #[test]
        fn argv_fallback_yields_only_argv_rules_for_the_named_node() {
            // What the Rust component sink reads as its fallback tier.
            let mut exec = executor();
            install_argv(
                &mut exec,
                &["--ros-args", "-r", "a:=/x", "-r", "other:b:=/y"],
            );
            exec.declare_remap("talker", "/", "a", "/launch").unwrap();
            let got: std::vec::Vec<_> = argv_fallback(&exec, "talker", "/").collect();
            assert_eq!(got, [("a", "/x")]);
        }

        #[test]
        fn install_is_all_or_nothing() {
            let mut exec = executor();
            let free = exec.remap_table.len();
            for i in 0..free - 1 {
                let from = std::format!("/f{i}");
                exec.declare_remap("n", "/", &from, "/t").unwrap();
            }
            let before = exec.remap_len;
            let rules = [
                RemapArg {
                    node: None,
                    from: "fits",
                    to: "/a",
                },
                RemapArg {
                    node: None,
                    from: "does_not",
                    to: "/b",
                },
            ];
            assert_eq!(install_argv_remaps(&mut exec, rules), Err(rules[1]));
            assert_eq!(
                exec.remap_len, before,
                "a refused install leaves no rule behind"
            );
            assert_eq!(resolve(&exec, "n", "fits"), "/fits");
        }
    }

    #[test]
    fn kind_qualified_remaps_are_refused() {
        let r = "rostopic://foo:=bar";
        assert_eq!(
            rules(&["--ros-args", "-r", r]),
            Err(RosArgsError::Unsupported { flag: r })
        );
    }
}
