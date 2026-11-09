//! RFC-0118 D4/D5/D7 (phase-529 W1, schema half) — the flash and console
//! tables a board descriptor and an image share, the `prepare` step syntax,
//! and the command-template placeholders.
//!
//! One definition for both levels of the customisation ladder: the board's
//! `[board.flash]` / `[board.console]` (`nros-board.toml`, read by
//! `nros-cli-core`'s board catalog) and the image's `[image.<id>.flash]` /
//! `[image.<id>.console]` (`system.toml`, read by both the leaf road here and
//! the workspace road's `ImageBlock`). A board table must name HOW (a `method`
//! or a `command`); an image table states only what it overrides.
//!
//! Everything is parsed into typed values and checked when the file is read:
//! an unknown key, an unknown `wait_for` event kind, a step with two actions
//! or an unknown `{placeholder}` is refused naming the table. Nothing here
//! EXECUTES a step or a template — that needs phase-528 W1's RunPlan.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

/// D5's closed placeholder list (plus `{tool:<name>}`).
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

/// A command template (RFC-0118 D5): a host command line whose `{…}`
/// placeholders the plan fills. Checked at parse time: every placeholder is
/// one of [`PLACEHOLDERS`] or `{tool:<name>}`, and every `{` closes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CommandTemplate(String);

impl CommandTemplate {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The placeholders the template names, in order (`tool:<name>` whole).
    #[must_use]
    pub fn placeholders(&self) -> Vec<&str> {
        scan(&self.0).unwrap_or_default()
    }
}

/// The placeholder names of `s`, with `{{` and `}}` read as literal braces
/// (the escape `nros_extension::plan::RunPlan::expand` expands), or the first
/// shape error.
fn scan(s: &str) -> Result<Vec<&str>, String> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(i) = rest.find(['{', '}']) {
        let tail = &rest[i..];
        if let Some(t) = tail.strip_prefix("{{").or_else(|| tail.strip_prefix("}}")) {
            rest = t;
        } else if tail.starts_with('}') {
            return Err(format!("`command = {s:?}`: a `}}` with no `{{` before it"));
        } else {
            let Some(j) = tail.find('}') else {
                return Err(format!("`command = {s:?}`: a `{{` that never closes"));
            };
            out.push(&tail[1..j]);
            rest = &tail[j + 1..];
        }
    }
    Ok(out)
}

impl TryFrom<String> for CommandTemplate {
    type Error = String;
    fn try_from(s: String) -> Result<Self, String> {
        if s.trim().is_empty() {
            return Err("`command` is empty".into());
        }
        for name in scan(&s)? {
            let ok =
                PLACEHOLDERS.contains(&name) || name.strip_prefix("tool:").is_some_and(is_word);
            if !ok {
                return Err(format!(
                    "`command = {s:?}`: unknown placeholder `{{{name}}}` — a template \
                     names only {}, or `{{tool:<name>}}`; `{{{{` and `}}}}` are literal \
                     braces (RFC-0118 D5)",
                    PLACEHOLDERS
                        .iter()
                        .map(|p| format!("`{{{p}}}`"))
                        .collect::<Vec<_>>()
                        .join(" ")
                ));
            }
        }
        Ok(Self(s))
    }
}

impl From<CommandTemplate> for String {
    fn from(c: CommandTemplate) -> String {
        c.0
    }
}

/// A lowercase word (`[a-z0-9][a-z0-9_-]*`): a method or tool name.
fn is_word(s: &str) -> bool {
    let mut c = s.chars();
    c.next()
        .is_some_and(|f| f.is_ascii_lowercase() || f.is_ascii_digit())
        && c.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_')
}

/// A USB `<vid>:<pid>`, both four hex digits (`0483:374b`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct UsbId {
    pub vid: u16,
    pub pid: u16,
}

impl FromStr for UsbId {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        let hex = |h: &str| {
            (h.len() == 4 && h.chars().all(|c| c.is_ascii_hexdigit()))
                .then(|| u16::from_str_radix(h, 16).ok())
                .flatten()
        };
        match s.split_once(':') {
            Some((v, p)) => match (hex(v), hex(p)) {
                (Some(vid), Some(pid)) => Ok(Self { vid, pid }),
                _ => Err(format!(
                    "`{s}` must read `<vid>:<pid>`, four hex digits each"
                )),
            },
            None => Err(format!(
                "`{s}` must read `<vid>:<pid>`, four hex digits each"
            )),
        }
    }
}

impl TryFrom<String> for UsbId {
    type Error = String;
    fn try_from(s: String) -> Result<Self, String> {
        s.parse()
    }
}

impl fmt::Display for UsbId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04x}:{:04x}", self.vid, self.pid)
    }
}

impl From<UsbId> for String {
    fn from(u: UsbId) -> String {
        u.to_string()
    }
}

/// A `wait_for` event (RFC-0118 D7).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum WaitFor {
    /// `usb:<vid>:<pid>` — a USB device enumerates.
    Usb(UsbId),
    /// `port:<path>` — a serial port (a path) appears.
    Port(String),
    /// `volume:<label>` — a mass-storage volume (a UF2 drive) mounts.
    Volume(String),
    /// `line:<text>` — the console prints a line holding `<text>`.
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
            "usb" => Self::Usb(
                arg.parse()
                    .map_err(|e| format!("`wait_for = {s:?}`: {e}"))?,
            ),
            "port" => Self::Port(nonempty("path")?),
            "volume" => Self::Volume(nonempty("label")?),
            "line" => Self::Line(nonempty("text")?),
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
            Self::Usb(u) => write!(f, "usb:{u}"),
            Self::Port(p) => write!(f, "port:{p}"),
            Self::Volume(v) => write!(f, "volume:{v}"),
            Self::Line(t) => write!(f, "line:{t}"),
        }
    }
}

impl From<WaitFor> for String {
    fn from(w: WaitFor) -> String {
        w.to_string()
    }
}

/// One `prepare` step (RFC-0118 D7): exactly one of `say`, `wait_for`, `run`,
/// except that `say` and `wait_for` may pair in one table (print, then wait).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawStep", into = "RawStep")]
pub enum PrepareStep {
    Say(String),
    WaitFor(WaitFor),
    /// `say` then `wait_for`, one table.
    SayThenWait {
        say: String,
        wait_for: WaitFor,
    },
    /// A host command (a relay, a power switch).
    Run(String),
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
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
            (Some(say), None, None) => Ok(Self::Say(say)),
            (None, Some(w), None) => Ok(Self::WaitFor(w)),
            (Some(say), Some(wait_for), None) => Ok(Self::SayThenWait { say, wait_for }),
            (None, None, Some(run)) if !run.trim().is_empty() => Ok(Self::Run(run)),
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
            PrepareStep::Say(say) => Self {
                say: Some(say),
                ..Self::default()
            },
            PrepareStep::WaitFor(w) => Self {
                wait_for: Some(w),
                ..Self::default()
            },
            PrepareStep::SayThenWait { say, wait_for } => Self {
                say: Some(say),
                wait_for: Some(wait_for),
                run: None,
            },
            PrepareStep::Run(run) => Self {
                run: Some(run),
                ..Self::default()
            },
        }
    }
}

/// `[board.flash]` / `[image.<id>.flash]` (RFC-0118 D4/D5).
///
/// `method` names the flasher runner (`west`, `espflash`, `probe-rs`,
/// `openocd`, `picotool`, `uf2`, `dfu-util`, …) by its word; `command`
/// replaces the method with a template (D5) and is exclusive with it. The
/// method word is checked for SHAPE, not against a closed list: runners are
/// packages (RFC-0119), so which words exist is a question for the installed
/// runner set when a plan resolves, not for the file format.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlashTable {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<CommandTemplate>,
    /// The part the flasher programs (`STM32F767ZITx` for probe-rs, `esp32c3`
    /// for espflash), in the method's own spelling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chip: Option<String>,
    /// Steps run before the flash, in order (D7).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prepare: Vec<PrepareStep>,
}

impl FlashTable {
    /// The shape rules serde cannot express. `board` requires a method or a
    /// command (an image table may state any subset). `at` names the table.
    pub fn validate(&self, at: &str, board: bool) -> Result<(), String> {
        match (&self.method, &self.command) {
            (Some(_), Some(_)) => {
                return Err(format!(
                    "{at}: `method` and `command` are alternatives — a command template \
                     REPLACES the method (RFC-0118 D5)"
                ));
            }
            (None, None) if board => {
                return Err(format!(
                    "{at}: states neither `method` (the flasher, e.g. \"probe-rs\") nor a \
                     `command` template"
                ));
            }
            _ => {}
        }
        if let Some(m) = &self.method
            && !is_word(m)
        {
            return Err(format!(
                "{at}: `method = {m:?}` must be a lowercase runner word \
                 (`[a-z0-9][a-z0-9_-]*`, e.g. \"probe-rs\")"
            ));
        }
        Ok(())
    }
}

impl FlashTable {
    /// `self` (the more specific level) over `base`, key by key: a key this
    /// table states wins, every other key is `base`'s. `method` and
    /// `command` are one choice — stating either replaces both of `base`'s.
    #[must_use]
    pub fn over(&self, base: &FlashTable) -> FlashTable {
        let how = self.method.is_some() || self.command.is_some();
        FlashTable {
            method: if how {
                self.method.clone()
            } else {
                base.method.clone()
            },
            command: if how {
                self.command.clone()
            } else {
                base.command.clone()
            },
            chip: self.chip.clone().or_else(|| base.chip.clone()),
            prepare: if self.prepare.is_empty() {
                base.prepare.clone()
            } else {
                self.prepare.clone()
            },
        }
    }
}

/// `[board.console] kind` / `[image.<id>.console] kind` (RFC-0118 D1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConsoleKind {
    Stdio,
    Serial,
    Rtt,
    Semihosting,
}

/// `[board.console]` / `[image.<id>.console]` (RFC-0118 D4/D5). `usb` and
/// `baud` describe a serial console; `command` attaches it through a template
/// (`picocom -b {baud} {port}`) instead of the console runner.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsoleTable {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<ConsoleKind>,
    /// The USB serial adapter the console appears on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usb: Option<UsbId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baud: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<CommandTemplate>,
}

impl ConsoleTable {
    /// `board` requires `kind`. `usb` and `baud` need a serial console — the
    /// table's own `kind`, else (for an image) the board's `base`.
    pub fn validate(&self, at: &str, board: bool) -> Result<(), String> {
        if board && self.kind.is_none() {
            return Err(format!(
                "{at}: states no `kind` (stdio, serial, rtt or semihosting)"
            ));
        }
        if self.kind.is_some_and(|k| k != ConsoleKind::Serial)
            && (self.usb.is_some() || self.baud.is_some())
        {
            return Err(format!(
                "{at}: `usb` and `baud` describe a serial console; this one is not \
                 `kind = \"serial\"`"
            ));
        }
        if self.baud == Some(0) {
            return Err(format!("{at}: `baud = 0` is not a rate"));
        }
        Ok(())
    }
}

impl ConsoleTable {
    /// `self` over `base`, key by key (see [`FlashTable::over`]).
    #[must_use]
    pub fn over(&self, base: &ConsoleTable) -> ConsoleTable {
        ConsoleTable {
            kind: self.kind.or(base.kind),
            usb: self.usb.or(base.usb),
            baud: self.baud.or(base.baud),
            command: self.command.clone().or_else(|| base.command.clone()),
        }
    }
}

/// `a` over `b`, either absent.
#[must_use]
pub fn over<T: Clone>(a: Option<&T>, b: Option<&T>, f: impl Fn(&T, &T) -> T) -> Option<T> {
    match (a, b) {
        (Some(a), Some(b)) => Some(f(a, b)),
        (a, b) => a.or(b).cloned(),
    }
}

/// `[<at>.<key>]` from a raw table (the leaf road), unchecked shape.
fn parse_table<T: serde::de::DeserializeOwned>(
    table: Option<&toml::Table>,
    at: &str,
    key: &str,
) -> Result<Option<T>, String> {
    let Some(v) = table.and_then(|t| t.get(key)) else {
        return Ok(None);
    };
    v.clone()
        .try_into()
        .map(Some)
        .map_err(|e| format!("[{at}.{key}]: {e}"))
}

/// `[<at>.flash]` (`at` = `image.<id>` or `image_defaults`), checked.
pub fn parse_flash_at(table: Option<&toml::Table>, at: &str) -> Result<Option<FlashTable>, String> {
    let t: Option<FlashTable> = parse_table(table, at, "flash")?;
    if let Some(f) = &t {
        f.validate(&format!("[{at}.flash]"), false)?;
    }
    Ok(t)
}

/// `[<at>.console]`, checked.
pub fn parse_console_at(
    table: Option<&toml::Table>,
    at: &str,
) -> Result<Option<ConsoleTable>, String> {
    let t: Option<ConsoleTable> = parse_table(table, at, "console")?;
    if let Some(c) = &t {
        c.validate(&format!("[{at}.console]"), false)?;
    }
    Ok(t)
}

/// `[image.<id>.flash]`, checked (the leaf road).
pub fn parse_image_flash(
    image: Option<&toml::Table>,
    id: &str,
) -> Result<Option<FlashTable>, String> {
    parse_flash_at(image, &format!("image.{id}"))
}

/// `[image.<id>.console]`, checked (the leaf road).
pub fn parse_image_console(
    image: Option<&toml::Table>,
    id: &str,
) -> Result<Option<ConsoleTable>, String> {
    parse_console_at(image, &format!("image.{id}"))
}

// ---- [board.run] ------------------------------------------------------------
//
// phase-544 W1 (issue 2172): `[board.run]` lives here, beside the flash and
// console tables, so the `nros` CLI's board catalog and the test harness read
// a board's launch with ONE type and ONE mapping onto a RunPlan's QEMU args
// ([`BoardRun::qemu_plan_args`]). The harness used to restate each board's
// `cpu`/`machine`/`icount`/`semihosting`/NIC model, so a descriptor edit never
// reached it.

/// The emulator kinds RFC-0114 D10's vocabulary names, and RFC-0118 D1's
/// launcher row: `<kind>:<machine>`. A twin is free and open source
/// (RFC-0115 Amendment 1), so a paid hosted simulator is no kind here and a
/// descriptor naming one is refused like any unknown kind.
pub const EMULATOR_KINDS: &[&str] = &["qemu", "fvp", "renode"];

/// The value `[board.run.qemu] semihosting = true` puts on
/// `-semihosting-config`.
pub const QEMU_SEMIHOSTING_CONFIG: &str = "enable=on,target=native";

/// `[board.run]` — RFC-0118 D4.
///
/// `emulator = "<kind>:<machine>"` names the emulator and the machine in the
/// RFC-0114 vocabulary (`qemu:mps2-an385`, `fvp:FVP_BaseR_AEMv8R`,
/// `renode:nucleo_f767zi`), or `host = true` says the image is
/// a host process. Never both. A runner-specific sub-table (`[board.run.qemu]`)
/// holds what only that runner reads; it requires the matching emulator kind.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BoardRun {
    /// `<kind>:<machine>`, kind one of [`EMULATOR_KINDS`].
    #[serde(default)]
    pub emulator: Option<String>,
    /// The image runs as a host process (no emulator, nothing flashed).
    #[serde(default)]
    pub host: bool,
    /// `[board.run.qemu]` — read only by the QEMU runner.
    #[serde(default)]
    pub qemu: Option<BoardRunQemu>,
    /// `[board.run.renode]` — read only by the Renode runner (phase-524 W4a).
    #[serde(default)]
    pub renode: Option<BoardRunRenode>,
}

/// `[board.run.qemu]` — the QEMU invocation's board facts.
///
/// They become a RunPlan's QEMU `args` through [`BoardRun::qemu_plan_args`],
/// which `nros_qemu_ladder::plan_line` renders in one fixed order
/// (`qemu-system-<system> [-cpu] -machine -nographic [-icount]
/// [-semihosting-config] [-nic] -kernel`), with the kernel appended by the
/// caller. A board whose QEMU line does not fit this shape (the NuttX and
/// ThreadX riscv64 boards, whose NIC is a virtio-mmio device the harness
/// varies per pair) states only `emulator` and keeps its line in
/// `nros-qemu-ladder`.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BoardRunQemu {
    /// The `qemu-system-<system>` binary's suffix (`arm`, `riscv64`).
    pub system: String,
    /// QEMU's `-machine`, when it differs from the machine word of
    /// `emulator` (that word is RFC-0114 vocabulary, not always QEMU's).
    #[serde(default)]
    pub machine: Option<String>,
    /// `-cpu`, when the machine's default CPU is not the one.
    #[serde(default)]
    pub cpu: Option<String>,
    /// `-icount <value>` (`shift=auto`: book-internals/src/testing/qemu-icount.md).
    #[serde(default)]
    pub icount: Option<String>,
    /// `-semihosting-config enable=on,target=native`.
    #[serde(default)]
    pub semihosting: bool,
    /// The NIC model (`lan9118`). The board states only the model; how it
    /// attaches (QEMU's user network, a multicast socket, a TAP) is the
    /// image's or the caller's.
    #[serde(default)]
    pub nic_model: Option<String>,
}

/// `[board.run.renode]` — the Renode twin's wiring (phase-524 W4a), read only
/// by the Renode runner: `nros_runner_renode`, which `nros run` hands it as
/// the plan's `link` and `console` args (phase-524 W4b), and the test
/// harness's launcher (`scripts/probe/twin/renode.sh`, through
/// `nros_tests::renode`).
/// The platform is the emulator's machine word (`platforms/boards/<m>.repl`).
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BoardRunRenode {
    /// The `sysbus` peripheral that carries the image's serial link
    /// (`usart3`), wired to a host pty a router opens as `serial/`.
    pub link: String,
    /// The `sysbus` peripheral that carries the console (`usart6`), when it is
    /// not the link; captured to a file.
    #[serde(default)]
    pub console: Option<String>,
}

impl BoardRun {
    /// `(kind, machine)` of `emulator`, when it is well formed.
    #[must_use]
    pub fn emulator_parts(&self) -> Option<(&str, &str)> {
        let (kind, machine) = self.emulator.as_deref()?.split_once(':')?;
        (EMULATOR_KINDS.contains(&kind) && !machine.is_empty()).then_some((kind, machine))
    }

    /// The emulator platform Zephyr's `west build -t run` names for this
    /// kind (`armfvp` for an FVP), projected as `NROS_BOARD_RUNNER`.
    #[must_use]
    pub fn zephyr_emulator(&self) -> Option<&'static str> {
        match self.emulator_parts()?.0 {
            "fvp" => Some("armfvp"),
            "qemu" => Some("qemu"),
            "renode" => Some("renode"),
            _ => None,
        }
    }

    /// The RunPlan QEMU `args` the board states (`system`, `cpu`, `machine`,
    /// `icount`, `semihosting`), when it states `[board.run.qemu]`: THE
    /// mapping, used by `nros run`'s plan, the cargo `runner` and the test
    /// harness alike. The NIC is not here: the board states its model
    /// ([`BoardRunQemu::nic_model`]) and each caller attaches it to a network.
    #[must_use]
    pub fn qemu_plan_args(&self) -> Option<std::collections::BTreeMap<String, String>> {
        let q = self.qemu.as_ref()?;
        let machine = q
            .machine
            .as_deref()
            .or_else(|| self.emulator_parts().map(|(_, m)| m))?;
        let mut args = std::collections::BTreeMap::new();
        args.insert("system".to_string(), q.system.clone());
        args.insert("machine".to_string(), machine.to_string());
        if let Some(c) = &q.cpu {
            args.insert("cpu".to_string(), c.clone());
        }
        if let Some(i) = &q.icount {
            args.insert("icount".to_string(), i.clone());
        }
        if q.semihosting {
            args.insert(
                "semihosting".to_string(),
                QEMU_SEMIHOSTING_CONFIG.to_string(),
            );
        }
        Some(args)
    }

    /// Why this table is malformed, if it is.
    #[must_use]
    pub fn shape_error(&self) -> Option<String> {
        if self.host && self.emulator.is_some() {
            return Some(
                "`[board.run]` states both `host = true` and `emulator`; a board runs \
                 as a host process OR under an emulator"
                    .into(),
            );
        }
        if let Some(e) = &self.emulator
            && self.emulator_parts().is_none()
        {
            return Some(format!(
                "`[board.run] emulator = \"{e}\"` must read `<kind>:<machine>` with kind \
                 one of {}",
                EMULATOR_KINDS.join(", ")
            ));
        }
        if self.qemu.is_some() && self.emulator_parts().map(|(k, _)| k) != Some("qemu") {
            return Some(
                "`[board.run.qemu]` is read only by the QEMU runner, so it needs \
                 `[board.run] emulator = \"qemu:<machine>\"`"
                    .into(),
            );
        }
        let kind = self.emulator_parts().map(|(k, _)| k);
        if self.renode.is_some() && kind != Some("renode") {
            return Some(
                "`[board.run.renode]` is read only by the Renode runner, so it needs \
                 `[board.run] emulator = \"renode:<platform>\"`"
                    .into(),
            );
        }
        None
    }
}

/// The `[board.run]` of the board `name` in the text of an `nros-board.toml`
/// (a `[[board]]` array, each entry with `names = [..]`), checked, or `None`
/// when that board states no `[board.run]`.
///
/// For a reader that needs a board's launch and nothing else of its
/// descriptor (the test harness). The `nros` CLI parses the whole descriptor
/// into its catalog, whose `run` field is this same [`BoardRun`]; the CLI's
/// test `board_run_reader_agrees_with_the_catalog` holds the two readings
/// equal over every in-tree descriptor.
///
/// # Errors
///
/// The text is not TOML, no `[[board]]` names `name`, or its `[board.run]`
/// does not parse or is malformed ([`BoardRun::shape_error`]).
pub fn board_run_named(text: &str, name: &str) -> Result<Option<BoardRun>, String> {
    let doc: toml::Table = text.parse().map_err(|e| format!("not TOML: {e}"))?;
    let boards: Vec<&toml::Table> = match doc.get("board") {
        Some(toml::Value::Array(a)) => a.iter().filter_map(toml::Value::as_table).collect(),
        Some(toml::Value::Table(t)) => vec![t],
        _ => Vec::new(),
    };
    let board = boards
        .into_iter()
        .find(|b| {
            b.get("names")
                .and_then(toml::Value::as_array)
                .is_some_and(|ns| ns.iter().any(|n| n.as_str() == Some(name)))
        })
        .ok_or_else(|| format!("no `[[board]]` names `{name}`"))?;
    let run: Option<BoardRun> = parse_table(Some(board), &format!("board `{name}`"), "run")?;
    if let Some(e) = run.as_ref().and_then(BoardRun::shape_error) {
        return Err(e);
    }
    Ok(run)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// phase-544 W1: `[board.run]` by board name, mapped onto plan args.
    #[test]
    fn board_run_named_reads_one_board_and_maps_its_qemu_facts() {
        let text = "[[board]]\nnames = [\"a\", \"a2\"]\n[board.run]\nemulator = \"qemu:m1\"\n\
                    [board.run.qemu]\nsystem = \"arm\"\ncpu = \"c\"\nicount = \"shift=auto\"\n\
                    semihosting = true\nnic_model = \"n\"\n\
                    [[board]]\nnames = [\"b\"]\n[board.run]\nhost = true\n";
        let a = board_run_named(text, "a2").unwrap().unwrap();
        let args = a.qemu_plan_args().unwrap();
        let got: Vec<(&str, &str)> = args.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        assert_eq!(
            got,
            [
                ("cpu", "c"),
                ("icount", "shift=auto"),
                ("machine", "m1"),
                ("semihosting", QEMU_SEMIHOSTING_CONFIG),
                ("system", "arm"),
            ]
        );
        let b = board_run_named(text, "b").unwrap().unwrap();
        assert!(b.host && b.qemu_plan_args().is_none());
        assert!(
            board_run_named(text, "c")
                .unwrap_err()
                .contains("names `c`")
        );
        let bad = "[[board]]\nnames = [\"x\"]\n[board.run]\nhost = true\nemulator = \"qemu:m\"\n";
        assert!(
            board_run_named(bad, "x")
                .unwrap_err()
                .contains("OR under an emulator")
        );
    }

    fn flash(body: &str) -> Result<FlashTable, String> {
        let t: FlashTable = toml::from_str(body).map_err(|e| e.to_string())?;
        t.validate("[board.flash]", true)?;
        Ok(t)
    }

    #[test]
    fn a_flash_table_parses_into_typed_steps() {
        let f = flash(
            "method = \"probe-rs\"\nchip = \"STM32F767ZITx\"\n\
             prepare = [\n\
               { say = \"Set BOOT0 to 1, then press RESET\", wait_for = \"usb:0483:df11\" },\n\
               { wait_for = \"port:/dev/ttyACM0\" },\n\
               { wait_for = \"volume:RPI-RP2\" },\n\
               { wait_for = \"line:ready\" },\n\
               { run = \"relay on 1\" },\n\
               { say = \"Plug it in\" },\n\
             ]\n",
        )
        .unwrap();
        assert_eq!(f.method.as_deref(), Some("probe-rs"));
        assert_eq!(
            f.prepare,
            vec![
                PrepareStep::SayThenWait {
                    say: "Set BOOT0 to 1, then press RESET".into(),
                    wait_for: WaitFor::Usb(UsbId {
                        vid: 0x0483,
                        pid: 0xdf11
                    }),
                },
                PrepareStep::WaitFor(WaitFor::Port("/dev/ttyACM0".into())),
                PrepareStep::WaitFor(WaitFor::Volume("RPI-RP2".into())),
                PrepareStep::WaitFor(WaitFor::Line("ready".into())),
                PrepareStep::Run("relay on 1".into()),
                PrepareStep::Say("Plug it in".into()),
            ]
        );
        // Round-trips through serde as it was written.
        let again: FlashTable = toml::from_str(&toml::to_string(&f).unwrap()).unwrap();
        assert_eq!(again, f);
    }

    #[test]
    fn malformed_flash_tables_and_steps_are_refused() {
        for (body, needle) in [
            (
                "method = \"probe-rs\"\ncommand = \"x {elf}\"\n",
                "alternatives",
            ),
            ("chip = \"x\"\n", "neither `method`"),
            ("method = \"Probe RS\"\n", "lowercase runner word"),
            ("method = \"west\"\nflasher = \"x\"\n", "unknown field"),
            (
                "method = \"w\"\nprepare = [{ say = \"a\", run = \"b\" }]\n",
                "only `say` and",
            ),
            (
                "method = \"w\"\nprepare = [{ wait_for = \"usb:0483\" }]\n",
                "four hex digits",
            ),
            (
                "method = \"w\"\nprepare = [{ wait_for = \"usb:483:df11\" }]\n",
                "four hex digits",
            ),
            (
                "method = \"w\"\nprepare = [{ wait_for = \"gpio:3\" }]\n",
                "unknown event kind",
            ),
            (
                "method = \"w\"\nprepare = [{ wait_for = \"port:\" }]\n",
                "names no path",
            ),
            ("method = \"w\"\nprepare = [{}]\n", "states nothing"),
            (
                "method = \"w\"\nprepare = [{ sleep = 3 }]\n",
                "unknown field",
            ),
            (
                "command = \"pyocd flash {image}\"\n",
                "unknown placeholder `{image}`",
            ),
            ("command = \"pyocd flash {elf\"\n", "never closes"),
            ("command = \"pyocd }{elf}\"\n", "no `{` before"),
            ("command = \"x {tool:}\"\n", "unknown placeholder"),
        ] {
            let e = flash(body).expect_err(body);
            assert!(e.contains(needle), "{body:?}: {e}");
        }
    }

    #[test]
    fn a_template_names_only_the_closed_placeholder_list() {
        let c = CommandTemplate::try_from(
            "{tool:pyocd} flash -t stm32f767zi {hex} --port {port} -b {baud} {elf} {bin} {uf2} \
             {build_dir} {probe} {nic}"
                .to_string(),
        )
        .unwrap();
        assert_eq!(c.placeholders()[0], "tool:pyocd");
        assert_eq!(c.placeholders().len(), 10);
    }

    #[test]
    fn console_tables_check_their_kind() {
        let board = |b: &str| {
            let t: ConsoleTable = toml::from_str(b).map_err(|e| e.to_string())?;
            t.validate("[board.console]", true).map(|()| t)
        };
        let c = board("kind = \"serial\"\nusb = \"0483:374b\"\nbaud = 115200\n").unwrap();
        assert_eq!(c.usb.map(|u| u.to_string()).as_deref(), Some("0483:374b"));
        for (b, needle) in [
            ("usb = \"0483:374b\"\n", "no `kind`"),
            ("kind = \"stdio\"\nbaud = 9600\n", "serial console"),
            ("kind = \"serial\"\nbaud = 0\n", "not a rate"),
            ("kind = \"jtag\"\n", "unknown variant"),
            (
                "kind = \"serial\"\ncommand = \"picocom {tty}\"\n",
                "unknown placeholder",
            ),
        ] {
            let e = board(b).expect_err(b);
            assert!(e.contains(needle), "{b:?}: {e}");
        }
        // An image table may leave `kind` to the board.
        let img: ConsoleTable = toml::from_str("baud = 921600\n").unwrap();
        img.validate("[image.a.console]", false).unwrap();
    }

    #[test]
    fn the_leaf_road_parses_image_flash_and_console() {
        let t: toml::Table = toml::from_str(
            "[image.a]\nboard = \"b\"\n[image.a.flash]\ncommand = \"pyocd flash {hex}\"\n\
             prepare = [{ say = \"Press RESET\" }]\n[image.a.console]\nbaud = 921600\n",
        )
        .unwrap();
        let img = t["image"]["a"].as_table();
        let f = parse_image_flash(img, "a").unwrap().unwrap();
        assert_eq!(
            f.command.as_ref().map(CommandTemplate::as_str),
            Some("pyocd flash {hex}")
        );
        assert_eq!(
            parse_image_console(img, "a").unwrap().unwrap().baud,
            Some(921600)
        );
        let bad: toml::Table =
            toml::from_str("[image.a.flash]\nprepare = [{ wait_for = \"tcp:1\" }]\n").unwrap();
        let e = parse_image_flash(bad["image"]["a"].as_table(), "a").unwrap_err();
        assert!(
            e.starts_with("[image.a.flash]") && e.contains("unknown event kind"),
            "{e}"
        );
    }

    #[test]
    fn doubled_braces_are_literal_and_tables_merge_key_by_key() {
        let c = CommandTemplate::try_from("sh -c 'echo {{x}}' {elf}".to_string()).unwrap();
        assert_eq!(c.placeholders(), vec!["elf"]);
        let base = FlashTable {
            method: Some("probe-rs".into()),
            chip: Some("A".into()),
            prepare: vec![PrepareStep::Say("x".into())],
            ..FlashTable::default()
        };
        let image = FlashTable {
            chip: Some("B".into()),
            ..FlashTable::default()
        };
        let m = image.over(&base);
        assert_eq!(m.method.as_deref(), Some("probe-rs"));
        assert_eq!(m.chip.as_deref(), Some("B"));
        assert_eq!(m.prepare, base.prepare);
        let cmd = FlashTable {
            command: Some(CommandTemplate::try_from("x {hex}".to_string()).unwrap()),
            ..FlashTable::default()
        };
        let m = cmd.over(&base);
        assert!(
            m.method.is_none() && m.command.is_some(),
            "a command replaces the method"
        );
    }
}
