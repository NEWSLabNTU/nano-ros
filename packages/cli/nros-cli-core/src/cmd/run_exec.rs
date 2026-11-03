//! `nros run` executes its RunPlan (RFC-0118 D2, D8; RFC-0119 D6).
//!
//! [`crate::cmd::run_plan`] resolves the plan; this module carries it out:
//!
//! 1. build the image if it is stale, through `nros build` (skipped under
//!    `--plan`, which executes a plan as written);
//! 2. find the runner the plan names, on the provider search path, and check
//!    its `describe` reply (protocol, verb);
//! 3. write the plan to `<ws>/build/nros/run/<image>/plan.json`;
//! 4. start what the link needs on the host side (`--router`): the zenoh
//!    router or the XRCE agent, before the target for a network locator,
//!    after the runner reports its endpoint for a serial one;
//! 5. start `<runner> run --plan plan.json`, tee its stdout (the console) to
//!    `console.log`, grade `--expect`, and stop the whole process group at the
//!    end, whatever the end was.
//!
//! The session machinery (process group, capture, grading, exit codes) is
//! [`crate::target_session`], which `nros flash --monitor` and `nros monitor`
//! share. What is here is what only `run` does: the build, the runner lookup
//! and the router.

use std::{
    io::{self, Write},
    net::{IpAddr, SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use nros_extension::{
    describe::Describe,
    exit,
    plan::{Link, LinkKind, RunPlan, Verb},
    protocol,
};

use crate::target_session::{self as ts, Expect, Outcome, ProcessGroup, Session};

/// The flags that only matter when the plan is executed.
#[derive(clap::Args, Debug, Clone, Default)]
pub struct ExecArgs {
    /// Stop with exit 0 as soon as a console line contains TEXT; exit 1 if the
    /// target ends, or `--timeout` passes, first.
    #[arg(long, value_name = "TEXT", conflicts_with = "expect_regex")]
    pub expect: Option<String>,
    /// As `--expect`, with a regular expression over one console line.
    #[arg(long, value_name = "REGEX")]
    pub expect_regex: Option<String>,
    /// Stop the target after SECONDS.
    #[arg(long, value_name = "SECONDS")]
    pub timeout: Option<f64>,
    /// What serves the image's link on this host: `auto` starts the zenoh
    /// router or the XRCE agent when the link needs one here, `none` starts
    /// nothing, a locator names a router that is already running.
    #[arg(long, value_name = "auto|none|LOCATOR", default_value = "auto")]
    pub router: String,
    /// Run a host image under this command (`--prefix 'valgrind -q'`).
    #[arg(long, value_name = "CMD")]
    pub prefix: Option<String>,
    /// Execute this plan file as written: resolve nothing, build nothing.
    #[arg(long, value_name = "FILE")]
    pub plan: Option<PathBuf>,
}

impl ExecArgs {
    fn expectation(&self) -> Result<Option<Expect>, String> {
        if let Some(e) = &self.expect {
            return Ok(Some(Expect::substring(e)));
        }
        match &self.expect_regex {
            Some(r) => Expect::regex(r)
                .map(Some)
                .map_err(|e| format!("`--expect-regex {r}` is not a regular expression: {e}")),
            None => Ok(None),
        }
    }

    fn timeout(&self) -> Result<Option<Duration>, String> {
        match self.timeout {
            None => Ok(None),
            Some(s) if s.is_finite() && s > 0.0 => Ok(Some(Duration::from_secs_f64(s))),
            Some(s) => Err(format!(
                "`--timeout {s}` must be a positive number of seconds"
            )),
        }
    }
}

/// The router switch, parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouterMode {
    /// Start what the link needs, if anything.
    Auto,
    /// Start nothing.
    None,
    /// A router already running at this locator.
    External(String),
}

impl RouterMode {
    /// Parse `--router`.
    ///
    /// # Errors
    ///
    /// A value that is neither word nor a `<protocol>/<address>` locator.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "auto" => Ok(RouterMode::Auto),
            "none" => Ok(RouterMode::None),
            loc if loc.split_once('/').is_some_and(|(p, a)| {
                !p.is_empty() && !a.is_empty() && p.chars().all(|c| c.is_ascii_lowercase())
            }) =>
            {
                Ok(RouterMode::External(loc.to_string()))
            }
            other => Err(format!(
                "`--router {other}`: expected `auto`, `none` or a locator such as \
                 `tcp/127.0.0.1:7447`"
            )),
        }
    }
}

/// The port zenoh clients dial when nothing states a locator: the default of
/// a host image and of Zephyr `native_sim` (`CONFIG_NROS_ZENOH_LOCATOR`), and
/// the port `rmw_zenoh_cpp` uses.
pub const ZENOH_DEFAULT_LOCATOR: &str = "tcp/127.0.0.1:7447";

/// Fill in what the plan needs before it is printed or executed, so the plan
/// `--dry-run` shows is the plan that runs: the link's RMW, a host zenoh
/// image's default link, and `--prefix`.
pub fn decorate(plan: &mut RunPlan, rmw: Option<&str>, exec: &ExecArgs) {
    let rmw = rmw.map(str::to_string);
    match plan.link.as_mut() {
        Some(l) => {
            if l.rmw.is_none() {
                l.rmw = rmw;
            }
        }
        // A host image reads its locator at run time and dials the zenoh
        // default when none is set.
        None if plan.target.host && rmw.as_deref() == Some("zenoh") => {
            let locator = std::env::var("NROS_LOCATOR")
                .ok()
                .filter(|l| !l.is_empty())
                .unwrap_or_else(|| ZENOH_DEFAULT_LOCATOR.to_string());
            plan.link = Some(Link {
                kind: if locator.starts_with("udp/") {
                    LinkKind::Udp
                } else {
                    LinkKind::Tcp
                },
                locator,
                rmw,
            });
            plan.provenance.insert("link".into(), "method".into());
        }
        None => {}
    }
    if let Some(p) = &exec.prefix {
        plan.args.insert("prefix".into(), p.clone());
        plan.provenance.insert(
            "args.prefix".into(),
            crate::cmd::run_plan::INVOCATION.into(),
        );
    }
}

// ── What the link needs on the host side ────────────────────────────────

/// What `--router` starts for a plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostSide {
    /// Nothing: no link, `--router none`, an external router, or a link this
    /// host does not serve.
    Nothing(String),
    /// A zenoh router listening on this locator, started before the target.
    ZenohListen(String),
    /// An XRCE agent with these arguments, started before the target.
    XrceAgent(Vec<String>),
    /// Started once the runner reports the serial endpoint: a zenoh router
    /// (`true`) or an XRCE agent (`false`), with the locator's `#` options.
    AfterLink {
        /// Zenoh router (`true`) or XRCE agent (`false`).
        zenoh: bool,
        /// The options after `#` in the image's locator (`baudrate=115200`).
        options: String,
    },
}

/// Decide what serves the plan's link on this host (RFC-0118 D8).
#[must_use]
pub fn host_side(plan: &RunPlan, mode: &RouterMode) -> HostSide {
    let Some(link) = plan.link.as_ref() else {
        return HostSide::Nothing("the image has no network or serial link".into());
    };
    match mode {
        RouterMode::None => return HostSide::Nothing("`--router none`".into()),
        RouterMode::External(l) => return HostSide::Nothing(format!("the router at {l}")),
        RouterMode::Auto => {}
    }
    let zenoh = match link.rmw.as_deref() {
        Some("xrce") => false,
        Some("zenoh") | None => true,
        Some(other) => {
            return HostSide::Nothing(format!("the `{other}` RMW needs no router"));
        }
    };
    if link.kind == LinkKind::Serial {
        let options = link
            .locator
            .split_once('#')
            .map(|(_, o)| o.to_string())
            .unwrap_or_default();
        return HostSide::AfterLink { zenoh, options };
    }
    let Some((proto, host, port)) = split_locator(&link.locator) else {
        return HostSide::Nothing(format!("cannot read the locator `{}`", link.locator));
    };
    let Some(listen_host) = local_listen_host(&host, plan.target.emulator.is_some()) else {
        return HostSide::Nothing(format!(
            "{} is not an address of this host, so its router runs elsewhere",
            link.locator
        ));
    };
    if zenoh {
        HostSide::ZenohListen(format!("{proto}/{listen_host}:{port}"))
    } else {
        let mode = if proto == "tcp" { "tcp4" } else { "udp4" };
        HostSide::XrceAgent(vec![mode.into(), "-p".into(), port.to_string()])
    }
}

/// `tcp/10.0.2.2:7447` → (`tcp`, `10.0.2.2`, 7447).
fn split_locator(loc: &str) -> Option<(String, String, u16)> {
    let (proto, rest) = loc.split_once('/')?;
    let rest = rest.split('#').next()?;
    let (host, port) = rest.rsplit_once(':')?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    Some((proto.to_string(), host.to_string(), port.parse().ok()?))
}

/// The address a host router listens on for a target dialling `host`, or
/// `None` when `host` is not this machine.
///
/// An emulator on QEMU's user network reaches the host's loopback as
/// `10.0.2.2`. Any other address is this host's when it can be bound here.
fn local_listen_host(host: &str, emulator: bool) -> Option<String> {
    if emulator && host == "10.0.2.2" {
        return Some("127.0.0.1".into());
    }
    if host == "localhost" {
        return Some("127.0.0.1".into());
    }
    let ip: IpAddr = host.parse().ok()?;
    if ip.is_loopback() || ip.is_unspecified() {
        return Some(host.to_string());
    }
    std::net::TcpListener::bind(SocketAddr::new(ip, 0))
        .ok()
        .map(|_| host.to_string())
}

// ── The zenoh router and the XRCE agent ─────────────────────────────────

/// The zenoh router command: ROS's `rmw_zenohd` (resolved, never from
/// `PATH`), configured through `ZENOH_CONFIG_OVERRIDE` because it reads no
/// arguments, in its own prefix's environment when the caller has not sourced
/// it, and with the zenoh library it was built against ahead on the loader
/// path. The test harness's router fixture does the same.
fn zenoh_router(endpoint: &str) -> Result<Command, String> {
    let bin = crate::cmd::setup::resolve_rmw_zenohd().ok_or_else(|| {
        format!(
            "no zenoh router: `rmw_zenoh_cpp/rmw_zenohd` was not found in \
             NROS_RMW_ZENOHD, AMENT_PREFIX_PATH={} or /opt/ros/$ROS_DISTRO. Source \
             your ROS setup, install `ros-<distro>-rmw-zenoh-cpp`, or pass \
             `--router none` and start one yourself",
            std::env::var("AMENT_PREFIX_PATH").unwrap_or_else(|_| "unset".into())
        )
    })?;
    let prefix = bin
        .parent()
        .filter(|d| d.file_name().is_some_and(|n| n == "rmw_zenoh_cpp"))
        .and_then(Path::parent)
        .and_then(Path::parent)
        .map(Path::to_path_buf);
    let sourced = |p: &Path| {
        std::env::var_os("AMENT_PREFIX_PATH")
            .is_some_and(|v| std::env::split_paths(&v).any(|x| x == p))
    };
    let setup = prefix
        .as_deref()
        .map(|p| p.join("setup.bash"))
        .filter(|s| s.is_file() && !sourced(s.parent().unwrap_or(Path::new("/"))));
    let mut cmd = match setup {
        Some(setup) => {
            let mut c = Command::new("bash");
            c.arg("-c")
                .arg(r#". "$1" >/dev/null 2>&1; exec "$2""#)
                .arg("nros-router")
                .arg(setup)
                .arg(&bin);
            c
        }
        None => Command::new(&bin),
    };
    if let Some(dir) = prefix
        .map(|p| p.join("opt/zenoh_cpp_vendor/lib"))
        .filter(|d| d.join("libzenohc.so").exists())
    {
        let mut search = vec![dir];
        if let Some(inherited) = std::env::var_os("LD_LIBRARY_PATH") {
            search.extend(std::env::split_paths(&inherited));
        }
        if let Ok(joined) = std::env::join_paths(search) {
            cmd.env("LD_LIBRARY_PATH", joined);
        }
    }
    cmd.env(
        "ZENOH_CONFIG_OVERRIDE",
        format!("scouting/multicast/enabled=false;listen/endpoints=[\"{endpoint}\"]"),
    );
    Ok(cmd)
}

/// The XRCE agent: the SDK store's `MicroXRCEAgent` at the version the index
/// pins, never from `PATH`.
fn xrce_agent(args: &[String], nano_ros_root: Option<&Path>) -> Result<Command, String> {
    let index = nano_ros_root
        .map(|r| r.join("nros-sdk-index.toml"))
        .and_then(|p| crate::orchestration::sdk_index::SdkIndex::load(&p).ok());
    let version = index
        .as_ref()
        .and_then(|i| i.tool.get("xrce-agent"))
        .map(|t| t.version.clone())
        .ok_or_else(|| "no XRCE agent: the SDK index pins no `xrce-agent`".to_string())?;
    let bin = nros_build_paths::store::root_unchecked()
        .join("sdk/xrce-agent")
        .join(&version)
        .join("bin/MicroXRCEAgent");
    if !bin.is_file() {
        return Err(format!(
            "no XRCE agent at {}: provision it with `nros setup --tool xrce-agent`, or pass \
             `--router none` and start one yourself",
            bin.display()
        ));
    }
    let mut cmd = Command::new(bin);
    cmd.args(args);
    Ok(cmd)
}

fn wait_for_tcp(port: u16, within: Duration) -> bool {
    let deadline = Instant::now() + within;
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    while Instant::now() < deadline {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

/// Start `cmd` in the group as `name`, logging to `<run>/<name>.log`.
fn start_member(
    group: &mut ProcessGroup,
    name: &str,
    mut cmd: Command,
    run_dir: &Path,
) -> Result<(), String> {
    let log = std::fs::File::create(run_dir.join(format!("{name}.log")))
        .map_err(|e| format!("creating {name}.log: {e}"))?;
    let log2 = log.try_clone().map_err(|e| e.to_string())?;
    cmd.stdin(Stdio::null()).stdout(log).stderr(log2);
    group
        .spawn(name, cmd)
        .map_err(|e| format!("starting the {name}: {e}"))?;
    Ok(())
}

// ── The runner ──────────────────────────────────────────────────────────

/// A runner, found and checked.
#[derive(Debug, Clone)]
pub struct Runner {
    /// The runner name the plan gave.
    pub name: String,
    /// The executable.
    pub exe: PathBuf,
}

/// Find the plan's runner on the provider search path and locate its built
/// executable.
// The one lookup for a runner package in this verb; it becomes the shared
// provider-resolution function when that lands.
fn locate_runner(ws: &Path, nano_ros_root: Option<&Path>, name: &str) -> Result<PathBuf, String> {
    let path = crate::cmd::ws::provider_search_path(ws).map_err(|e| e.to_string())?;
    let scan =
        cargo_nano_ros::provider_scan::scan_roots(&path.paths()).map_err(|e| e.to_string())?;
    let r = cargo_nano_ros::extension_descriptor::resolve_runner_in(&scan, name)
        .map_err(|e| format!("runner `{name}`: {e}"))?;
    let dir = r.dir.canonicalize().unwrap_or(r.dir.clone());
    let ws = ws.canonicalize().unwrap_or(ws.to_path_buf());
    // A package of this workspace: `nros build` built it for the host.
    if dir.starts_with(&ws) {
        let triple = crate::orchestration::metadata_build::host_triple();
        return crate::builder::host_tool::executable(
            &ws,
            &r.package,
            &r.bin,
            &triple,
            &format!("runner `{name}`"),
        )
        .map_err(|e| e.to_string());
    }
    // A nano-ros checkout's own runner: built beside the CLI.
    if let Some(root) = nano_ros_root.and_then(|p| p.canonicalize().ok())
        && dir.starts_with(root.join("packages/runners"))
    {
        let exe = root.join("packages/runners/target/release").join(&r.bin);
        return if exe.is_file() {
            Ok(exe)
        } else {
            Err(format!(
                "runner `{name}` (package `{}`) is not built (looked for {}). Build it with \
                 `just setup-cli` in {}",
                r.package,
                exe.display(),
                root.display()
            ))
        };
    }
    // A released toolchain ships the package prebuilt, under `bin/`.
    let exe = dir.join("bin").join(&r.bin);
    if exe.is_file() {
        Ok(exe)
    } else {
        Err(format!(
            "runner `{name}` (package `{}` at {}) has no built executable `bin/{}`",
            r.package,
            dir.display(),
            r.bin
        ))
    }
}

/// Ask the runner what it speaks and check it can run this plan.
///
/// # Errors
///
/// The runner did not answer, speaks no protocol in common with this nros,
/// or does not implement `verb`.
pub fn check_runner(exe: &Path, name: &str, verb: Verb) -> Result<Describe, String> {
    let out = Command::new(exe)
        .arg("describe")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("runner `{name}` ({}): {e}", exe.display()))?;
    if !out.status.success() {
        return Err(format!(
            "runner `{name}` ({}) failed `describe`: {}",
            exe.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let d = Describe::parse(&String::from_utf8_lossy(&out.stdout))
        .map_err(|e| format!("runner `{name}`: {e}"))?;
    d.negotiate(protocol::SUPPORTED)
        .map_err(|e| format!("runner `{name}`: {e}"))?;
    if !d.supports(verb) {
        return Err(format!(
            "runner `{name}` does not implement `{verb}` (it states {:?})",
            d.verbs.iter().map(|v| v.as_str()).collect::<Vec<_>>()
        ));
    }
    Ok(d)
}

// ── Executing ───────────────────────────────────────────────────────────

/// Where a session writes its plan and logs: `<ws>/build/nros/run/<image>/`.
#[must_use]
pub fn run_dir(ws: &Path, image: &str) -> PathBuf {
    let safe: String = image
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    ws.join("build/nros/run").join(safe)
}

/// Build `image` through `nros build` with the build's output on stderr, so
/// stdout stays the console. `true` when the build succeeded.
pub fn build_image(ws: &Path, nano_ros_path: Option<&Path>, image: &str) -> Result<(), String> {
    let me = std::env::current_exe().map_err(|e| format!("locating nros: {e}"))?;
    let mut cmd = Command::new(me);
    cmd.arg("build").arg("--workspace").arg(ws);
    if let Some(n) = nano_ros_path {
        cmd.arg("--nano-ros-path").arg(n);
    }
    cmd.arg(image);
    let stderr = io::stderr()
        .as_fd_dup()
        .map_err(|e| format!("duplicating stderr: {e}"))?;
    cmd.stdin(Stdio::null()).stdout(stderr);
    let status = cmd
        .status()
        .map_err(|e| format!("running nros build: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`nros build {image}` failed ({status})"))
    }
}

trait DupFd {
    fn as_fd_dup(&self) -> io::Result<Stdio>;
}

impl DupFd for io::Stderr {
    fn as_fd_dup(&self) -> io::Result<Stdio> {
        use std::os::fd::AsFd;
        Ok(Stdio::from(self.as_fd().try_clone_to_owned()?))
    }
}

fn workspace_of(args: &crate::cmd::run_plan::Args) -> PathBuf {
    let ws = args
        .workspace
        .clone()
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    ws.canonicalize().unwrap_or(ws)
}

fn exit_with(code: u8) -> ! {
    let _ = io::stdout().flush();
    std::process::exit(i32::from(code));
}

/// `nros run` on a resolved plan: build the image if it is stale, then
/// [`execute`]. Exits the process with the verb's code.
///
/// # Errors
///
/// Never returns an error; every failure is an exit code (RFC-0118 D2).
pub fn build_and_execute(
    args: &crate::cmd::run_plan::Args,
    image: &str,
    plan: &RunPlan,
) -> eyre::Result<()> {
    let ws = workspace_of(args);
    let nano_ros_root =
        crate::orchestration::nano_ros_root::resolve(args.nano_ros_path.clone(), &ws);
    if let Err(e) = build_image(&ws, args.nano_ros_path.as_deref(), image) {
        eprintln!("nros run: {e}");
        exit_with(exit::RUNNER_FAILED);
    }
    exit_with(execute(
        plan,
        &Execution {
            ws: &ws,
            nano_ros_root: nano_ros_root.as_deref(),
            exec: &args.exec,
            runner: None,
        },
    ));
}

/// `nros run --plan <file>`: execute the plan as written. Nothing is resolved
/// and nothing is built; `--dry-run` prints it back.
///
/// # Errors
///
/// Never returns an error; every failure is an exit code (RFC-0118 D2).
pub fn run_plan_file(args: &crate::cmd::run_plan::Args, file: &Path) -> eyre::Result<()> {
    let mut plan = match RunPlan::read(file) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("nros run: {e}");
            exit_with(exit::PLAN_UNRESOLVED);
        }
    };
    if plan.verb != Verb::Run {
        eprintln!(
            "nros run: {} is a `{}` plan; `nros run` executes `run` plans",
            file.display(),
            plan.verb
        );
        exit_with(exit::PLAN_UNRESOLVED);
    }
    if !args.passthrough.is_empty() {
        plan.passthrough = args.passthrough.clone();
    }
    if let Some(p) = &args.exec.prefix {
        plan.args.insert("prefix".into(), p.clone());
    }
    if args.dry_run {
        print!(
            "{}",
            crate::cmd::run_plan::render_plan_text(&plan, args.why, args.json)
        );
        return Ok(());
    }
    let ws = workspace_of(args);
    let nano_ros_root =
        crate::orchestration::nano_ros_root::resolve(args.nano_ros_path.clone(), &ws);
    exit_with(execute(
        &plan,
        &Execution {
            ws: &ws,
            nano_ros_root: nano_ros_root.as_deref(),
            exec: &args.exec,
            runner: None,
        },
    ));
}

/// The plan's console command, split into words and expanded from the plan
/// (RFC-0118 D5 placeholders), or `None` when the runner owns the console.
///
/// # Errors
///
/// An unclosed quote, an unknown placeholder or one the plan does not carry.
pub fn console_command(plan: &RunPlan) -> Result<Option<Vec<String>>, String> {
    let Some(template) = plan.console.command.as_deref() else {
        return Ok(None);
    };
    let words = nros_extension::runner::split_words(template)
        .map_err(|e| format!("the console command: {e}"))?;
    let words = words
        .iter()
        .map(|w| {
            plan.expand(w)
                .map_err(|e| format!("the console command `{template}`: {e}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if words.is_empty() {
        return Err("the console command is empty".to_string());
    }
    Ok(Some(words))
}

/// The last `n` lines of a text file (lossy), oldest first.
fn last_lines(path: &Path, n: usize) -> Vec<String> {
    let Ok(bytes) = std::fs::read(path) else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&bytes);
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..]
        .iter()
        .map(|l| l.trim_end_matches('\r').to_string())
        .collect()
}

/// What [`execute`] needs besides the plan.
#[derive(Debug, Clone)]
pub struct Execution<'a> {
    /// The workspace root (for the run directory and the runner lookup).
    pub ws: &'a Path,
    /// The nano-ros checkout, when there is one.
    pub nano_ros_root: Option<&'a Path>,
    /// The flags.
    pub exec: &'a ExecArgs,
    /// The runner, already located (tests pass a fake). `None`: look it up.
    pub runner: Option<PathBuf>,
}

/// Execute a resolved plan and return the verb's exit code (RFC-0118 D2).
pub fn execute(plan: &RunPlan, x: &Execution<'_>) -> u8 {
    let fail = |code: u8, msg: &str| {
        eprintln!("nros run: {msg}");
        code
    };
    let expect = match x.exec.expectation() {
        Ok(e) => e,
        Err(e) => return fail(exit::PLAN_UNRESOLVED, &e),
    };
    let timeout = match x.exec.timeout() {
        Ok(t) => t,
        Err(e) => return fail(exit::PLAN_UNRESOLVED, &e),
    };
    let mode = match RouterMode::parse(&x.exec.router) {
        Ok(m) => m,
        Err(e) => return fail(exit::PLAN_UNRESOLVED, &e),
    };
    let exe = match &x.runner {
        Some(p) => p.clone(),
        None => match locate_runner(x.ws, x.nano_ros_root, &plan.runner.name) {
            Ok(p) => p,
            Err(e) => return fail(exit::RUNNER_FAILED, &e),
        },
    };
    if let Err(e) = check_runner(&exe, &plan.runner.name, plan.verb) {
        return fail(exit::RUNNER_FAILED, &e);
    }

    let dir = run_dir(x.ws, &plan.image);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return fail(
            exit::RUNNER_FAILED,
            &format!("creating {}: {e}", dir.display()),
        );
    }
    let plan_path = dir.join("plan.json");
    if let Err(e) = plan.write(&plan_path) {
        return fail(
            exit::RUNNER_FAILED,
            &format!("writing {}: {e}", plan_path.display()),
        );
    }
    let log = match std::fs::File::create(dir.join("console.log")) {
        Ok(f) => f,
        Err(e) => return fail(exit::RUNNER_FAILED, &format!("creating console.log: {e}")),
    };

    ts::install_interrupt_handler();
    let started = Instant::now();
    let mut group = ProcessGroup::new();
    let mut required = Vec::new();

    // ---- the host side of the link, when it can start first -------------
    let side = host_side(plan, &mode);
    match &side {
        HostSide::Nothing(why) => {
            if plan.link.is_some() {
                eprintln!("nros run: starting no router ({why})");
            }
        }
        HostSide::ZenohListen(endpoint) => {
            let cmd = match zenoh_router(endpoint) {
                Ok(c) => c,
                Err(e) => return fail(exit::RUNNER_FAILED, &e),
            };
            eprintln!("nros run: starting the zenoh router on {endpoint}");
            if let Err(e) = start_member(&mut group, "router", cmd, &dir) {
                return fail(exit::RUNNER_FAILED, &e);
            }
            let port = split_locator(endpoint).map(|(_, _, p)| p);
            if endpoint.starts_with("tcp/")
                && let Some(port) = port
                && !wait_for_tcp(port, Duration::from_secs(10))
            {
                let died = group.poll("router").is_some();
                return fail(
                    exit::RUNNER_FAILED,
                    &format!(
                        "the zenoh router {} on {endpoint} (log: {}); is the port taken?",
                        if died { "exited" } else { "is not listening" },
                        dir.join("router.log").display()
                    ),
                );
            }
            required.push("router".to_string());
        }
        HostSide::XrceAgent(args) => {
            let cmd = match xrce_agent(args, x.nano_ros_root) {
                Ok(c) => c,
                Err(e) => return fail(exit::RUNNER_FAILED, &e),
            };
            eprintln!("nros run: starting the XRCE agent ({})", args.join(" "));
            if let Err(e) = start_member(&mut group, "agent", cmd, &dir) {
                return fail(exit::RUNNER_FAILED, &e);
            }
            std::thread::sleep(Duration::from_millis(300));
            required.push("agent".to_string());
        }
        HostSide::AfterLink { .. } => {}
    }

    // ---- the runner --------------------------------------------------------
    let mut cmd = Command::new(&exe);
    cmd.arg(plan.verb.as_str())
        .arg("--plan")
        .arg(&plan_path)
        // The target is not in the terminal's foreground group, so it must
        // not read the terminal (it would stop on SIGTTIN).
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let RouterMode::External(l) = &mode {
        cmd.env("NROS_LOCATOR", l);
    }
    // A stated console command attaches the console instead of the runner
    // (`[board.console] command`): its stdout is what is graded and logged,
    // and the runner's own stdout becomes a diagnostic on stderr.
    let console_cmd = match console_command(plan) {
        Ok(c) => c,
        Err(e) => return fail(exit::PLAN_UNRESOLVED, &e),
    };
    let (tx, rx) = ts::channel();
    let mut log = Some(log);
    match group.spawn("runner", cmd) {
        Ok(m) => {
            let out = m.child.stdout.take().expect("piped");
            let err = m.child.stderr.take().expect("piped");
            if console_cmd.is_some() {
                let (side_tx, _side_rx) = ts::channel();
                ts::spawn_console(out, Box::new(io::stderr()), None, None, side_tx);
            } else {
                ts::spawn_console(
                    out,
                    Box::new(io::stdout()),
                    log.take(),
                    expect.clone(),
                    tx.clone(),
                );
            }
            ts::spawn_stderr(err, tx.clone());
        }
        Err(e) => {
            return fail(
                exit::RUNNER_FAILED,
                &format!(
                    "starting runner `{}` ({}): {e}",
                    plan.runner.name,
                    exe.display()
                ),
            );
        }
    }

    if let Some(words) = console_cmd {
        let mut c = Command::new(&words[0]);
        c.args(&words[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        match group.spawn("console", c) {
            Ok(m) => {
                let out = m.child.stdout.take().expect("piped");
                ts::spawn_console(out, Box::new(io::stdout()), log.take(), expect.clone(), tx);
                required.push("console".to_string());
            }
            Err(e) => {
                return fail(
                    exit::RUNNER_FAILED,
                    &format!("starting the console command `{}`: {e}", words.join(" ")),
                );
            }
        }
    } else {
        drop(tx);
    }

    let mut session = Session::new(group, "runner", timeout);
    session.required = required;
    let nano_ros_root = x.nano_ros_root.map(Path::to_path_buf);
    let mut linked = false;
    let outcome = session.watch(&rx, started, |endpoint, s| {
        let HostSide::AfterLink { zenoh, options } = &side else {
            return Ok(());
        };
        if linked {
            return Ok(());
        }
        linked = true;
        let (name, cmd) = if *zenoh {
            let ep = if options.is_empty() {
                format!("serial/{endpoint}")
            } else {
                format!("serial/{endpoint}#{options}")
            };
            eprintln!("nros run: starting the zenoh router on {ep}");
            ("router", zenoh_router(&ep)?)
        } else {
            let baud = options
                .split('&')
                .find_map(|kv| kv.strip_prefix("baudrate="))
                .unwrap_or("115200")
                .to_string();
            let args = vec![
                "serial".into(),
                "--dev".into(),
                endpoint.to_string(),
                "-b".into(),
                baud,
            ];
            eprintln!("nros run: starting the XRCE agent ({})", args.join(" "));
            ("agent", xrce_agent(&args, nano_ros_root.as_deref())?)
        };
        start_member(&mut s.group, name, cmd, &dir)?;
        s.required.push(name.to_string());
        Ok(())
    });
    session.group.teardown(ts::TEARDOWN_GRACE);
    let code = outcome.exit_code(expect.is_some());
    let line = outcome.describe(expect.as_ref(), started.elapsed());
    let _ = io::stdout().flush();
    eprintln!(
        "nros run: {line} (console: {})",
        dir.join("console.log").display()
    );
    // An unmet expectation: the last console lines, on stderr, for a reader
    // whose stdout went to a file or a pipe.
    if expect.is_some() && code != exit::SUCCESS {
        let tail = last_lines(&dir.join("console.log"), 10);
        if !tail.is_empty() {
            eprintln!("nros run: the console's last lines:");
            for l in tail {
                eprintln!("  | {l}");
            }
        }
    }
    if let Outcome::MemberDied { name, .. } = &outcome {
        eprintln!(
            "nros run: the {name}'s log: {}",
            dir.join(format!("{name}.log")).display()
        );
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;
    use nros_extension::plan::{Console, ConsoleKind, RunnerRef, Target};
    use std::collections::BTreeMap;

    fn plan(host: bool, link: Option<Link>) -> RunPlan {
        RunPlan {
            protocol: 1,
            image: "talker".into(),
            artifacts: BTreeMap::new(),
            build_dir: None,
            verb: Verb::Run,
            runner: RunnerRef {
                package: "nros_runner_fake".into(),
                name: "fake".into(),
            },
            target: Target {
                emulator: (!host).then(|| "qemu:mps2-an385".into()),
                host,
                flasher: None,
            },
            console: Console {
                kind: ConsoleKind::Stdio,
                port: None,
                baud: None,
                command: None,
            },
            link,
            args: BTreeMap::new(),
            tools: BTreeMap::new(),
            passthrough: Vec::new(),
            prepare: Vec::new(),
            provenance: BTreeMap::new(),
        }
    }

    fn link(kind: LinkKind, locator: &str, rmw: &str) -> Option<Link> {
        Some(Link {
            kind,
            locator: locator.into(),
            rmw: Some(rmw.into()),
        })
    }

    #[test]
    fn router_switch_parses() {
        assert_eq!(RouterMode::parse("auto"), Ok(RouterMode::Auto));
        assert_eq!(RouterMode::parse("none"), Ok(RouterMode::None));
        assert_eq!(
            RouterMode::parse("tcp/10.0.0.5:7447"),
            Ok(RouterMode::External("tcp/10.0.0.5:7447".into()))
        );
        assert!(RouterMode::parse("router=tcp/x:1").is_err());
        assert!(RouterMode::parse("bogus").is_err());
    }

    #[test]
    fn an_emulator_dialling_the_user_net_host_gets_a_loopback_router() {
        let p = plan(false, link(LinkKind::Tcp, "tcp/10.0.2.2:10500", "zenoh"));
        assert_eq!(
            host_side(&p, &RouterMode::Auto),
            HostSide::ZenohListen("tcp/127.0.0.1:10500".into())
        );
        assert!(matches!(
            host_side(&p, &RouterMode::None),
            HostSide::Nothing(_)
        ));
    }

    #[test]
    fn a_router_on_another_host_is_not_started() {
        // TEST-NET-1: never an address of this machine.
        let p = plan(false, link(LinkKind::Tcp, "tcp/192.0.2.7:7447", "zenoh"));
        assert!(matches!(
            host_side(&p, &RouterMode::Auto),
            HostSide::Nothing(_)
        ));
    }

    #[test]
    fn xrce_starts_an_agent_on_the_locators_port() {
        let p = plan(true, link(LinkKind::Udp, "udp/127.0.0.1:2018", "xrce"));
        assert_eq!(
            host_side(&p, &RouterMode::Auto),
            HostSide::XrceAgent(vec!["udp4".into(), "-p".into(), "2018".into()])
        );
    }

    #[test]
    fn a_serial_link_waits_for_the_runners_endpoint() {
        let p = plan(
            false,
            link(LinkKind::Serial, "serial/UART_0#baudrate=115200", "zenoh"),
        );
        assert_eq!(
            host_side(&p, &RouterMode::Auto),
            HostSide::AfterLink {
                zenoh: true,
                options: "baudrate=115200".into()
            }
        );
    }

    #[test]
    fn cyclonedds_needs_no_router() {
        let p = plan(
            true,
            link(LinkKind::Udp, "udp/127.0.0.1:7400", "cyclonedds"),
        );
        assert!(matches!(
            host_side(&p, &RouterMode::Auto),
            HostSide::Nothing(_)
        ));
    }

    #[test]
    fn decorate_gives_a_host_zenoh_image_the_default_link_and_the_prefix() {
        let mut p = plan(true, None);
        let x = ExecArgs {
            prefix: Some("valgrind -q".into()),
            ..ExecArgs::default()
        };
        decorate(&mut p, Some("zenoh"), &x);
        let l = p.link.as_ref().unwrap();
        // `NROS_LOCATOR`, when the environment sets it, wins over the default.
        let expected = std::env::var("NROS_LOCATOR")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| ZENOH_DEFAULT_LOCATOR.to_string());
        assert_eq!(l.locator, expected);
        assert_eq!(l.rmw.as_deref(), Some("zenoh"));
        assert_eq!(p.args["prefix"], "valgrind -q");
        assert_eq!(p.provenance["args.prefix"], "invocation");

        // An emulator's link comes only from its image.
        let mut q = plan(false, None);
        decorate(&mut q, Some("zenoh"), &ExecArgs::default());
        assert!(q.link.is_none());
    }

    #[test]
    fn a_console_command_is_graded_instead_of_the_runner() {
        let _g = crate::target_session::tests::FLAG
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let d = tempfile::tempdir().unwrap();
        let r = fake_runner(d.path(), "echo 'Publishing: 0'; sleep 30");
        let mut p = plan(true, None);
        p.console.command = Some("sh -c 'echo console says hi; sleep 30'".into());
        let x = |e: &str| ExecArgs {
            expect: Some(e.into()),
            timeout: Some(3.0),
            router: "none".into(),
            ..ExecArgs::default()
        };
        let run = |x: ExecArgs| {
            execute(
                &p,
                &Execution {
                    ws: d.path(),
                    nano_ros_root: None,
                    exec: &x,
                    runner: Some(r.clone()),
                },
            )
        };
        assert_eq!(run(x("console says hi")), 0);
        // The runner's stdout is no longer the console.
        assert_eq!(run(x("Publishing: 0")), 1);

        let mut q = plan(true, None);
        q.console.command = Some("picocom {port}".into());
        assert!(console_command(&q).unwrap_err().contains("port"));
    }

    #[test]
    fn run_dir_is_one_directory_per_image() {
        assert_eq!(
            run_dir(Path::new("/ws"), "bringup:talker"),
            PathBuf::from("/ws/build/nros/run/bringup_talker")
        );
    }

    // ---- the executor against a fake runner ------------------------------
    //
    // The fake runner is a shell script speaking the runner contract
    // (`describe`, `run --plan <file>`): nothing is compiled.

    fn fake_runner(dir: &Path, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join("fake-runner");
        std::fs::write(
            &p,
            format!(
                "#!/bin/sh\n\
                 if [ \"$1\" = describe ]; then\n\
                 echo '{{\"protocol\":[1],\"verbs\":[\"run\"],\"needs\":[]}}'; exit 0\n\
                 fi\n\
                 [ \"$1\" = run ] && [ \"$2\" = --plan ] && [ -f \"$3\" ] || exit 2\n\
                 {body}\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    fn exec_with(dir: &Path, runner: PathBuf, x: ExecArgs) -> u8 {
        // The interrupt flag is process-global; see `target_session`'s tests.
        let _g = crate::target_session::tests::FLAG
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let p = plan(true, None);
        execute(
            &p,
            &Execution {
                ws: dir,
                nano_ros_root: None,
                exec: &x,
                runner: Some(runner),
            },
        )
    }

    #[test]
    fn executor_matches_and_captures_the_console() {
        let d = tempfile::tempdir().unwrap();
        let r = fake_runner(d.path(), "echo boot; echo 'Publishing: 0'; sleep 30");
        let code = exec_with(
            d.path(),
            r,
            ExecArgs {
                expect: Some("Publishing: 0".into()),
                timeout: Some(10.0),
                router: "none".into(),
                ..ExecArgs::default()
            },
        );
        assert_eq!(code, 0);
        let run = run_dir(d.path(), "talker");
        let log = std::fs::read_to_string(run.join("console.log")).unwrap();
        assert!(log.contains("Publishing: 0"), "{log}");
        let written = RunPlan::read(&run.join("plan.json")).unwrap();
        assert_eq!(written.image, "talker");
    }

    #[test]
    fn executor_exit_codes_follow_the_contract() {
        let d = tempfile::tempdir().unwrap();
        let x = |expect: Option<&str>, timeout: f64| ExecArgs {
            expect: expect.map(str::to_string),
            timeout: Some(timeout),
            router: "none".into(),
            ..ExecArgs::default()
        };
        // Timed out waiting: 1.
        let r = fake_runner(d.path(), "sleep 30");
        assert_eq!(exec_with(d.path(), r, x(Some("never"), 0.5)), 1);
        // The target failed: the runner's 1.
        let r = fake_runner(d.path(), "echo bad; exit 1");
        assert_eq!(exec_with(d.path(), r, x(None, 10.0)), 1);
        // The runner failed: 2.
        let r = fake_runner(d.path(), "exit 2");
        assert_eq!(exec_with(d.path(), r, x(None, 10.0)), 2);
        // Ran cleanly with no expectation: 0.
        let r = fake_runner(d.path(), "echo ok");
        assert_eq!(exec_with(d.path(), r, x(None, 10.0)), 0);
    }

    /// A runner PACKAGE in the workspace, found the way `nros run` finds one:
    /// the provider search path, its descriptor, and the executable `nros
    /// build` leaves in the host-tool directory. The "built" executable is
    /// the shell-script fake, placed where the CMake road puts it.
    #[test]
    fn executor_finds_a_workspace_runner_package() {
        use std::os::unix::fs::PermissionsExt;
        let _g = crate::target_session::tests::FLAG
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let ws = tempfile::tempdir().unwrap();
        let ws = ws.path().canonicalize().unwrap();
        let pkg = ws.join("src/w4_fake_runner");
        std::fs::create_dir_all(&pkg).unwrap();
        std::fs::write(
            pkg.join("package.xml"),
            "<?xml version=\"1.0\"?><package format=\"3\"><name>w4_fake_runner</name>\
             <version>0.1.0</version><description>d</description>\
             <maintainer email=\"a@b.c\">m</maintainer><license>MIT</license>\
             <export><build_type>nros_cmake</build_type>\
             <nano_ros_provides kind=\"runner\" name=\"w4fake\"/></export></package>",
        )
        .unwrap();
        std::fs::write(
            pkg.join("nros-runner.toml"),
            "protocol = 1\nverbs = [\"run\"]\nbin = \"w4-fake-runner\"\n",
        )
        .unwrap();
        let built = crate::builder::host_tool::build_dir(&ws, "w4_fake_runner").join("cmake");
        std::fs::create_dir_all(&built).unwrap();
        let script = fake_runner(&built, "echo 'Publishing: 0'; sleep 30");
        let exe = built.join("w4-fake-runner");
        std::fs::rename(&script, &exe).unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert_eq!(locate_runner(&ws, None, "w4fake").unwrap(), exe);

        let mut p = plan(true, None);
        p.runner = RunnerRef {
            package: "w4_fake_runner".into(),
            name: "w4fake".into(),
        };
        let x = ExecArgs {
            expect: Some("Publishing: 0".into()),
            timeout: Some(10.0),
            router: "none".into(),
            ..ExecArgs::default()
        };
        let code = execute(
            &p,
            &Execution {
                ws: &ws,
                nano_ros_root: None,
                exec: &x,
                runner: None,
            },
        );
        assert_eq!(code, 0);

        // Not built: refused with the command that builds it.
        std::fs::remove_file(&exe).unwrap();
        let e = locate_runner(&ws, None, "w4fake").unwrap_err();
        assert!(
            e.contains("nros build --packages-select w4_fake_runner"),
            "{e}"
        );
    }

    #[test]
    fn a_runner_without_the_verb_or_the_protocol_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("old-runner");
        std::fs::write(
            &p,
            "#!/bin/sh\necho '{\"protocol\":[9],\"verbs\":[\"run\"],\"needs\":[]}'\n",
        )
        .unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        let e = check_runner(&p, "old", Verb::Run).unwrap_err();
        assert!(e.contains("protocol"), "{e}");
        let x = ExecArgs {
            router: "none".into(),
            ..ExecArgs::default()
        };
        assert_eq!(exec_with(d.path(), p, x), 2);
    }
}
