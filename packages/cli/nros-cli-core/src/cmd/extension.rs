//! Command dispatch (RFC-0119 D7, phase-527 W4).
//!
//! `nros <verb> <args…>` with no built-in match looks up an announced
//! `command` named `<verb>` and executes its `bin` by absolute path with
//! `<args…>` unchanged; `nros dev <verb>` does the same for a `tier = "dev"`
//! command. The command's environment carries `NROS_WS_ROOT`,
//! `NROS_TOOLCHAIN` (this toolchain's version) and `NROS_CLI` (the absolute
//! path of the running `nros`), and its exit status passes through.
//!
//! This runs in the toolchain's `nros`, after the launcher has selected the
//! toolchain (RFC-0097 D4): the launcher parses nothing.
//!
//! The provider choice is
//! [`resolve_command_with`](cargo_nano_ros::extension_descriptor::resolve_command_with)'s,
//! over the same search path and override policy as every other resolution
//! (RFC-0119 D4), and the executable is found by the ladder runner lookup
//! uses ([`crate::builder::host_tool::locate_built`]). Nothing here scans or
//! orders providers itself.
//!
//! A built-in or reserved verb cannot be provided: a command announcing one is
//! refused at discovery, here and in `nros help --all`, never a shadow. The
//! built-in always matches first anyway, so the refusal is what tells the
//! package's author why their verb never runs.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use cargo_nano_ros::{
    extension_descriptor::{CommandTier, ResolvedCommand, resolve_command_with},
    package_xml::COMMAND_KIND,
    provider_scan::ScanResult,
};

/// The environment variables a command receives (RFC-0119 D7).
pub const ENV_WS_ROOT: &str = "NROS_WS_ROOT";
/// This toolchain's version.
pub const ENV_TOOLCHAIN: &str = "NROS_TOOLCHAIN";
/// The absolute path of the running `nros`.
pub const ENV_CLI: &str = "NROS_CLI";

/// The exit code of a command nros could not start: RFC-0118 D2's "the plan
/// does not resolve", the code reserved for nros itself.
pub const REFUSED: i32 = nros_extension::exit::PLAN_UNRESOLVED as i32;

/// What one argv asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// `nros <verb>` or `nros dev <verb>`.
    pub tier: CommandTier,
    /// The verb.
    pub verb: String,
    /// Everything after the verb, unchanged.
    pub args: Vec<OsString>,
}

impl Request {
    /// Read the verb out of `argv` (without the program name). `None` when the
    /// first word is a flag, or `nros dev` names no verb.
    #[must_use]
    pub fn from_argv(argv: &[OsString]) -> Option<Request> {
        let first = argv.first()?.to_str()?;
        if first.starts_with('-') {
            return None;
        }
        if first == "dev" {
            let verb = argv.get(1)?.to_str()?;
            if verb.starts_with('-') {
                return None;
            }
            return Some(Request {
                tier: CommandTier::Dev,
                verb: verb.to_string(),
                args: argv[2..].to_vec(),
            });
        }
        Some(Request {
            tier: CommandTier::User,
            verb: first.to_string(),
            args: argv[1..].to_vec(),
        })
    }

    /// How the user spelt it: `nros router`, `nros dev fixture-hello`.
    #[must_use]
    pub fn spelling(&self) -> String {
        spelling(self.tier, &self.verb)
    }
}

fn spelling(tier: CommandTier, verb: &str) -> String {
    match tier {
        CommandTier::User => format!("nros {verb}"),
        CommandTier::Dev => format!("nros dev {verb}"),
    }
}

/// A resolved, built command, ready to execute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    /// The executable, absolute.
    pub exe: PathBuf,
    /// The arguments, unchanged.
    pub args: Vec<OsString>,
    /// The three variables of RFC-0119 D7, in order.
    pub env: Vec<(&'static str, OsString)>,
    /// What the override policy had to say, printed once before executing.
    pub warnings: Vec<String>,
}

/// What a lookup found.
#[derive(Debug)]
pub enum Lookup {
    /// No package announces the verb: the caller prints clap's own error.
    NotACommand,
    /// Run it.
    Run(Box<Invocation>),
    /// A package announces the verb and cannot run: say why and exit
    /// [`REFUSED`].
    Refused(String),
}

/// The search path's scan and the workspace's override policy, as every
/// resolution reads them.
fn scan_and_policy(
    ws: &Path,
) -> Result<
    (
        ScanResult,
        cargo_nano_ros::provider_overrides::OverridePolicy,
    ),
    String,
> {
    // Commands take their arguments unchanged, so `nros <verb>` has no
    // `--allow-overriding` or `--base-paths`: only declared overrides and the
    // default source roots apply.
    let policy = crate::provider_overrides::policy(ws, &[]).map_err(|e| format!("{e:#}"))?;
    // Every unknown verb reaches this scan, so a directory that is not a
    // workspace is not walked as one: a typo in `$HOME` would otherwise walk
    // `$HOME`. The underlay and `NROS_PACKAGE_PATH` still answer there.
    let path = if is_workspace(ws) {
        crate::cmd::ws::provider_search_path_for(ws, &[]).map_err(|e| format!("{e:#}"))?
    } else {
        crate::cmd::ws::provider_search_path_without_sources(ws)
    };
    let scan = cargo_nano_ros::provider_scan::scan_search_path(&path).map_err(|e| e.to_string())?;
    Ok((scan, policy))
}

/// Whether `dir` is a workspace whose packages a command lookup should walk:
/// it holds an `nros.toml`, a `system.toml`, a `package.xml` (a single-package
/// leaf) or a `src/` directory (colcon's layout).
#[must_use]
pub fn is_workspace(dir: &Path) -> bool {
    ["nros.toml", "system.toml", "package.xml"]
        .iter()
        .any(|f| dir.join(f).is_file())
        || dir.join("src").is_dir()
}

fn announces(scan: &ScanResult, verb: &str) -> bool {
    scan.providers.iter().any(|p| {
        p.provides
            .iter()
            .any(|v| v.kind == COMMAND_KIND && v.name == verb)
    })
}

/// The refusal for a command named like a built-in or reserved verb, when it
/// is one; `None` when the name is free for its tier.
fn taken_refusal(c: &ResolvedCommand, root: &clap::Command) -> Option<String> {
    let dev = c.descriptor.tier == CommandTier::Dev;
    if !crate::builtin_verbs::taken(root, dev).contains(&c.name) {
        return None;
    }
    // RFC-0119 D7: a refusal at discovery, never a shadow.
    Some(format!(
        "package `{}` at {} announces command `{}`, which is a built-in or reserved \
         `nros` verb{}. A built-in cannot be provided, and the package is refused \
         rather than shadowing it. Name the verb something else.",
        c.package,
        c.dir.display(),
        c.name,
        if dev { " (or a `nros dev` verb)" } else { "" },
    ))
}

/// Look `req` up for the workspace `ws`, with `nano_ros_root` the tree a
/// checkout's own command packages are built in.
#[must_use]
pub fn lookup(req: &Request, ws: &Path, nano_ros_root: Option<&Path>) -> Lookup {
    let (scan, policy) = match scan_and_policy(ws) {
        Ok(v) => v,
        // A workspace whose search path cannot be read is not evidence that
        // the verb is a command; let clap report the unknown verb.
        Err(_) => return Lookup::NotACommand,
    };
    if !announces(&scan, &req.verb) {
        return Lookup::NotACommand;
    }
    let wanted = format!("`{}`", req.spelling());
    let c = match resolve_command_with(&scan, &req.verb, &policy) {
        Ok(c) => c,
        Err(e) => return Lookup::Refused(format!("{wanted}: {e}")),
    };
    if let Some(r) = taken_refusal(&c, &crate::builtin_verbs::tree()) {
        return Lookup::Refused(r);
    }
    if c.descriptor.tier != req.tier {
        return Lookup::Refused(format!(
            "{wanted} is not a verb: package `{}` provides it as `{}` (tier = \"{}\" in its \
             nros-command.toml).",
            c.package,
            spelling(c.descriptor.tier, &c.name),
            tier_word(c.descriptor.tier),
        ));
    }
    let exe = match crate::builder::host_tool::locate_built(
        ws,
        nano_ros_root,
        &c.package,
        &c.dir,
        &c.bin,
        &wanted,
    ) {
        Ok(e) => e,
        Err(m) => return Lookup::Refused(m),
    };
    let cli = std::env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok())
        .unwrap_or_default();
    Lookup::Run(Box::new(Invocation {
        exe,
        args: req.args.clone(),
        env: vec![
            (ENV_WS_ROOT, ws.as_os_str().to_os_string()),
            (ENV_TOOLCHAIN, env!("CARGO_PKG_VERSION").into()),
            (ENV_CLI, cli.into_os_string()),
        ],
        warnings: c.warnings,
    }))
}

fn tier_word(t: CommandTier) -> &'static str {
    match t {
        CommandTier::User => "user",
        CommandTier::Dev => "dev",
    }
}

/// The workspace a command runs for: the current directory, as for every
/// other verb that takes no `--workspace`.
#[must_use]
pub fn workspace() -> PathBuf {
    let ws = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    ws.canonicalize().unwrap_or(ws)
}

/// Dispatch `argv` (without the program name) to a command package, when one
/// announces its verb. Returns only when no package does, so the caller can
/// print its own unknown-verb error; otherwise it executes the command (and on
/// Unix never returns) or exits with the command's status or [`REFUSED`].
pub fn dispatch(argv: &[OsString]) {
    let Some(req) = Request::from_argv(argv) else {
        return;
    };
    let ws = workspace();
    let root = crate::orchestration::nano_ros_root::resolve(None, &ws);
    match lookup(&req, &ws, root.as_deref()) {
        Lookup::NotACommand => {}
        Lookup::Refused(m) => {
            eprintln!("nros: {m}");
            std::process::exit(REFUSED);
        }
        Lookup::Run(inv) => {
            for w in &inv.warnings {
                eprintln!("warning: {w}");
            }
            let code = execute(&inv);
            std::process::exit(code);
        }
    }
}

/// Execute `inv`: `exec` on Unix, so the command's status (and a signal) is
/// the process's own; elsewhere spawn, wait, and return its code.
fn execute(inv: &Invocation) -> i32 {
    let mut cmd = std::process::Command::new(&inv.exe);
    cmd.args(&inv.args);
    for (k, v) in &inv.env {
        cmd.env(k, v);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = cmd.exec();
        eprintln!("nros: exec {}: {err}", inv.exe.display());
        REFUSED
    }
    #[cfg(not(unix))]
    {
        match cmd.status() {
            Ok(s) => s.code().unwrap_or(1),
            Err(e) => {
                eprintln!("nros: run {}: {e}", inv.exe.display());
                REFUSED
            }
        }
    }
}

/// One extension verb as `nros help --all` lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    /// `nros router`, `nros dev fixture-hello`.
    pub spelling: String,
    /// The descriptor's `summary`.
    pub summary: String,
    /// The providing package.
    pub package: String,
}

/// Every command the workspace `ws` can reach, and every one refused, by
/// verb name. The listing resolves each name the way dispatch would, so the
/// two cannot disagree about which package answers.
#[must_use]
pub fn discover(ws: &Path) -> (Vec<Listed>, Vec<String>) {
    let Ok((scan, policy)) = scan_and_policy(ws) else {
        return (Vec::new(), Vec::new());
    };
    let mut names: Vec<&str> = scan
        .providers
        .iter()
        .flat_map(|p| p.provides.iter())
        .filter(|v| v.kind == COMMAND_KIND)
        .map(|v| v.name.as_str())
        .collect();
    names.sort_unstable();
    names.dedup();
    let root = crate::builtin_verbs::tree();
    let (mut listed, mut refused) = (Vec::new(), Vec::new());
    for name in names {
        match resolve_command_with(&scan, name, &policy) {
            Ok(c) => match taken_refusal(&c, &root) {
                Some(r) => refused.push(r),
                None => listed.push(Listed {
                    spelling: spelling(c.descriptor.tier, &c.name),
                    summary: c.descriptor.summary.clone(),
                    package: c.package.clone(),
                }),
            },
            Err(e) => refused.push(format!("command `{name}`: {e}")),
        }
    }
    (listed, refused)
}

/// `nros help --all`'s extension section; empty when there is none.
#[must_use]
pub fn render_listing(listed: &[Listed]) -> String {
    if listed.is_empty() {
        return String::new();
    }
    let width = listed.iter().map(|l| l.spelling.len()).max().unwrap_or(0);
    let mut out = String::from("Extension commands (from packages):\n");
    for l in listed {
        out.push_str(&format!(
            "  {:<width$}  {} (package {})\n",
            l.spelling, l.summary, l.package
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(v: &[&str]) -> Vec<OsString> {
        v.iter().map(OsString::from).collect()
    }

    #[test]
    fn the_verb_and_its_unchanged_args_are_read_from_argv() {
        let r = Request::from_argv(&os(&["router", "config", "--x", "y"])).unwrap();
        assert_eq!(r.tier, CommandTier::User);
        assert_eq!(r.verb, "router");
        assert_eq!(r.args, os(&["config", "--x", "y"]));
        let r = Request::from_argv(&os(&["dev", "hello", "-a"])).unwrap();
        assert_eq!(r.tier, CommandTier::Dev);
        assert_eq!(r.verb, "hello");
        assert_eq!(r.args, os(&["-a"]));
        assert_eq!(r.spelling(), "nros dev hello");
        assert!(Request::from_argv(&os(&["--version"])).is_none());
        assert!(Request::from_argv(&os(&["dev"])).is_none());
        assert!(Request::from_argv(&os(&["dev", "--help"])).is_none());
    }

    #[test]
    fn only_a_workspace_is_walked() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(!is_workspace(tmp.path()), "an empty directory is not one");
        std::fs::create_dir(tmp.path().join("src")).unwrap();
        assert!(is_workspace(tmp.path()));
        let leaf = tempfile::tempdir().unwrap();
        std::fs::write(leaf.path().join("system.toml"), "").unwrap();
        assert!(is_workspace(leaf.path()));
    }

    #[test]
    fn the_listing_names_summary_and_package() {
        let text = render_listing(&[Listed {
            spelling: "nros router".into(),
            summary: "Print a router config".into(),
            package: "nros_zenoh_tools".into(),
        }]);
        assert!(text.starts_with("Extension commands"), "{text}");
        assert!(
            text.contains("nros router  Print a router config (package nros_zenoh_tools)"),
            "{text}"
        );
        assert!(render_listing(&[]).is_empty());
    }

    #[test]
    fn a_built_in_or_reserved_name_is_taken_for_its_tier() {
        let root = crate::builtin_verbs::tree();
        let user = crate::builtin_verbs::taken(&root, false);
        assert!(user.contains("build") && user.contains("run") && user.contains("help"));
        assert!(!user.contains("rmw"), "`rmw` is a `nros dev` verb only");
        let dev = crate::builtin_verbs::taken(&root, true);
        assert!(dev.contains("rmw") && dev.contains("build"));
    }
}
