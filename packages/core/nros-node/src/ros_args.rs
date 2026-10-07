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
//! * `-p` / `--param` is followed by ONE token, `[node:]name:=value`, and
//!   `--params-file` by a path. Both are parameter OVERRIDES (phase-482 W5,
//!   RFC-0015 §9.4's POSIX row): reported through [`parse_ros_args_events`],
//!   installed with `Executor::install_argv_params`, and applied when a node
//!   declares the parameter. The file is the caller's to read — this module
//!   has no file system — and [`parse_params_yaml`] reads its text.
//!
//! # What is REFUSED, and why each one
//!
//! Every flag this parser does not honour is a [`RosArgsError`], never a skip.
//! Silently dropping a ROS argument is the defect `nros::init_with_args` was
//! written to stop (a `-r chatter:=/other` that compiles and then publishes on
//! `chatter`). rclcpp is no more lenient: `rclcpp::init` throws
//! `UnknownROSArgsError` for any ROS argument rcl left unparsed.
//!
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

/// The `-p`/`--param` override one argument carried: `[node:]name:=value`,
/// borrowed from the argument it was parsed out of. Also what
/// [`parse_params_yaml`] reports for each value in a parameter file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParamArg<'a> {
    /// `Some(name)` applies only to the node of that NAME; `None` to every
    /// node (a bare `-p`, or a parameter file's `/**` section).
    pub node: Option<&'a str>,
    /// The parameter name, dotted for a nested one (`gains.kp`).
    pub name: &'a str,
    /// The value as written. Typed when a node declares the parameter: by the
    /// type of the default it declares, never guessed from the text alone.
    pub value: &'a str,
}

/// One thing a `--ros-args` scope said, in argv order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RosArg<'a> {
    /// `-r` / `--remap [node:]from:=to`.
    Remap(RemapArg<'a>),
    /// `-p` / `--param [node:]name:=value`.
    Param(ParamArg<'a>),
    /// `--params-file <path>`. The caller reads the file and hands its text to
    /// [`parse_params_yaml`].
    ParamsFile(&'a str),
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
    /// `-p`/`--param` or `--params-file` was the last argument.
    MissingValue {
        /// The flag as written.
        flag: &'a str,
    },
    /// `-p`/`--param` was followed by something that is not `[node:]name:=value`.
    InvalidParam {
        /// The rule as written.
        rule: &'a str,
    },
}

impl core::fmt::Display for RosArgsError<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Unsupported { flag } => write!(
                f,
                "`{flag}` is a ROS argument nano-ros does not honour here (enclaves and log \
                 configuration have no counterpart; `-p` / `--params-file` are honoured only \
                 where an executor can hold parameter overrides)"
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
            Self::MissingValue { flag } => {
                write!(f, "`{flag}` must be followed by a value")
            }
            Self::InvalidParam { rule } => write!(
                f,
                "`{rule}` is not a parameter override; expected `[node:]name:=value`"
            ),
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
const UNSUPPORTED_FLAGS: &[&str] = &["-e", "--enclave", "--log-level", "--log-config-file"];

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

/// Parse one `[node:]name:=value` override. An empty value is allowed (it is
/// an empty string parameter); an empty or blank name is not.
pub fn parse_param_rule(rule: &str) -> Result<ParamArg<'_>, RosArgsError<'_>> {
    let invalid = RosArgsError::InvalidParam { rule };
    let (lhs, value) = rule.split_once(":=").ok_or(invalid)?;
    let (node, name) = match lhs.split_once(':') {
        Some((node, name)) => (Some(node), name),
        None => (None, lhs),
    };
    let bad = |s: &str| s.is_empty() || s.contains(':') || s.chars().any(char::is_whitespace);
    if bad(name) {
        return Err(invalid);
    }
    if let Some(n) = node
        && (bad(n) || n.contains('/') || n.contains('~'))
    {
        return Err(invalid);
    }
    Ok(ParamArg { node, name, value })
}

/// Parse `args` the way `rcl_parse_arguments` does, reporting each remap rule
/// in argv order through `on_remap`.
///
/// Returns the first refusal, naming its token; `on_remap` may have been
/// called for rules before it, so a caller that must be all-or-nothing should
/// collect and only commit on `Ok`. Arguments outside every `--ros-args` scope
/// are ignored, so a vector with no `--ros-args` reports nothing and succeeds.
///
/// The remap-only parse: a `-p` / `--params-file` here is REFUSED as
/// unsupported, because a caller that only installs remaps would otherwise
/// drop it. A caller that can hold parameter overrides uses
/// [`parse_ros_args_events`].
pub fn parse_ros_args<'a, I, F>(args: I, mut on_remap: F) -> Result<(), RosArgsError<'a>>
where
    I: IntoIterator<Item = &'a str>,
    F: FnMut(RemapArg<'a>),
{
    parse_impl(args, false, |a| {
        if let RosArg::Remap(r) = a {
            on_remap(r);
        }
    })
}

/// Parse `args` like [`parse_ros_args`], reporting remaps AND parameter
/// overrides (`-p`, `--params-file`) in argv order — phase-482 W5.
pub fn parse_ros_args_events<'a, I, F>(args: I, on_arg: F) -> Result<(), RosArgsError<'a>>
where
    I: IntoIterator<Item = &'a str>,
    F: FnMut(RosArg<'a>),
{
    parse_impl(args, true, on_arg)
}

fn parse_impl<'a, I, F>(args: I, params: bool, mut on_arg: F) -> Result<(), RosArgsError<'a>>
where
    I: IntoIterator<Item = &'a str>,
    F: FnMut(RosArg<'a>),
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
                on_arg(RosArg::Remap(parse_remap_rule(rule)?));
            }
            "-p" | "--param" | "--params-file" if !params => {
                return Err(RosArgsError::Unsupported { flag: arg });
            }
            "-p" | "--param" => {
                let rule = it.next().ok_or(RosArgsError::MissingValue { flag: arg })?;
                on_arg(RosArg::Param(parse_param_rule(rule)?));
            }
            "--params-file" => {
                let path = it.next().ok_or(RosArgsError::MissingValue { flag: arg })?;
                on_arg(RosArg::ParamsFile(path));
            }
            flag if UNSUPPORTED_FLAGS.contains(&flag) || is_log_switch(flag) => {
                return Err(RosArgsError::Unsupported { flag });
            }
            token => return Err(RosArgsError::Unknown { token }),
        }
    }
    Ok(())
}

/// Why a parameter file was refused: the 1-based line, the line as written,
/// and the reason. Every construct the subset does not read is a refusal,
/// never a skip — a value silently dropped from a file is the same "compiles
/// and differs" the argv parse exists to stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParamsYamlError<'t> {
    /// 1-based line number.
    pub line: usize,
    /// The offending line, trimmed.
    pub text: &'t str,
    /// What is wrong with it.
    pub reason: &'static str,
}

impl core::fmt::Display for ParamsYamlError<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "line {}: {}: `{}`", self.line, self.reason, self.text)
    }
}

/// The longest dotted name [`parse_params_yaml`] builds (`group.sub.name`).
pub const MAX_PARAMS_FILE_NAME_LEN: usize = 128;
const MAX_PARAMS_FILE_DEPTH: usize = 8;

/// Read a ROS 2 parameter file's TEXT — the `--params-file` format — and
/// report each value through `on_param`, in file order.
///
/// The subset rcl's files use and nothing more:
///
/// ```yaml
/// /**:                     # every node; or `talker` / `/ns/talker`
///   ros__parameters:
///     rate_hz: 10
///     gains:
///       kp: 1.5            # reported as `gains.kp`
///     frame: "base_link"
/// ```
///
/// A node key matches by its last segment, the node NAME, as `-p node:` and
/// `-r node:` do; its namespace is not compared. Comments, blank lines and
/// `---` are skipped. Refused, each with its line: tabs used as indentation,
/// sequences and flow collections (array parameters are not read from a file
/// yet), block scalars, anchors, wildcards other than `/**`, and any value not
/// under `<node>: ros__parameters:`.
///
/// `no_std` and allocation-free: the dotted name is built in a fixed buffer
/// and lent to `on_param` for the call.
pub fn parse_params_yaml<'t, F>(text: &'t str, mut on_param: F) -> Result<(), ParamsYamlError<'t>>
where
    F: for<'n> FnMut(ParamArg<'n>),
{
    let mut stack: heapless::Vec<(usize, &'t str), MAX_PARAMS_FILE_DEPTH> = heapless::Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let err = |reason| ParamsYamlError {
            line: i + 1,
            text: raw.trim(),
            reason,
        };
        let content = yaml_strip_comment(raw);
        let body = content.trim();
        if body.is_empty() || body == "---" || body == "..." {
            continue;
        }
        let lead = &content[..content.len() - content.trim_start().len()];
        if lead.contains('\t') {
            return Err(err("a tab is not YAML indentation"));
        }
        let indent = lead.len();
        if body == "-" || body.starts_with("- ") {
            return Err(err("a sequence is not read from a parameter file"));
        }
        let (key, rest) =
            yaml_split_key(body).ok_or_else(|| err("expected `key:` or `key: value`"))?;
        let key = yaml_unquote(key.trim());
        while let Some(&(ind, _)) = stack.last() {
            if ind >= indent {
                stack.pop();
            } else {
                break;
            }
        }
        let rest = rest.trim();
        if rest.is_empty() {
            stack
                .push((indent, key))
                .map_err(|_| err("nested deeper than a parameter file needs"))?;
            continue;
        }
        if stack.len() < 2 {
            return Err(err(
                "a value must sit under `<node>:` then `ros__parameters:`",
            ));
        }
        if stack[1].1 != "ros__parameters" {
            return Err(err("the level under a node key must be `ros__parameters`"));
        }
        if rest.starts_with(['[', '{', '|', '>', '&', '*', '!']) {
            return Err(err(
                "an array, inline map, block scalar, anchor or tag is not read from a parameter file",
            ));
        }
        let node = yaml_node_selector(stack[0].1)
            .ok_or_else(|| err("only the `/**` wildcard is supported"))?;
        let mut name: heapless::String<MAX_PARAMS_FILE_NAME_LEN> = heapless::String::new();
        for &(_, group) in &stack[2..] {
            name.push_str(group)
                .and_then(|_| name.push('.'))
                .map_err(|_| err("parameter name longer than MAX_PARAMS_FILE_NAME_LEN"))?;
        }
        name.push_str(key)
            .map_err(|_| err("parameter name longer than MAX_PARAMS_FILE_NAME_LEN"))?;
        on_param(ParamArg {
            node,
            name: &name,
            value: yaml_unquote(rest),
        });
    }
    Ok(())
}

/// The line up to a `#` comment that is not inside quotes.
fn yaml_strip_comment(line: &str) -> &str {
    let mut quote = None;
    let mut prev_space = true;
    for (i, c) in line.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '#') if prev_space => return &line[..i],
            _ => {}
        }
        prev_space = c.is_whitespace();
    }
    line
}

/// `key: rest` / `key:` — the first `:` outside quotes followed by a space or
/// the end of the line.
fn yaml_split_key(body: &str) -> Option<(&str, &str)> {
    let mut quote = None;
    let bytes = body.as_bytes();
    for (i, c) in body.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, ':') if i + 1 == bytes.len() || bytes[i + 1] == b' ' => {
                return Some((&body[..i], &body[i + 1..]));
            }
            _ => {}
        }
    }
    None
}

fn yaml_unquote(s: &str) -> &str {
    let b = s.as_bytes();
    if b.len() >= 2 && (b[0] == b'"' || b[0] == b'\'') && b[b.len() - 1] == b[0] {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

/// `None` for every node (`/**`, `**`), `Some(name)` for a node key, or a
/// refusal (`Err` as `None` of the outer option) for another wildcard.
#[allow(clippy::option_option)]
fn yaml_node_selector(key: &str) -> Option<Option<&str>> {
    match key {
        "/**" | "**" => Some(None),
        k if k.contains('*') => None,
        k => Some(Some(k.rsplit('/').next().unwrap_or(k))),
    }
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
/// Why [`apply_ros_args`] refused an argument vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyError<'a> {
    /// The parse refused a token ([`parse_ros_args`]).
    Refused(RosArgsError<'a>),
    /// The rule did not fit: the remap table is full (its slots are shared
    /// with launch rules), or a name is longer than its slot.
    DoesNotFit(RemapArg<'a>),
}

#[cfg(any(has_rmw, test))]
impl core::fmt::Display for ApplyError<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Refused(e) => e.fmt(f),
            Self::DoesNotFit(r) => write!(
                f,
                "remap `{}:={}` does not fit: the executor's remap table holds {} rules, \
                 shared with the launch file's; `from`/`to` hold {} bytes, a `node:` \
                 prefix 64",
                r.from,
                r.to,
                crate::executor::spin::MAX_REMAPS,
                crate::names::MAX_RESOLVED_NAME_LEN,
            ),
        }
    }
}

#[cfg(any(has_rmw, test))]
/// Parse `args` and, given an executor, install its `-r` rules as the
/// FALLBACK tier — the whole of "honour `--ros-args`" for a caller with no
/// allocator (the C++ ABI, `nros_cpp_install_argv_remaps`).
///
/// With `executor = None` it only VALIDATES, so a caller can refuse a bad
/// vector before opening a session at all. All or nothing either way: on any
/// error nothing is installed. Returns the number of rules installed (or that
/// would be).
pub fn apply_ros_args<'a, I>(
    executor: Option<&mut crate::executor::Executor<'_>>,
    args: I,
) -> Result<usize, ApplyError<'a>>
where
    I: IntoIterator<Item = &'a str>,
{
    const N: usize = crate::executor::spin::MAX_REMAPS;
    let mut rules: [Option<RemapArg<'a>>; N] = [None; N];
    let mut n = 0usize;
    let mut overflow = None;
    parse_ros_args(args, |r| {
        if n < N {
            rules[n] = Some(r);
            n += 1;
        } else if overflow.is_none() {
            overflow = Some(r);
        }
    })
    .map_err(ApplyError::Refused)?;
    if let Some(r) = overflow {
        return Err(ApplyError::DoesNotFit(r));
    }
    if let Some(executor) = executor {
        install_argv_remaps(executor, rules[..n].iter().flatten().copied())
            .map_err(ApplyError::DoesNotFit)?;
    }
    Ok(n)
}

#[cfg(all(any(has_rmw, test), feature = "param-services"))]
/// Why [`apply_ros_args_with_params`] refused an argument vector.
#[derive(Debug, Clone)]
pub enum ApplyParamsError<'a> {
    /// The remap half refused it, exactly as [`apply_ros_args`] would.
    Remaps(ApplyError<'a>),
    /// `--params-file <path>` could not be read; the reason is the reader's.
    Unreadable {
        /// The path as written.
        path: &'a str,
        /// What the reader said.
        why: alloc::string::String,
    },
    /// The file was read and its text refused by [`parse_params_yaml`].
    BadFile {
        /// The path as written.
        path: &'a str,
        /// `line N: reason: `text``.
        why: alloc::string::String,
    },
    /// The parameter store could not be allocated (issue 1706).
    NoStore,
}

#[cfg(all(any(has_rmw, test), feature = "param-services"))]
impl core::fmt::Display for ApplyParamsError<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Remaps(e) => e.fmt(f),
            Self::Unreadable { path, why } => {
                write!(f, "`--params-file {path}` cannot be read: {why}")
            }
            Self::BadFile { path, why } => write!(f, "`--params-file {path}`, {why}"),
            Self::NoStore => f.write_str(
                "the parameter store could not be allocated, so the parameter overrides have \
                 nowhere to go",
            ),
        }
    }
}

#[cfg(all(any(has_rmw, test), feature = "param-services"))]
/// [`apply_ros_args`], plus the parameter half — phase-482 W5.
///
/// `-r` rules are installed as [`apply_ros_args`] installs them. `-p` overrides
/// and every `--params-file` (read through `read_file`, which is the caller's:
/// this crate has no file system) are installed with
/// `Executor::install_argv_params`, in argv order, so a later value wins.
///
/// With `executor = None` it only VALIDATES — files are still read and parsed,
/// so a bad file is refused before a session opens. All or nothing: on any
/// error nothing is installed.
pub fn apply_ros_args_with_params<'a, I, R>(
    executor: Option<&mut crate::executor::Executor<'_>>,
    args: I,
    mut read_file: R,
) -> Result<usize, ApplyParamsError<'a>>
where
    I: IntoIterator<Item = &'a str> + Clone,
    R: FnMut(&str) -> Result<alloc::string::String, alloc::string::String>,
{
    use alloc::string::String;
    // Pass 1: the remaps and the refusals, through the remap-only grammar's
    // twin so a refused token reports exactly as before.
    let mut params: alloc::vec::Vec<(Option<String>, String, String)> = alloc::vec::Vec::new();
    let mut files: alloc::vec::Vec<&'a str> = alloc::vec::Vec::new();
    let mut order: alloc::vec::Vec<Result<usize, usize>> = alloc::vec::Vec::new();
    parse_ros_args_events(args.clone(), |a| match a {
        RosArg::Param(p) => {
            order.push(Ok(params.len()));
            params.push((
                p.node.map(String::from),
                String::from(p.name),
                String::from(p.value),
            ));
        }
        RosArg::ParamsFile(path) => {
            order.push(Err(files.len()));
            files.push(path);
        }
        RosArg::Remap(_) => {}
    })
    .map_err(|e| ApplyParamsError::Remaps(ApplyError::Refused(e)))?;
    // Read and parse every file before installing anything.
    let mut file_params: alloc::vec::Vec<alloc::vec::Vec<(Option<String>, String, String)>> =
        alloc::vec::Vec::new();
    for &path in &files {
        let text = read_file(path).map_err(|why| ApplyParamsError::Unreadable { path, why })?;
        let mut got = alloc::vec::Vec::new();
        parse_params_yaml(&text, |p| {
            got.push((
                p.node.map(String::from),
                String::from(p.name),
                String::from(p.value),
            ))
        })
        .map_err(|e| ApplyParamsError::BadFile {
            path,
            why: alloc::format!("{e}"),
        })?;
        file_params.push(got);
    }
    // The remap half, minus the parameter flags it would refuse.
    let remap_args = args.into_iter().scan(false, |skip, a| {
        if core::mem::take(skip) {
            return Some(None);
        }
        if matches!(a, "-p" | "--param" | "--params-file") {
            *skip = true;
            return Some(None);
        }
        Some(Some(a))
    });
    let remap_args: alloc::vec::Vec<&'a str> = remap_args.flatten().collect();
    let Some(executor) = executor else {
        let n =
            apply_ros_args(None, remap_args.iter().copied()).map_err(ApplyParamsError::Remaps)?;
        return Ok(n + params.len() + file_params.iter().map(alloc::vec::Vec::len).sum::<usize>());
    };
    // Commit in the order that keeps it all-or-nothing: make sure the store
    // exists (the one way the parameter half can fail), then the remaps (which
    // are all-or-nothing on their own), then the parameters, which can no
    // longer fail.
    executor
        .install_argv_params(core::iter::empty())
        .map_err(|_| ApplyParamsError::NoStore)?;
    let n = apply_ros_args(Some(&mut *executor), remap_args.iter().copied())
        .map_err(ApplyParamsError::Remaps)?;
    let mut flat: alloc::vec::Vec<&(Option<String>, String, String)> = alloc::vec::Vec::new();
    for slot in &order {
        match *slot {
            Ok(i) => flat.push(&params[i]),
            Err(f) => flat.extend(file_params[f].iter()),
        }
    }
    let m = executor
        .install_argv_params(flat.iter().map(|(node, name, value)| ParamArg {
            node: node.as_deref(),
            name,
            value,
        }))
        .map_err(|_| ApplyParamsError::NoStore)?;
    Ok(n + m)
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

    #[test]
    fn apply_validates_without_an_executor_and_installs_nothing() {
        let ok = [
            "prog",
            "--ros-args",
            "-r",
            "a:=b",
            "-r",
            "n:c:=d",
            "--",
            "x",
        ];
        assert_eq!(apply_ros_args(None, ok.iter().copied()), Ok(2));
        assert_eq!(
            apply_ros_args(None, ["prog", "plain"].iter().copied()),
            Ok(0)
        );
    }

    #[test]
    fn apply_refuses_by_name_what_the_parse_refuses() {
        let bad = ["prog", "--ros-args", "-r", "a:=b", "-p", "x:=1"];
        assert_eq!(
            apply_ros_args(None, bad.iter().copied()),
            Err(ApplyError::Refused(RosArgsError::Unsupported {
                flag: "-p"
            }))
        );
    }

    #[test]
    fn apply_refuses_more_rules_than_the_table_can_ever_hold() {
        const N: usize = crate::executor::spin::MAX_REMAPS;
        let mut args: heapless::Vec<&str, { 2 + 2 * (N + 1) }> = heapless::Vec::new();
        args.extend_from_slice(&["prog", "--ros-args"]).unwrap();
        for _ in 0..=N {
            args.extend_from_slice(&["-r", "a:=b"]).unwrap();
        }
        assert!(matches!(
            apply_ros_args(None, args.iter().copied()),
            Err(ApplyError::DoesNotFit(_))
        ));
    }
}

// `alloc` for the YAML helper's owned rows; the lanes that build nros-node
// without it (the declared-QoS registration check) still compile its tests.
#[cfg(all(test, feature = "alloc"))]
mod param_tests {
    use super::*;

    fn events<'a>(args: &[&'a str]) -> Result<heapless::Vec<RosArg<'a>, 8>, RosArgsError<'a>> {
        let mut out = heapless::Vec::new();
        parse_ros_args_events(args.iter().copied(), |a| out.push(a).unwrap())?;
        Ok(out)
    }

    #[test]
    fn p_and_params_file_are_events_in_argv_order() {
        let got = events(&[
            "prog",
            "--ros-args",
            "-p",
            "a:=1",
            "--params-file",
            "x.yaml",
            "-r",
            "f:=t",
            "--param",
            "n:b:=two",
        ])
        .unwrap();
        assert_eq!(got.len(), 4);
        assert_eq!(
            got[0],
            RosArg::Param(ParamArg {
                node: None,
                name: "a",
                value: "1"
            })
        );
        assert_eq!(got[1], RosArg::ParamsFile("x.yaml"));
        assert!(matches!(got[2], RosArg::Remap(_)));
        assert_eq!(
            got[3],
            RosArg::Param(ParamArg {
                node: Some("n"),
                name: "b",
                value: "two"
            })
        );
    }

    #[test]
    fn the_remap_only_parse_still_refuses_parameter_flags() {
        let r = parse_ros_args(["prog", "--ros-args", "-p", "a:=1"], |_| {});
        assert_eq!(r, Err(RosArgsError::Unsupported { flag: "-p" }));
        let r = parse_ros_args(["prog", "--ros-args", "--params-file", "x"], |_| {});
        assert_eq!(
            r,
            Err(RosArgsError::Unsupported {
                flag: "--params-file"
            })
        );
    }

    #[test]
    fn malformed_overrides_are_refused_by_name() {
        assert_eq!(
            events(&["p", "--ros-args", "-p"]).unwrap_err(),
            RosArgsError::MissingValue { flag: "-p" }
        );
        assert_eq!(
            events(&["p", "--ros-args", "-p", "novalue"]).unwrap_err(),
            RosArgsError::InvalidParam { rule: "novalue" }
        );
        assert_eq!(
            events(&["p", "--ros-args", "--params-file"]).unwrap_err(),
            RosArgsError::MissingValue {
                flag: "--params-file"
            }
        );
        // An empty VALUE is an empty string, not a malformed rule.
        assert_eq!(
            events(&["p", "--ros-args", "-p", "label:="]).unwrap()[0],
            RosArg::Param(ParamArg {
                node: None,
                name: "label",
                value: ""
            })
        );
    }

    fn yaml(
        text: &str,
    ) -> Result<
        alloc::vec::Vec<(
            Option<alloc::string::String>,
            alloc::string::String,
            alloc::string::String,
        )>,
        alloc::string::String,
    > {
        let mut out = alloc::vec::Vec::new();
        parse_params_yaml(text, |p| {
            out.push((
                p.node.map(alloc::string::String::from),
                alloc::string::String::from(p.name),
                alloc::string::String::from(p.value),
            ))
        })
        .map_err(|e| alloc::format!("{e}"))?;
        Ok(out)
    }

    #[test]
    fn a_params_file_reports_dotted_names_per_node() {
        let got = yaml("/**:\n  ros__parameters:\n    a: 1  # c\n    g:\n      b: 'x # not a comment'\n/ns/talker:\n  ros__parameters:\n    c: true\n").unwrap();
        assert_eq!(got.len(), 3);
        assert_eq!(got[0], (None, "a".into(), "1".into()));
        assert_eq!(got[1], (None, "g.b".into(), "x # not a comment".into()));
        assert_eq!(got[2], (Some("talker".into()), "c".into(), "true".into()));
    }

    #[test]
    fn what_the_subset_does_not_read_is_refused_with_its_line() {
        let e = yaml("/**:\n  ros__parameters:\n    xs: [1, 2]\n").unwrap_err();
        assert!(e.starts_with("line 3:"), "{e}");
        let e = yaml("/**:\n  params:\n    a: 1\n").unwrap_err();
        assert!(e.contains("ros__parameters"), "{e}");
        let e = yaml("/*/x:\n  ros__parameters:\n    a: 1\n").unwrap_err();
        assert!(e.contains("wildcard"), "{e}");
        let e = yaml("/**:\n\tros__parameters:\n").unwrap_err();
        assert!(e.contains("tab"), "{e}");
        let e = yaml("/**:\n  ros__parameters:\n    - a\n").unwrap_err();
        assert!(e.contains("sequence"), "{e}");
    }
}
