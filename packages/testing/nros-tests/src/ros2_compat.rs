//! Ask a nano-ros node and a stock ROS 2 node the same `ros2` CLI question,
//! and compare the two answers (phase-556 W1).
//!
//! The ROS 2 interface compatibility audit (phase-556) measures what a stock
//! user sees: the same `ros2 param` / `service` / `action` / `topic` query,
//! asked of a nano-ros node and of a stock `rclcpp` / `rclpy` node that
//! declare the same interface. This module is the one place that knows how to
//! do that, so every axis is a few lines in `tests/ros2_cli_parity_interop.rs`
//! rather than its own scaffold.
//!
//! # Shape
//!
//! Each side runs on its OWN `rmw_zenohd` router ([`GraphSide`]). A side's
//! graph therefore holds exactly one implementation, so a graph-wide query
//! (`service list -t`, `action info`, `topic info -v`) answers about that
//! side alone, and the two sides may use the same service, action and topic
//! names, as the stock examples and ours do. A [`Pair`] holds both sides and
//! asks each the same question ([`Pair::ask`]).
//!
//! # Normalisation
//!
//! Only differences that are by design are normalised away, and only in one
//! way: every occurrence of a side's own node name (fully qualified, or bare as
//! `topic info -v` prints it) becomes [`NODE_PLACEHOLDER`], and the log lines
//! the CLI's own middleware session writes to stderr are dropped
//! ([`is_middleware_log_line`]; they describe the CLI process, not the node,
//! and turn up on either side at random). Nothing is sorted:
//! the `ros2` CLI already sorts every listing this audit asks for, so an
//! ordering difference is a real difference. A caller that has to set a known,
//! separately tested difference aside does so explicitly with
//! [`without_lines_containing`] or [`only_lines_containing`], at the call site,
//! where a reader can see it.
//!
//! # Daemon
//!
//! Every query goes through [`crate::ros2::ros2_query_cmd`] (`--no-daemon`),
//! except the three verbs that cannot take it:
//!
//! * `ros2 service call` creates its own node and never consults the daemon,
//!   and rejects the flag;
//! * `ros2 action list` and `ros2 action info` consult the daemon and reject
//!   the flag (issues 1333, 1342 and 1691 are why the daemon must not be
//!   consulted). They are run through ros2cli's own entry point with its
//!   `NodeStrategy` told that no daemon is running and none may be spawned
//!   ([`DAEMONLESS_ROS2`]), which is exactly what `--no-daemon` does for the
//!   verbs that accept it: the verb's own code, against a direct node.

use crate::{
    TestError, TestResult,
    fixtures::{ZenohRouter, or_unmet},
    process::ManagedProcess,
    ros2::{DEFAULT_ROS_DISTRO, ros2_env_setup_zenoh, ros2_query_cmd},
};
use std::{
    fmt,
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

/// What a side's own node name becomes in a normalised answer.
pub const NODE_PLACEHOLDER: &str = "<node>";

/// The six parameter services every stock node serves, by suffix after the
/// node's fully qualified name, with their types, in the order `ros2 service
/// list -t` prints them.
pub const PARAMETER_SERVICES: [(&str, &str); 6] = [
    (
        "describe_parameters",
        "rcl_interfaces/srv/DescribeParameters",
    ),
    (
        "get_parameter_types",
        "rcl_interfaces/srv/GetParameterTypes",
    ),
    ("get_parameters", "rcl_interfaces/srv/GetParameters"),
    ("list_parameters", "rcl_interfaces/srv/ListParameters"),
    ("set_parameters", "rcl_interfaces/srv/SetParameters"),
    (
        "set_parameters_atomically",
        "rcl_interfaces/srv/SetParametersAtomically",
    ),
];

/// Runs one `ros2` verb with ros2cli's daemon lookup and daemon spawn turned
/// off, for the verbs that do not accept `--no-daemon` (`action list`,
/// `action info`). Arguments follow the script on the command line.
///
/// `NodeStrategy.__init__` reads both names from its own module's globals at
/// call time, so replacing them there is all `--no-daemon` would have done:
/// the verb then runs its own code against a `DirectNode`.
pub const DAEMONLESS_ROS2: &str = r#"
import sys
import ros2cli.node.strategy as strategy
strategy.is_daemon_running = lambda *a, **k: False
strategy.spawn_daemon = lambda *a, **k: False
from ros2cli.cli import main
sys.exit(main(argv=sys.argv[1:]))
"#;

/// Which implementation a side runs.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Implementation {
    NanoRos,
    Stock,
}

impl fmt::Display for Implementation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Implementation::NanoRos => "nano-ros",
            Implementation::Stock => "stock ROS 2",
        })
    }
}

/// One `ros2` command's answer: what it printed (stdout and stderr together,
/// in the order the CLI wrote them) and its exit status.
#[derive(Clone, Debug)]
pub struct CliAnswer {
    /// The `ros2` arguments as asked, after `{node}` substitution.
    pub args: String,
    pub output: String,
    /// `None` when the process was killed by a signal.
    pub exit_code: Option<i32>,
}

impl fmt::Display for CliAnswer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let code = self
            .exit_code
            .map_or_else(|| "killed".to_string(), |c| c.to_string());
        write!(
            f,
            "$ ros2 {}\n{}[exit {code}]",
            self.args,
            ensure_trailing_newline(&self.output)
        )
    }
}

fn ensure_trailing_newline(s: &str) -> String {
    if s.is_empty() || s.ends_with('\n') {
        s.to_string()
    } else {
        format!("{s}\n")
    }
}

/// How a query reaches the `ros2` CLI. See the module docs, "Daemon".
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Route {
    NoDaemonFlag,
    OwnNode,
    Daemonless,
}

fn route_for(args: &str) -> Route {
    let mut words = args.split_whitespace();
    match (words.next(), words.next()) {
        (Some("service"), Some("call")) => Route::OwnNode,
        (Some("action"), Some("list" | "info")) => Route::Daemonless,
        _ => Route::NoDaemonFlag,
    }
}

/// One side of a pair: its own router, the `ros2` environment that reaches
/// it, the processes that make up the side, and the node a `{node}` in a
/// query stands for.
pub struct GraphSide {
    pub implementation: Implementation,
    /// The node `{node}` expands to, fully qualified (`/alpha`).
    pub node: String,
    env_setup: String,
    _router: ZenohRouter,
    _config: tempfile::TempDir,
    procs: Vec<ManagedProcess>,
    logs: Vec<PathBuf>,
    /// Holds the stock peers' log files.
    log_dir: tempfile::TempDir,
}

impl GraphSide {
    /// A side with its own router and nothing running on it yet.
    pub fn new(implementation: Implementation, node: &str) -> Self {
        let router = or_unmet(ZenohRouter::start_unique());
        let (env_setup, config) = ros2_env_setup_zenoh(DEFAULT_ROS_DISTRO, &router.locator(), 0);
        GraphSide {
            implementation,
            node: node.to_string(),
            env_setup,
            _router: router,
            _config: config,
            procs: Vec::new(),
            logs: Vec::new(),
            log_dir: tempfile::tempdir().expect("create a log dir for the stock peers"),
        }
    }

    /// The router locator a nano-ros image on this side dials.
    pub fn locator(&self) -> String {
        self._router.locator()
    }

    /// Start a built nano-ros image on this side's router.
    pub fn spawn_nano_image(&mut self, binary: &std::path::Path, name: &str) {
        let mut cmd = Command::new(binary);
        cmd.env("RUST_LOG", "info")
            .env("NROS_LOCATOR", self.locator())
            .env("NROS_SESSION_MODE", "client");
        let proc = ManagedProcess::spawn_image(cmd, name)
            .unwrap_or_else(|e| panic!("could not start the nano-ros image {name}: {e}"));
        self.procs.push(proc);
    }

    /// Start a stock ROS 2 process on this side's router. `command` is a
    /// shell command run after the side's ROS 2 environment is sourced, such
    /// as `ros2 run demo_nodes_cpp talker`. Its output goes to a log file
    /// that [`Self::describe`] prints.
    pub fn spawn_stock(&mut self, command: &str, name: &str) {
        let log = self.log_dir.path().join(format!("{}.log", self.logs.len()));
        let deadline = crate::process::deadline(300);
        let shell = format!(
            "{} && exec {deadline} {command} > {} 2>&1",
            self.env_setup,
            log.display()
        );
        let mut cmd = Command::new("bash");
        cmd.args(["-c", &shell]);
        let proc = ManagedProcess::spawn_command(cmd, name)
            .unwrap_or_else(|e| panic!("could not start the stock node {name}: {e}"));
        self.procs.push(proc);
        self.logs.push(log);
    }

    /// Run one `ros2` query on this side. `{node}` in `args` becomes
    /// [`Self::node`].
    pub fn query(&self, args: &str) -> TestResult<CliAnswer> {
        let args = args.replace("{node}", &self.node);
        let shell = match route_for(&args) {
            Route::NoDaemonFlag => ros2_query_cmd(&self.env_setup, 20, &args),
            Route::OwnNode => {
                let deadline = crate::process::deadline(20);
                format!("{} && {deadline} ros2 {args} 2>&1", self.env_setup)
            }
            Route::Daemonless => {
                let deadline = crate::process::deadline(20);
                format!(
                    "{} && {deadline} python3 -c \"$NROS_DAEMONLESS_ROS2\" {args} 2>&1",
                    self.env_setup
                )
            }
        };
        // A CLI process whose own session failed to close reports that in
        // its exit status, so its answer is asked again (a few times at most).
        let mut attempt = 0;
        loop {
            attempt += 1;
            let out = Command::new("bash")
                .args(["-c", &shell])
                .env("NROS_DAEMONLESS_ROS2", DAEMONLESS_ROS2)
                .output()
                .map_err(|e| {
                    TestError::ProcessFailed(format!("could not run `ros2 {args}`: {e}"))
                })?;
            let answer = CliAnswer {
                args: args.clone(),
                output: String::from_utf8_lossy(&out.stdout).into_owned(),
                exit_code: out.status.code(),
            };
            if attempt >= 3 || !is_cli_teardown_failure(&answer.output) {
                return Ok(answer);
            }
        }
    }

    /// [`Self::query`], repeated until `ready` accepts the answer or
    /// `timeout` passes, returning the last answer either way. Discovery
    /// through a router takes a moment, and a CLI that has not discovered a
    /// node answers with an empty listing and exit 0, so readiness has to be
    /// judged on the content.
    pub fn query_until(
        &self,
        args: &str,
        timeout: Duration,
        ready: impl Fn(&CliAnswer) -> bool,
    ) -> CliAnswer {
        let start = Instant::now();
        loop {
            let answer = self
                .query(args)
                .unwrap_or_else(|e| panic!("{} side: {e}", self.implementation));
            if ready(&answer) || start.elapsed() >= timeout {
                return answer;
            }
            std::thread::sleep(Duration::from_millis(700));
        }
    }

    /// Wait until `ros2 node list` shows this side's node, or panic with
    /// what was seen. A side whose node never appears is a failure of that
    /// side to reach the ROS graph, not a missing prerequisite.
    pub fn await_node(&mut self) {
        let node = self.node.clone();
        let answer = self.query_until("node list", Duration::from_secs(30), |a| {
            a.output.lines().any(|l| l.trim() == node)
        });
        if !answer.output.lines().any(|l| l.trim() == node) {
            let implementation = self.implementation;
            let described = self.describe();
            panic!(
                "the {implementation} node {node} never appeared in `ros2 node list`.\n\
                 {answer}\n{described}"
            );
        }
    }

    /// The processes on this side, and the tail of each stock peer's log, for
    /// a failure message.
    pub fn describe(&mut self) -> String {
        let mut s = format!("{} side, node {}:\n", self.implementation, self.node);
        for p in &mut self.procs {
            let state = if p.is_running() { "running" } else { "exited" };
            s.push_str(&format!("  [{state}] {}\n", p.command_line()));
        }
        for log in &self.logs {
            let text = std::fs::read_to_string(log).unwrap_or_default();
            let tail: Vec<&str> = text.lines().rev().take(15).collect();
            s.push_str(&format!("  log {}:\n", log.display()));
            for l in tail.into_iter().rev() {
                s.push_str(&format!("    {l}\n"));
            }
        }
        s
    }
}

impl Drop for GraphSide {
    fn drop(&mut self) {
        for p in &mut self.procs {
            p.kill();
        }
    }
}

/// A nano-ros side and a stock side, asked the same questions.
pub struct Pair {
    pub nano: GraphSide,
    pub stock: GraphSide,
}

/// Both answers to one query, normalised, and the difference between them.
#[derive(Clone, Debug)]
pub struct Compared {
    /// The query as asked, with `{node}` standing for each side's node.
    pub query: String,
    pub nano: CliAnswer,
    pub stock: CliAnswer,
    /// `nano.output`, normalised.
    pub nano_text: String,
    /// `stock.output`, normalised.
    pub stock_text: String,
}

impl Compared {
    /// The line difference of the normalised outputs and the exit statuses,
    /// or `None` when both agree. Lines only nano-ros printed start with `-`,
    /// lines only stock printed with `+`; the exit status is the last line of
    /// each side, as `[exit N]`.
    pub fn diff(&self) -> Option<String> {
        line_diff(
            &with_exit(&self.nano_text, self.nano.exit_code),
            &with_exit(&self.stock_text, self.stock.exit_code),
        )
    }

    /// Restrict both normalised texts to the lines `keep` accepts. The exit
    /// statuses are still compared.
    pub fn filtered(mut self, keep: impl Fn(&str) -> bool) -> Self {
        self.nano_text = keep_lines(&self.nano_text, &keep);
        self.stock_text = keep_lines(&self.stock_text, &keep);
        self
    }

    /// Rewrite every line of both normalised texts with `f`.
    pub fn mapped(mut self, f: impl Fn(&str) -> String) -> Self {
        let map = |t: &str| t.lines().map(&f).collect::<Vec<_>>().join("\n");
        self.nano_text = map(&self.nano_text);
        self.stock_text = map(&self.stock_text);
        self
    }

    /// Both raw answers, for a failure message.
    pub fn report(&self) -> String {
        format!(
            "nano-ros answered:\n{}\n\nstock ROS 2 answered:\n{}",
            self.nano, self.stock
        )
    }

    /// Panic unless the two normalised answers are equal.
    pub fn assert_same(&self, what: &str) {
        if let Some(diff) = self.diff() {
            panic!(
                "{what}: nano-ros and stock ROS 2 answer `ros2 {}` differently \
                 (- nano-ros only, + stock only):\n{diff}\n\n{}",
                self.query,
                self.report()
            );
        }
    }
}

impl Pair {
    pub fn new(nano: GraphSide, stock: GraphSide) -> Self {
        Pair { nano, stock }
    }

    /// Ask both sides `args` (with `{node}` standing for each side's node),
    /// each until `ready` accepts its answer, and normalise both.
    pub fn ask_until(
        &self,
        args: &str,
        timeout: Duration,
        ready: impl Fn(&CliAnswer) -> bool,
    ) -> Compared {
        self.ask_each(args, timeout, &ready, &ready)
    }

    /// [`Self::ask_until`] with a readiness test per side, for a graph-wide
    /// query whose answer is ready when it shows that side's own node.
    pub fn ask_each(
        &self,
        args: &str,
        timeout: Duration,
        nano_ready: impl Fn(&CliAnswer) -> bool,
        stock_ready: impl Fn(&CliAnswer) -> bool,
    ) -> Compared {
        let nano = self.nano.query_until(args, timeout, nano_ready);
        let stock = self.stock.query_until(args, timeout, stock_ready);
        compare(args, nano, &self.nano.node, stock, &self.stock.node)
    }

    /// [`Self::ask_until`] with the answer accepted as soon as it is
    /// non-empty.
    pub fn ask(&self, args: &str) -> Compared {
        self.ask_until(args, Duration::from_secs(15), |a| {
            !a.output.trim().is_empty()
        })
    }
}

/// Normalise both answers against their own node names. The pure half of
/// [`Pair::ask`], separate so its behaviour can be tested without a graph.
pub fn compare(
    query: &str,
    nano: CliAnswer,
    nano_node: &str,
    stock: CliAnswer,
    stock_node: &str,
) -> Compared {
    let nano_text = normalise(&nano.output, nano_node);
    let stock_text = normalise(&stock.output, stock_node);
    Compared {
        query: query.to_string(),
        nano,
        stock,
        nano_text,
        stock_text,
    }
}

/// Replace `node` (fully qualified, `/alpha`) and its bare name (`alpha`,
/// as `topic info -v` prints it in `Node name:`) with [`NODE_PLACEHOLDER`]
/// wherever it stands as a whole name, trim trailing whitespace from each line
/// and drop trailing blank lines. `/alpha_chatter` and `alpha2` are left
/// alone: a name is whole when the characters on either side cannot continue
/// a ROS name.
pub fn normalise(text: &str, node: &str) -> String {
    let fq = if node.starts_with('/') {
        node.to_string()
    } else {
        format!("/{node}")
    };
    let bare = fq.rsplit('/').next().unwrap_or_default().to_string();
    let plain = strip_ansi(text);
    let mut lines: Vec<String> = plain
        .lines()
        .filter(|l| !is_middleware_log_line(l))
        .map(|l| {
            let l = replace_whole_name(l, &fq);
            let l = if bare.is_empty() {
                l
            } else {
                replace_whole_name(&l, &bare)
            };
            l.trim_end().to_string()
        })
        .collect();
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

/// `text` without ANSI escape sequences (ESC, then `[`, parameters and a final letter). The CLI's
/// middleware writes its log lines in colour even into a pipe.
pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// True when the `ros2` CLI process itself failed to tear down its middleware
/// session (`Failed to close the session`): the answer may be complete, but
/// the exit status then reports the CLI's teardown, not the node.
pub fn is_cli_teardown_failure(output: &str) -> bool {
    output.contains("Failed to close the session") || output.contains("Error closing session")
}

/// A log line the `ros2` CLI's OWN middleware session wrote to stderr: a
/// zenoh tracing line (`2026-11-12T20:01:47.655822Z  WARN rx-1 ...`) or an
/// rcutils line from the RMW (`[WARN] [1794512829.94] [rmw_zenoh_cpp]: ...`).
/// It reports on the CLI process, not on the node being asked, and appears on
/// either side at random, so it is not part of the answer.
pub fn is_middleware_log_line(line: &str) -> bool {
    let l = line.trim_start();
    let tracing = l.len() > 28
        && l.as_bytes()[4] == b'-'
        && l.as_bytes()[10] == b'T'
        && l[..4].bytes().all(|b| b.is_ascii_digit())
        && [
            "  TRACE ", "  DEBUG ", "  INFO ", "  WARN ", "  ERROR ", " TRACE ", " DEBUG ",
            " INFO ", " WARN ", " ERROR ",
        ]
        .iter()
        .any(|lvl| l.contains(lvl))
        && l.contains(" zenoh");
    let rcutils = [
        "[DEBUG] [",
        "[INFO] [",
        "[WARN] [",
        "[ERROR] [",
        "[FATAL] [",
    ]
    .iter()
    .any(|p| l.starts_with(p))
        && l.contains("] [rmw_zenoh_cpp]:");
    let teardown = l.starts_with("Failed to close the session");
    tracing || rcutils || teardown
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn replace_whole_name(line: &str, name: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(i) = rest.find(name) {
        let before = rest[..i]
            .chars()
            .next_back()
            .or_else(|| out.chars().next_back());
        let after = rest[i + name.len()..].chars().next();
        // A fully qualified name starts with `/`, which already bounds it on
        // the left; a bare one must not follow a name character or a `/`.
        let left_ok = if name.starts_with('/') {
            !before.is_some_and(is_name_char)
        } else {
            !before.is_some_and(|c| is_name_char(c) || c == '/')
        };
        let right_ok = !after.is_some_and(is_name_char);
        out.push_str(&rest[..i]);
        if left_ok && right_ok {
            out.push_str(NODE_PLACEHOLDER);
        } else {
            out.push_str(name);
        }
        rest = &rest[i + name.len()..];
    }
    out.push_str(rest);
    out
}

fn with_exit(text: &str, code: Option<i32>) -> String {
    let code = code.map_or_else(|| "killed".to_string(), |c| c.to_string());
    if text.is_empty() {
        format!("[exit {code}]")
    } else {
        format!("{text}\n[exit {code}]")
    }
}

fn keep_lines(text: &str, keep: &impl Fn(&str) -> bool) -> String {
    text.lines()
        .filter(|l| keep(l))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A line filter for [`Compared::filtered`]: drop lines containing any of
/// `needles`.
pub fn without_lines_containing<'a>(needles: &'a [&'a str]) -> impl Fn(&str) -> bool + 'a {
    move |l: &str| !needles.iter().any(|n| l.contains(n))
}

/// A line filter for [`Compared::filtered`]: keep only lines containing one of
/// `needles`.
pub fn only_lines_containing<'a>(needles: &'a [&'a str]) -> impl Fn(&str) -> bool + 'a {
    move |l: &str| needles.iter().any(|n| l.contains(n))
}

/// Line difference of two texts, `None` when equal. A longest-common-
/// subsequence walk: the answers compared here are tens of lines.
pub fn line_diff(nano: &str, stock: &str) -> Option<String> {
    if nano == stock {
        return None;
    }
    let a: Vec<&str> = nano.lines().collect();
    let b: Vec<&str> = stock.lines().collect();
    let (n, m) = (a.len(), b.len());
    let mut lcs = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < n || j < m {
        if i < n && j < m && a[i] == b[j] {
            out.push(format!("  {}", a[i]));
            i += 1;
            j += 1;
        } else if i < n && (j == m || lcs[i + 1][j] >= lcs[i][j + 1]) {
            out.push(format!("- {}", a[i]));
            i += 1;
        } else {
            out.push(format!("+ {}", b[j]));
            j += 1;
        }
    }
    Some(out.join("\n"))
}

/// The changed lines of a [`line_diff`], without the context lines.
pub fn changed_lines(diff: &str) -> Vec<String> {
    diff.lines()
        .filter(|l| l.starts_with("- ") || l.starts_with("+ "))
        .map(str::to_string)
        .collect()
}

/// The six parameter service lines `ros2 service list -t` prints for a node,
/// normalised (`<node>/get_parameters [rcl_interfaces/srv/GetParameters]`).
pub fn parameter_service_lines() -> Vec<String> {
    PARAMETER_SERVICES
        .iter()
        .map(|(s, t)| format!("{NODE_PLACEHOLDER}/{s} [{t}]"))
        .collect()
}

/// True if `line` names one of some node's six parameter services with its
/// type, as `ros2 service list -t` prints it (`/n/get_parameters
/// [rcl_interfaces/srv/GetParameters]`) or as `ros2 node info` does
/// (`/n/get_parameters: rcl_interfaces/srv/GetParameters`).
pub fn is_parameter_service_line(line: &str) -> bool {
    PARAMETER_SERVICES
        .iter()
        .any(|(s, t)| line.contains(&format!("/{s} [{t}]")) || line.contains(&format!("/{s}: {t}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(args: &str, output: &str, code: i32) -> CliAnswer {
        CliAnswer {
            args: args.into(),
            output: output.into(),
            exit_code: Some(code),
        }
    }

    // The acceptance of the harness itself: a pair known to match reports no
    // difference, and a seeded difference is reported. Both pairs are the
    // first measurement's real answers, `ros2 param describe` and
    // `ros2 param list`.
    const DESCRIBE: &str = "Parameter name: rate\n  Type: integer\n  Constraints:\n";

    #[test]
    fn a_matching_pair_reports_no_difference() {
        let c = compare(
            "q",
            answer("param describe /alpha rate", DESCRIBE, 0),
            "/alpha",
            answer("param describe /alpha_stock rate", DESCRIBE, 0),
            "/alpha_stock",
        );
        assert_eq!(c.diff(), None, "{}", c.report());
    }

    #[test]
    fn a_seeded_difference_is_reported_on_its_own_side() {
        let c = compare(
            "q",
            answer("param list /alpha", "  rate\n  stepped\n", 0),
            "/alpha",
            answer(
                "param list /alpha_stock",
                "  rate\n  stepped\n  use_sim_time\n",
                0,
            ),
            "/alpha_stock",
        );
        let diff = c.diff().expect("a seeded difference must be reported");
        assert_eq!(changed_lines(&diff), vec!["+   use_sim_time".to_string()]);

        let c = compare(
            "q",
            answer("x", "a\nnano-only\nb\n", 0),
            "/n",
            answer("x", "a\nb\n", 0),
            "/s",
        );
        assert_eq!(
            changed_lines(&c.diff().unwrap()),
            vec!["- nano-only".to_string()]
        );
    }

    #[test]
    fn node_names_normalise_only_as_whole_names() {
        let nano = "/alpha/get_parameters [t]\n/alpha_chatter [x]\nNode name: alpha\n";
        assert_eq!(
            normalise(nano, "/alpha"),
            "<node>/get_parameters [t]\n/alpha_chatter [x]\nNode name: <node>"
        );
        let stock = "/alpha_stock/get_parameters [t]\nNode name: alpha_stock\n";
        assert_eq!(
            normalise(stock, "/alpha_stock"),
            "<node>/get_parameters [t]\nNode name: <node>"
        );
        // A bare name inside a longer path is not the node.
        assert_eq!(normalise("/ns/alpha", "/alpha"), "/ns/alpha");
    }

    #[test]
    fn the_clis_own_middleware_log_lines_are_not_part_of_the_answer() {
        let noisy = "  rate (type: integer)\n\u{1b}[2m2026-11-12T20:01:47.655822Z\u{1b}[0m \u{1b}[33m WARN\u{1b}[0m rx-1 \
                     ThreadId(06) zenoh::api::session: Received ResponseFinal for unknown \
                     Request: 1\n[WARN] [1794512829.947758030] [rmw_zenoh_cpp]: Unable to \
                     connect to a Zenoh router.\n";
        assert_eq!(normalise(noisy, "/alpha"), "  rate (type: integer)");
        // A node's own answer that merely mentions zenoh is kept.
        assert_eq!(
            normalise("Setting parameter failed: zenoh WARN x", "/a"),
            "Setting parameter failed: zenoh WARN x"
        );
    }

    #[test]
    fn exit_status_differences_are_differences() {
        let c = compare(
            "q",
            answer("param get /a nope", "Parameter not set.\n", 0),
            "/a",
            answer("param get /b nope", "Parameter not set.\n", 1),
            "/b",
        );
        let diff = c
            .diff()
            .expect("an exit status difference must be reported");
        assert_eq!(
            changed_lines(&diff),
            vec!["- [exit 0]".to_string(), "+ [exit 1]".to_string()]
        );
    }

    #[test]
    fn ordering_differences_are_differences() {
        assert!(line_diff("a\nb", "b\na").is_some());
    }

    #[test]
    fn filters_set_a_difference_aside_on_both_sides() {
        let c = compare(
            "q",
            answer("param list /alpha", "  rate\n", 0),
            "/alpha",
            answer("param list /alpha_stock", "  rate\n  use_sim_time\n", 0),
            "/alpha_stock",
        )
        .filtered(without_lines_containing(&["use_sim_time"]));
        assert_eq!(c.diff(), None);
    }

    #[test]
    fn queries_route_by_verb() {
        assert_eq!(route_for("param list /a"), Route::NoDaemonFlag);
        assert_eq!(route_for("service call /a/b t {}"), Route::OwnNode);
        assert_eq!(route_for("action list -t"), Route::Daemonless);
        assert_eq!(route_for("action info -t /fibonacci"), Route::Daemonless);
        assert_eq!(route_for("action send_goal /f t {}"), Route::NoDaemonFlag);
    }

    #[test]
    fn parameter_service_lines_are_the_six() {
        let lines = parameter_service_lines();
        assert_eq!(lines.len(), 6);
        assert!(lines.iter().all(|l| is_parameter_service_line(l)));
        assert!(!is_parameter_service_line(
            "/add_two_ints [example_interfaces/srv/AddTwoInts]"
        ));
    }
}
