//! The RunPlan (RFC-0118 D3) and its JSON form.
//!
//! A RunPlan is the resolved, serialisable answer to "how is this image run".
//! nros resolves it from the customisation ladder (RFC-0118 D5) and hands it
//! to a runner as `--plan <plan.json>`; the runner resolves nothing on its
//! own. `nros build` prints a plan's command as its next step, and the test
//! harness writes one and calls `nros run --plan`, so all three consumers go
//! through these types.
//!
//! ```
//! use nros_extension::plan::RunPlan;
//!
//! let plan = RunPlan::from_json(
//!     r#"{
//!     "protocol": 1,
//!     "image": "talker",
//!     "artifacts": { "elf": "/ws/build/talker.elf" },
//!     "verb": "run",
//!     "runner": { "package": "nros_runner_qemu", "name": "qemu" },
//!     "target": { "emulator": "qemu:mps2-an385" },
//!     "console": { "kind": "semihosting" },
//!     "link": { "kind": "tcp", "locator": "tcp/10.0.2.2:7447" },
//!     "args": { "nic": "user,model=lan9118" }
//! }"#,
//! )
//! .unwrap();
//! assert_eq!(
//!     plan.expand("-kernel {elf} -nic {nic}").unwrap(),
//!     "-kernel /ws/build/talker.elf -nic user,model=lan9118"
//! );
//! ```

use std::{
    collections::BTreeMap,
    fmt,
    path::{Path, PathBuf},
    str::FromStr,
};

use serde::{Deserialize, Serialize};

use crate::{
    protocol::{PROTOCOL, SUPPORTED},
    usb::UsbId,
};

/// What a runner is asked to do. `nros monitor` asks a runner for `console`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verb {
    /// Launch a host process or an emulator and stream its console.
    Run,
    /// Program a real board.
    Flash,
    /// Open the console only.
    Console,
}

impl Verb {
    /// Every verb, in the order the contract lists them.
    pub const ALL: [Verb; 3] = [Verb::Run, Verb::Flash, Verb::Console];

    /// The spelling on the command line and in JSON.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Verb::Run => "run",
            Verb::Flash => "flash",
            Verb::Console => "console",
        }
    }

    /// Parse the command-line spelling.
    #[must_use]
    pub fn parse(s: &str) -> Option<Verb> {
        Verb::ALL.into_iter().find(|v| v.as_str() == s)
    }
}

impl fmt::Display for Verb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Which runner package executes the plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerRef {
    /// The package that announces the runner, for example the QEMU runner
    /// package of RFC-0119 D8.
    pub package: String,
    /// The announced runner name (`qemu`).
    pub name: String,
}

/// The launcher or flasher (RFC-0118 D1). Exactly one of the three is set.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    /// An emulator in RFC-0114's vocabulary: `qemu:<machine>`, `fvp:<model>`,
    /// `renode:<platform>`, `wokwi:<board>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emulator: Option<String>,
    /// The image is a host process.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub host: bool,
    /// A flasher: a framework runner (`west`) or a tool (`espflash`, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flasher: Option<String>,
}

/// How the target's console is observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConsoleKind {
    /// The process's own stdout/stdin.
    Stdio,
    /// A serial device.
    Serial,
    /// SEGGER RTT through a probe.
    Rtt,
    /// Arm semihosting.
    Semihosting,
}

/// The console (RFC-0118 D1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Console {
    /// How it is observed.
    pub kind: ConsoleKind,
    /// The serial port, once nros has resolved one (RFC-0118 D6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<String>,
    /// The serial baud rate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baud: Option<u32>,
}

/// The transport kind of the link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkKind {
    /// A TCP locator.
    Tcp,
    /// A UDP locator.
    Udp,
    /// A serial endpoint.
    Serial,
}

/// The RMW link, as RFC-0109 resolves it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    /// The transport kind.
    pub kind: LinkKind,
    /// The locator (`tcp/10.0.2.2:7447`) or serial endpoint.
    pub locator: String,
}

/// A `wait_for` event (RFC-0118 D7), written `<kind>:<value>`.
///
/// The same four kinds, spellings and errors as
/// `nros_orchestration_ir::target_exec::WaitFor`, which parses the board and
/// image tables a plan is resolved from; a test in `nros-cli-core` holds the
/// two JSON forms equal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum WaitFor {
    /// `usb:<vid>:<pid>`: a USB device enumerates.
    Usb(UsbId),
    /// `port:<path>`: a serial port appears.
    Port(String),
    /// `volume:<label>`: a mass-storage volume (a UF2 drive) mounts.
    Volume(String),
    /// `line:<text>`: the console prints a line holding `<text>`.
    Line(String),
}

impl FromStr for WaitFor {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let (kind, arg) = s.split_once(':').ok_or_else(|| {
            format!(
                "`wait_for = {s:?}` must read `<kind>:<value>`, kind one of usb, port, volume, line"
            )
        })?;
        let nonempty = |what: &str| {
            if arg.is_empty() {
                Err(format!("`wait_for = {s:?}`: `{kind}:` names no {what}"))
            } else {
                Ok(arg.to_string())
            }
        };
        Ok(match kind {
            "usb" => WaitFor::Usb(
                arg.parse()
                    .map_err(|e| format!("`wait_for = {s:?}`: {e}"))?,
            ),
            "port" => WaitFor::Port(nonempty("path")?),
            "volume" => WaitFor::Volume(nonempty("label")?),
            "line" => WaitFor::Line(nonempty("text")?),
            other => {
                return Err(format!(
                    "`wait_for = {s:?}`: unknown event kind `{other}` — one of usb, port, \
                     volume, line (RFC-0118 D7)"
                ));
            }
        })
    }
}

impl TryFrom<String> for WaitFor {
    type Error = String;

    fn try_from(s: String) -> Result<Self, String> {
        s.parse()
    }
}

impl fmt::Display for WaitFor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WaitFor::Usb(u) => write!(f, "usb:{u}"),
            WaitFor::Port(p) => write!(f, "port:{p}"),
            WaitFor::Volume(v) => write!(f, "volume:{v}"),
            WaitFor::Line(t) => write!(f, "line:{t}"),
        }
    }
}

impl From<WaitFor> for String {
    fn from(w: WaitFor) -> String {
        w.to_string()
    }
}

/// One `prepare` step (RFC-0118 D7): exactly one of `say`, `wait_for` and
/// `run`, except that `say` and `wait_for` may share a step (print, then
/// wait). Serialised as a table with those keys, e.g. `{"say": "…"}`,
/// `{"say": "…", "wait_for": "usb:0483:df11"}` or `{"run": "./relay.sh 3"}`.
///
/// The shape of `nros_orchestration_ir::target_exec::PrepareStep`, so a step
/// read from a board or image table reaches the runner unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawStep", into = "RawStep")]
pub enum PrepareStep {
    /// Print an instruction.
    Say(String),
    /// Block until an event.
    WaitFor(WaitFor),
    /// Print an instruction, then block until an event.
    SayThenWait {
        /// The instruction.
        say: String,
        /// The event.
        wait_for: WaitFor,
    },
    /// Run a host command (a relay, a power switch). nros runs it, not the
    /// runner: `prepare` is one of the things nros owns (RFC-0119 D6).
    Run(String),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStep {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    say: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    wait_for: Option<WaitFor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    run: Option<String>,
}

impl TryFrom<RawStep> for PrepareStep {
    type Error = String;

    fn try_from(r: RawStep) -> Result<Self, String> {
        match (r.say, r.wait_for, r.run) {
            (Some(say), None, None) => Ok(PrepareStep::Say(say)),
            (None, Some(w), None) => Ok(PrepareStep::WaitFor(w)),
            (Some(say), Some(wait_for), None) => Ok(PrepareStep::SayThenWait { say, wait_for }),
            (None, None, Some(run)) if !run.trim().is_empty() => Ok(PrepareStep::Run(run)),
            (None, None, Some(_)) => Err("a `prepare` step's `run` is empty".into()),
            (None, None, None) => Err(
                "a `prepare` step states nothing — it is one of `say`, `wait_for`, `run` \
                 (or `say` with `wait_for`)"
                    .into(),
            ),
            _ => Err(
                "a `prepare` step is one of `say`, `wait_for`, `run`; only `say` and \
                 `wait_for` may share a step (RFC-0118 D7)"
                    .into(),
            ),
        }
    }
}

impl From<PrepareStep> for RawStep {
    fn from(s: PrepareStep) -> Self {
        match s {
            PrepareStep::Say(say) => RawStep {
                say: Some(say),
                ..RawStep::default()
            },
            PrepareStep::WaitFor(w) => RawStep {
                wait_for: Some(w),
                ..RawStep::default()
            },
            PrepareStep::SayThenWait { say, wait_for } => RawStep {
                say: Some(say),
                wait_for: Some(wait_for),
                run: None,
            },
            PrepareStep::Run(run) => RawStep {
                run: Some(run),
                ..RawStep::default()
            },
        }
    }
}

/// The resolved plan.
///
/// Unknown JSON fields are ignored rather than refused, so a field a newer
/// nros adds within the same protocol version does not break an older
/// runner. A change a runner must understand bumps [`PROTOCOL`] instead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunPlan {
    /// The protocol version this plan is written in.
    #[serde(default = "default_protocol")]
    pub protocol: u32,
    /// The image name.
    pub image: String,
    /// Build outputs by kind: `elf`, `bin`, `hex`, `uf2`.
    #[serde(default)]
    pub artifacts: BTreeMap<String, PathBuf>,
    /// The image's build directory, for `{build_dir}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_dir: Option<PathBuf>,
    /// What is asked.
    pub verb: Verb,
    /// Who executes it.
    pub runner: RunnerRef,
    /// The launcher or flasher.
    pub target: Target,
    /// The console.
    pub console: Console,
    /// The RMW link, when the image has a networked transport.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<Link>,
    /// Method-specific values (`nic`, `probe`, ...).
    #[serde(default)]
    pub args: BTreeMap<String, String>,
    /// Host tools resolved through the SDK store, for `{tool:<name>}`. Never
    /// resolved from `PATH`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tools: BTreeMap<String, PathBuf>,
    /// Arguments given after `--` on the nros command line, unchanged.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub passthrough: Vec<String>,
    /// Human steps, in order.
    #[serde(default)]
    pub prepare: Vec<PrepareStep>,
    /// Which ladder level supplied each field (`"args.nic" = "image"`), for
    /// `--why`.
    #[serde(default)]
    pub provenance: BTreeMap<String, String>,
}

fn default_protocol() -> u32 {
    PROTOCOL
}

/// A plan could not be read, or is not one this crate accepts.
#[derive(Debug)]
pub enum PlanError {
    /// Reading the file failed.
    Io(PathBuf, std::io::Error),
    /// The JSON does not describe a plan.
    Json(serde_json::Error),
    /// The plan is written in a protocol this crate does not speak.
    Protocol(u32),
    /// The target names no launcher or flasher, or more than one.
    Target(String),
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlanError::Io(p, e) => write!(f, "reading plan {}: {e}", p.display()),
            PlanError::Json(e) => write!(f, "the plan is not valid RunPlan JSON: {e}"),
            PlanError::Protocol(v) => write!(
                f,
                "the plan is written in protocol {v}, and this runner speaks {:?}",
                SUPPORTED
            ),
            PlanError::Target(why) => write!(f, "the plan's target {why}"),
        }
    }
}

impl std::error::Error for PlanError {}

impl RunPlan {
    /// Parse and validate a plan.
    ///
    /// # Errors
    ///
    /// A [`PlanError`] for malformed JSON, an unsupported protocol, or a
    /// target that is not exactly one launcher or flasher.
    pub fn from_json(text: &str) -> Result<RunPlan, PlanError> {
        let plan: RunPlan = serde_json::from_str(text).map_err(PlanError::Json)?;
        plan.validate()?;
        Ok(plan)
    }

    /// Read and validate a plan file.
    ///
    /// # Errors
    ///
    /// As [`RunPlan::from_json`], plus an I/O failure.
    pub fn read(path: &Path) -> Result<RunPlan, PlanError> {
        let text =
            std::fs::read_to_string(path).map_err(|e| PlanError::Io(path.to_path_buf(), e))?;
        RunPlan::from_json(&text)
    }

    /// The plan as pretty JSON, the form written to `--plan` files.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a RunPlan always serialises")
    }

    /// Write the plan as JSON.
    ///
    /// # Errors
    ///
    /// An I/O failure.
    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        std::fs::write(path, self.to_json() + "\n")
    }

    /// Check what serde alone cannot.
    ///
    /// # Errors
    ///
    /// See [`RunPlan::from_json`].
    pub fn validate(&self) -> Result<(), PlanError> {
        if !SUPPORTED.contains(&self.protocol) {
            return Err(PlanError::Protocol(self.protocol));
        }
        let set = usize::from(self.target.emulator.is_some())
            + usize::from(self.target.host)
            + usize::from(self.target.flasher.is_some());
        match set {
            1 => Ok(()),
            0 => Err(PlanError::Target(
                "names no launcher or flasher; set one of emulator, host or flasher".into(),
            )),
            _ => Err(PlanError::Target(
                "names more than one of emulator, host and flasher; a target has exactly one \
                 (RFC-0118 D1)"
                    .into(),
            )),
        }
    }

    /// The value of one placeholder, or `None` when the plan does not carry
    /// it. `name` is the text between the braces.
    #[must_use]
    pub fn placeholder(&self, name: &str) -> Option<String> {
        let ph = Placeholder::parse(name)?;
        let path = |p: &PathBuf| p.display().to_string();
        match ph {
            Placeholder::Elf => self.artifacts.get("elf").map(path),
            Placeholder::Bin => self.artifacts.get("bin").map(path),
            Placeholder::Hex => self.artifacts.get("hex").map(path),
            Placeholder::Uf2 => self.artifacts.get("uf2").map(path),
            Placeholder::BuildDir => self.build_dir.as_ref().map(path),
            Placeholder::Port => self.console.port.clone(),
            Placeholder::Baud => self.console.baud.map(|b| b.to_string()),
            Placeholder::Probe => self.args.get("probe").cloned(),
            Placeholder::Nic => self.args.get("nic").cloned(),
            Placeholder::Tool(t) => self.tools.get(&t).map(path),
        }
    }

    /// Expand a command template (RFC-0118 D5). `{{` and `}}` are literal
    /// braces.
    ///
    /// # Errors
    ///
    /// An [`ExpandError`] for an unknown placeholder (refused, never passed
    /// through), a known one this plan does not carry, or an unclosed brace.
    pub fn expand(&self, template: &str) -> Result<String, ExpandError> {
        let mut out = String::with_capacity(template.len());
        let mut rest = template;
        while let Some(i) = rest.find(['{', '}']) {
            out.push_str(&rest[..i]);
            let tail = &rest[i..];
            if let Some(t) = tail.strip_prefix("{{") {
                out.push('{');
                rest = t;
            } else if let Some(t) = tail.strip_prefix("}}") {
                out.push('}');
                rest = t;
            } else if tail.starts_with('}') {
                return Err(ExpandError::Unbalanced(template.to_string()));
            } else {
                let end = tail
                    .find('}')
                    .ok_or_else(|| ExpandError::Unbalanced(template.to_string()))?;
                let name = &tail[1..end];
                if Placeholder::parse(name).is_none() {
                    return Err(ExpandError::Unknown(name.to_string()));
                }
                let value = self
                    .placeholder(name)
                    .ok_or_else(|| ExpandError::Missing(name.to_string()))?;
                out.push_str(&value);
                rest = &tail[end + 1..];
            }
        }
        out.push_str(rest);
        Ok(out)
    }
}

/// A command-template placeholder (RFC-0118 D5). Placeholders come only from
/// the plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Placeholder {
    /// `{elf}`
    Elf,
    /// `{bin}`
    Bin,
    /// `{hex}`
    Hex,
    /// `{uf2}`
    Uf2,
    /// `{build_dir}`
    BuildDir,
    /// `{port}`
    Port,
    /// `{baud}`
    Baud,
    /// `{probe}`
    Probe,
    /// `{nic}`
    Nic,
    /// `{tool:<name>}`, a host tool from the SDK store.
    Tool(String),
}

/// The fixed placeholder names, as a runner's `needs` list spells them.
pub const PLACEHOLDERS: &[&str] = &[
    "elf",
    "bin",
    "hex",
    "uf2",
    "build_dir",
    "port",
    "baud",
    "probe",
    "nic",
];

impl Placeholder {
    /// Parse the text between the braces. `None` for an unknown name.
    #[must_use]
    pub fn parse(name: &str) -> Option<Placeholder> {
        Some(match name {
            "elf" => Placeholder::Elf,
            "bin" => Placeholder::Bin,
            "hex" => Placeholder::Hex,
            "uf2" => Placeholder::Uf2,
            "build_dir" => Placeholder::BuildDir,
            "port" => Placeholder::Port,
            "baud" => Placeholder::Baud,
            "probe" => Placeholder::Probe,
            "nic" => Placeholder::Nic,
            _ => {
                let tool = name.strip_prefix("tool:")?;
                if tool.is_empty() {
                    return None;
                }
                Placeholder::Tool(tool.to_string())
            }
        })
    }
}

/// A template could not be expanded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpandError {
    /// Not a placeholder RFC-0118 D5 defines.
    Unknown(String),
    /// A defined placeholder this plan carries no value for.
    Missing(String),
    /// A `{` with no closing `}`, or a lone `}`.
    Unbalanced(String),
}

impl fmt::Display for ExpandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExpandError::Unknown(n) => write!(
                f,
                "unknown placeholder {{{n}}}; known: {}, tool:<name>",
                PLACEHOLDERS
                    .iter()
                    .map(|p| format!("{{{p}}}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            ExpandError::Missing(n) => {
                write!(f, "placeholder {{{n}}} has no value in this plan")
            }
            ExpandError::Unbalanced(t) => write!(f, "unbalanced braces in template {t:?}"),
        }
    }
}

impl std::error::Error for ExpandError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> RunPlan {
        RunPlan {
            protocol: PROTOCOL,
            image: "talker".into(),
            artifacts: BTreeMap::from([("elf".into(), PathBuf::from("/b/talker.elf"))]),
            build_dir: Some(PathBuf::from("/b")),
            verb: Verb::Flash,
            runner: RunnerRef {
                package: "nros_runner_command".into(),
                name: "command".into(),
            },
            target: Target {
                flasher: Some("pyocd".into()),
                ..Target::default()
            },
            console: Console {
                kind: ConsoleKind::Serial,
                port: Some("/dev/ttyACM0".into()),
                baud: Some(115_200),
            },
            link: None,
            args: BTreeMap::new(),
            tools: BTreeMap::from([("pyocd".into(), PathBuf::from("/sdk/pyocd"))]),
            passthrough: vec!["--verbose".into()],
            prepare: vec![
                PrepareStep::Say("Set BOOT0 to 1".into()),
                PrepareStep::WaitFor("usb:0483:df11".parse().unwrap()),
                PrepareStep::SayThenWait {
                    say: "Press RESET".into(),
                    wait_for: "line:READY".parse().unwrap(),
                },
                PrepareStep::Run("./relay.sh 3".into()),
            ],
            provenance: BTreeMap::from([("target.flasher".into(), "image".into())]),
        }
    }

    #[test]
    fn json_round_trips() {
        let p = sample();
        let back = RunPlan::from_json(&p.to_json()).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn prepare_steps_use_the_rfc_spelling() {
        let json = sample().to_json();
        assert!(json.contains(r#""say": "Set BOOT0 to 1""#), "{json}");
        assert!(json.contains(r#""wait_for": "usb:0483:df11""#), "{json}");
        assert!(json.contains(r#""run": "./relay.sh 3""#), "{json}");
    }

    /// RFC-0118 D7 — the three step kinds and the `say` + `wait_for` pair,
    /// with the refusals the board and image tables give.
    #[test]
    fn prepare_steps_round_trip_and_refuse_bad_shapes() {
        let step = |j: &str| serde_json::from_str::<PrepareStep>(j);
        assert_eq!(
            step(r#"{"run":"./relay.sh 3"}"#).unwrap(),
            PrepareStep::Run("./relay.sh 3".into())
        );
        assert_eq!(
            step(r#"{"say":"x","wait_for":"volume:RPI-RP2"}"#).unwrap(),
            PrepareStep::SayThenWait {
                say: "x".into(),
                wait_for: WaitFor::Volume("RPI-RP2".into())
            }
        );
        assert_eq!(
            step(r#"{"wait_for":"usb:0483:DF11"}"#).unwrap(),
            PrepareStep::WaitFor(WaitFor::Usb("0483:df11".parse().unwrap()))
        );
        for bad in [
            r#"{}"#,
            r#"{"run":"  "}"#,
            r#"{"say":"x","run":"y"}"#,
            r#"{"wait_for":"usb:0483"}"#,
            r#"{"wait_for":"gpio:3"}"#,
            r#"{"wait_for":"port:"}"#,
            r#"{"say":"x","beep":true}"#,
        ] {
            assert!(step(bad).is_err(), "{bad} must be refused");
        }
        for s in [
            PrepareStep::Run("r".into()),
            PrepareStep::WaitFor(WaitFor::Port("/dev/ttyACM0".into())),
            PrepareStep::WaitFor(WaitFor::Line("ready: ok".into())),
        ] {
            let j = serde_json::to_string(&s).unwrap();
            assert_eq!(step(&j).unwrap(), s, "{j}");
        }
    }

    #[test]
    fn a_missing_protocol_reads_as_the_current_one() {
        let mut v: serde_json::Value = serde_json::from_str(&sample().to_json()).unwrap();
        v.as_object_mut().unwrap().remove("protocol");
        let p = RunPlan::from_json(&v.to_string()).unwrap();
        assert_eq!(p.protocol, PROTOCOL);
    }

    #[test]
    fn an_unsupported_protocol_is_refused() {
        let mut p = sample();
        p.protocol = 99;
        assert!(matches!(
            RunPlan::from_json(&p.to_json()),
            Err(PlanError::Protocol(99))
        ));
    }

    #[test]
    fn a_target_has_exactly_one_launcher_or_flasher() {
        let mut p = sample();
        p.target.host = true;
        assert!(matches!(p.validate(), Err(PlanError::Target(_))));
        p.target = Target::default();
        assert!(matches!(p.validate(), Err(PlanError::Target(_))));
    }

    #[test]
    fn templates_expand_from_the_plan_only() {
        let p = sample();
        assert_eq!(
            p.expand("{tool:pyocd} flash {elf} --port {port} -b {baud} {{x}}")
                .unwrap(),
            "/sdk/pyocd flash /b/talker.elf --port /dev/ttyACM0 -b 115200 {x}"
        );
        assert_eq!(p.expand("{home}"), Err(ExpandError::Unknown("home".into())));
        assert_eq!(p.expand("{hex}"), Err(ExpandError::Missing("hex".into())));
        assert_eq!(
            p.expand("{tool:openocd}"),
            Err(ExpandError::Missing("tool:openocd".into()))
        );
        assert!(matches!(p.expand("{elf"), Err(ExpandError::Unbalanced(_))));
        assert!(matches!(p.expand("x}"), Err(ExpandError::Unbalanced(_))));
        assert_eq!(
            p.expand("{tool:}"),
            Err(ExpandError::Unknown("tool:".into()))
        );
    }

    #[test]
    fn every_fixed_placeholder_parses() {
        for name in PLACEHOLDERS {
            assert!(Placeholder::parse(name).is_some(), "{name}");
        }
    }
}
