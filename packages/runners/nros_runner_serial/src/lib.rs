//! The `serial` console runner's parts: what the plan says about the port,
//! opening it raw at a baud rate, and the two streams between it and nros.
//!
//! The port is the plan's `console.port`, which nros resolved before the
//! runner starts (RFC-0118 D6: `--port`, `NROS_PORT`, the bench file, exactly
//! one attached device). This crate resolves nothing on its own.
//!
//! What the plan carries for a serial console:
//!
//! | Field | Meaning | Default |
//! | --- | --- | --- |
//! | `console.port` | the device to open | none: refused |
//! | `console.baud` | the baud rate | 115200 |
//! | `args.line_ending` | `lf`, `cr` or `crlf`, for what is SENT to the board | `lf` |
//! | `args.send` | lines to send once the port is open (`nros monitor --send`), one per `\n` | none |
//! | `console.command` | a template that attaches the console instead (`picocom -b {baud} {port}`) | none |
//!
//! What the board prints reaches stdout byte for byte (RFC-0119 D6): nros
//! grades `--expect` over it. Only the direction TO the board is translated.

use std::{
    fs::File,
    io::{self, Read, Write},
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    path::Path,
};

use nros_extension::{plan::RunPlan, runner};

/// The baud rate when the plan states none.
pub const DEFAULT_BAUD: u32 = 115_200;

/// What a newline sent to the board becomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    /// `\n`
    Lf,
    /// `\r`
    Cr,
    /// `\r\n`
    CrLf,
}

impl LineEnding {
    /// Parse `lf`, `cr` or `crlf`.
    ///
    /// # Errors
    ///
    /// Any other word.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "lf" => Ok(Self::Lf),
            "cr" => Ok(Self::Cr),
            "crlf" => Ok(Self::CrLf),
            other => Err(format!(
                "`line_ending = {other:?}`: one of `lf`, `cr`, `crlf`"
            )),
        }
    }

    /// The bytes it is.
    #[must_use]
    pub fn bytes(self) -> &'static [u8] {
        match self {
            Self::Lf => b"\n",
            Self::Cr => b"\r",
            Self::CrLf => b"\r\n",
        }
    }
}

/// Translates a stream sent to the board: every `\n`, and every `\r\n`, becomes
/// the line ending. A `\r` is held until the next byte shows whether it starts
/// a `\r\n`, so the rule holds across reads.
#[derive(Debug, Clone)]
pub struct Encoder {
    ending: LineEnding,
    pending_cr: bool,
}

impl Encoder {
    /// An encoder for `ending`.
    #[must_use]
    pub fn new(ending: LineEnding) -> Self {
        Self {
            ending,
            pending_cr: false,
        }
    }

    /// Translate `input`, appending to `out`.
    pub fn push(&mut self, input: &[u8], out: &mut Vec<u8>) {
        for &b in input {
            if std::mem::take(&mut self.pending_cr) {
                if b == b'\n' {
                    out.extend_from_slice(self.ending.bytes());
                    continue;
                }
                out.push(b'\r');
            }
            match b {
                b'\r' => self.pending_cr = true,
                b'\n' => out.extend_from_slice(self.ending.bytes()),
                b => out.push(b),
            }
        }
    }

    /// The held `\r`, if the stream ended on one.
    pub fn finish(&mut self, out: &mut Vec<u8>) {
        if std::mem::take(&mut self.pending_cr) {
            out.push(b'\r');
        }
    }
}

/// The serial console a plan describes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// The device.
    pub port: String,
    /// The baud rate, one [`speed`] knows.
    pub baud: u32,
    /// What a sent newline becomes.
    pub ending: LineEnding,
    /// Lines to send once the port is open.
    pub send: Vec<String>,
}

/// The serial settings of `plan`.
///
/// # Errors
///
/// No port, a baud rate the host's termios has no constant for, or an unknown
/// line ending.
pub fn settings(plan: &RunPlan) -> Result<Settings, String> {
    let port = plan.console.port.clone().ok_or(
        "the plan carries no console port. nros resolves one from `--port`, `NROS_PORT`, the \
         bench file (`nros bench scan`) or exactly one attached device (RFC-0118 D6)",
    )?;
    let baud = plan.console.baud.unwrap_or(DEFAULT_BAUD);
    if speed(baud).is_none() {
        return Err(format!(
            "baud {baud} is not a rate this host's serial driver takes; one of {}",
            BAUDS
                .iter()
                .map(|(b, _)| b.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let ending = plan
        .args
        .get("line_ending")
        .map(|s| LineEnding::parse(s))
        .transpose()?
        .unwrap_or(LineEnding::Lf);
    let send = plan
        .args
        .get("send")
        .map(|s| s.split('\n').map(str::to_string).collect())
        .unwrap_or_default();
    Ok(Settings {
        port,
        baud,
        ending,
        send,
    })
}

#[cfg(target_os = "linux")]
const BAUDS: &[(u32, libc::speed_t)] = &[
    (1200, libc::B1200),
    (2400, libc::B2400),
    (4800, libc::B4800),
    (9600, libc::B9600),
    (19200, libc::B19200),
    (38400, libc::B38400),
    (57600, libc::B57600),
    (115_200, libc::B115200),
    (230_400, libc::B230400),
    (460_800, libc::B460800),
    (500_000, libc::B500000),
    (576_000, libc::B576000),
    (921_600, libc::B921600),
    (1_000_000, libc::B1000000),
    (1_500_000, libc::B1500000),
    (2_000_000, libc::B2000000),
];

#[cfg(not(target_os = "linux"))]
const BAUDS: &[(u32, libc::speed_t)] = &[
    (1200, libc::B1200),
    (2400, libc::B2400),
    (4800, libc::B4800),
    (9600, libc::B9600),
    (19200, libc::B19200),
    (38400, libc::B38400),
    (57600, libc::B57600),
    (115_200, libc::B115200),
    (230_400, libc::B230400),
];

/// The termios speed constant for `baud`.
#[must_use]
pub fn speed(baud: u32) -> Option<libc::speed_t> {
    BAUDS.iter().find(|(b, _)| *b == baud).map(|(_, s)| *s)
}

fn check(rc: libc::c_int) -> io::Result<()> {
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Open `port` for reading and writing, raw (no echo, no line discipline, no
/// newline translation), 8N1 at `baud`, ignoring modem control lines.
///
/// # Errors
///
/// The device cannot be opened or configured, or `baud` has no constant.
pub fn open(port: &Path, baud: u32) -> io::Result<File> {
    let sp = speed(baud).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported baud {baud}"),
        )
    })?;
    // O_NONBLOCK so the open does not wait for carrier detect; cleared below.
    let f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOCTTY | libc::O_NONBLOCK)
        .open(port)?;
    let fd = f.as_raw_fd();
    // SAFETY: `fd` is an open descriptor owned by `f` for this whole block,
    // and `t` is a plain C struct the calls fill in.
    unsafe {
        let mut t: libc::termios = std::mem::zeroed();
        check(libc::tcgetattr(fd, &mut t))?;
        libc::cfmakeraw(&mut t);
        t.c_cflag |= libc::CLOCAL | libc::CREAD;
        t.c_cflag &= !(libc::CSTOPB | libc::PARENB);
        t.c_cc[libc::VMIN] = 1;
        t.c_cc[libc::VTIME] = 0;
        check(libc::cfsetispeed(&mut t, sp))?;
        check(libc::cfsetospeed(&mut t, sp))?;
        check(libc::tcsetattr(fd, libc::TCSANOW, &t))?;
        let flags = libc::fcntl(fd, libc::F_GETFL);
        if flags < 0 || libc::fcntl(fd, libc::F_SETFL, flags & !libc::O_NONBLOCK) < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(f)
}

/// Stream the console: send `settings.send`, forward `input` to the port
/// (translated) on its own thread, and copy the port to `output` unmodified
/// until nros stops reading or the port goes away.
///
/// Returns `Ok` when `output` closes (nros stopped the console), and an error
/// naming the port when the port ends or fails: a console that went away is
/// the runner's failure (RFC-0118 D2 exit 2).
///
/// # Errors
///
/// A write to the port, or the port ending or failing.
pub fn serve<I, O>(port: File, settings: &Settings, input: I, mut output: O) -> Result<(), String>
where
    I: Read + Send + 'static,
    O: Write,
{
    let name = settings.port.clone();
    let mut tx = port
        .try_clone()
        .map_err(|e| format!("{name}: cannot share the port: {e}"))?;
    let mut enc = Encoder::new(settings.ending);
    let mut buf = Vec::new();
    for line in &settings.send {
        enc.push(line.as_bytes(), &mut buf);
        buf.extend_from_slice(settings.ending.bytes());
    }
    if !buf.is_empty() {
        tx.write_all(&buf)
            .and_then(|()| tx.flush())
            .map_err(|e| format!("{name}: sending `--send`: {e}"))?;
    }
    std::thread::spawn(move || forward(input, tx, enc));

    let mut rx = port;
    let mut chunk = [0u8; 4096];
    loop {
        let n = match rx.read(&mut chunk) {
            Ok(0) => return Err(format!("{name}: the port closed")),
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(format!("{name}: the port went away: {e}")),
        };
        if output
            .write_all(&chunk[..n])
            .and_then(|()| output.flush())
            .is_err()
        {
            // nros closed the stream: the console is over, not failed.
            return Ok(());
        }
    }
}

/// Forward `input` to the port until `input` ends. A write failure ends the
/// thread; the reading side reports the port.
fn forward<I: Read>(mut input: I, mut tx: File, mut enc: Encoder) {
    let mut chunk = [0u8; 1024];
    let mut out = Vec::new();
    loop {
        out.clear();
        match input.read(&mut chunk) {
            Ok(0) => {
                enc.finish(&mut out);
                let _ = tx.write_all(&out);
                return;
            }
            Ok(n) => enc.push(&chunk[..n], &mut out),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return,
        }
        if tx.write_all(&out).and_then(|()| tx.flush()).is_err() {
            return;
        }
    }
}

/// The argument vector of a `console.command` template, program first: split
/// into words, then each word expanded from the plan (the `command` runner's
/// rule), then arguments after `--`.
///
/// # Errors
///
/// An unclosed quote, an empty template, or an unknown or missing
/// placeholder.
pub fn command_argv(plan: &RunPlan, template: &str) -> Result<Vec<String>, String> {
    let words = runner::split_words(template).map_err(|e| format!("`console.command`: {e}"))?;
    if words.is_empty() {
        return Err("the `console.command` template is empty".to_string());
    }
    let mut out = words
        .iter()
        .map(|w| {
            plan.expand(w)
                .map_err(|e| format!("`console.command = {template:?}`: {e}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    out.extend(plan.passthrough.iter().cloned());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(extra: &str) -> RunPlan {
        RunPlan::from_json(&format!(
            r#"{{"image":"i","verb":"console","runner":{{"package":"nros_runner_serial","name":"serial"}},
            "target":{{"flasher":"west"}},"console":{{"kind":"serial"{extra}}}}}"#
        ))
        .unwrap()
    }

    #[test]
    fn newlines_become_the_line_ending_across_reads() {
        let mut e = Encoder::new(LineEnding::CrLf);
        let mut out = Vec::new();
        e.push(b"a\nb\r", &mut out);
        e.push(b"\nc\rd", &mut out);
        e.push(b"\r", &mut out);
        e.finish(&mut out);
        assert_eq!(out, b"a\r\nb\r\nc\rd\r");
        let mut out = Vec::new();
        Encoder::new(LineEnding::Cr).push(b"x\r\ny\n", &mut out);
        assert_eq!(out, b"x\ry\r");
    }

    #[test]
    fn settings_default_the_baud_and_refuse_what_is_missing() {
        let s = settings(&plan(r#","port":"/dev/ttyACM0""#)).unwrap();
        assert_eq!(s.baud, DEFAULT_BAUD);
        assert_eq!(s.ending, LineEnding::Lf);
        assert!(s.send.is_empty());

        let e = settings(&plan("")).unwrap_err();
        assert!(e.contains("--port") && e.contains("nros bench scan"), "{e}");
        let e = settings(&plan(r#","port":"/p","baud":12345"#)).unwrap_err();
        assert!(e.contains("12345") && e.contains("115200"), "{e}");

        let mut p = plan(r#","port":"/p","baud":9600"#);
        p.args.insert("line_ending".into(), "crlf".into());
        p.args.insert("send".into(), "help\nstatus".into());
        let s = settings(&p).unwrap();
        assert_eq!((s.baud, s.ending), (9600, LineEnding::CrLf));
        assert_eq!(s.send, ["help", "status"]);
        p.args.insert("line_ending".into(), "nl".into());
        assert!(settings(&p).unwrap_err().contains("`lf`, `cr`, `crlf`"));
    }

    #[test]
    fn a_console_command_expands_from_the_plan() {
        let p = plan(r#","port":"/dev/ttyUSB0","baud":9600"#);
        assert_eq!(
            command_argv(&p, "picocom -b {baud} {port}").unwrap(),
            ["picocom", "-b", "9600", "/dev/ttyUSB0"]
        );
        assert!(command_argv(&p, "x {nope}").is_err());
        assert!(command_argv(&p, "  ").is_err());
    }
}
