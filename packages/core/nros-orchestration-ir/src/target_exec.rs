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

#[cfg(test)]
mod tests {
    use super::*;

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
