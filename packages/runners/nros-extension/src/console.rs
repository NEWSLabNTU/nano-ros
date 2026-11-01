//! The console pump and the `link` line (RFC-0119 D6).
//!
//! A runner's stdout is the target's console stream, **unmodified**: nros
//! grades `--expect` over it and captures it to `console.log`, so a runner
//! never prefixes, reflows or colours it. Diagnostics go to stderr, and once a
//! runner knows where the target's RMW link is (for example the pty a serial
//! console was given) it prints one line there:
//!
//! ```text
//! nros-runner: link <endpoint>
//! ```
//!
//! [`pump`] is the runner side of the stdout rule, [`report_link`] the runner
//! side of the stderr rule, and [`scan_stderr`] the nros side of both.

use std::{
    io::{self, BufRead, Read, Write},
    thread::{self, JoinHandle},
};

/// The prefix of the link line on a runner's stderr.
pub const LINK_PREFIX: &str = "nros-runner: link ";

/// The link line for `endpoint`, without a newline.
#[must_use]
pub fn link_line(endpoint: &str) -> String {
    format!("{LINK_PREFIX}{endpoint}")
}

/// Print the link line on stderr. Call it once, as soon as the endpoint is
/// known.
pub fn report_link(endpoint: &str) {
    let mut err = io::stderr().lock();
    // A closed stderr leaves nothing to report the failure on.
    let _ = writeln!(err, "{}", link_line(endpoint));
    let _ = err.flush();
}

/// The endpoint a stderr line reports, if it is a link line.
#[must_use]
pub fn parse_link_line(line: &str) -> Option<&str> {
    let endpoint = line
        .trim_end_matches(['\r', '\n'])
        .strip_prefix(LINK_PREFIX)?
        .trim();
    (!endpoint.is_empty()).then_some(endpoint)
}

/// Copy `from` to `to` byte for byte until end of file, flushing after every
/// read so an interactive console is not held in a buffer. Returns the bytes
/// copied.
///
/// # Errors
///
/// A read or write failure. A write that fails because the reader of `to`
/// went away is returned like any other, for the caller to decide.
pub fn pump<R: Read, W: Write>(mut from: R, mut to: W) -> io::Result<u64> {
    let mut buf = [0u8; 8192];
    let mut total = 0u64;
    loop {
        let n = match from.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        to.write_all(&buf[..n])?;
        to.flush()?;
        total += n as u64;
    }
    Ok(total)
}

/// [`pump`] on its own thread, for a runner that pumps a child's stdout to
/// its own while forwarding stdin the other way.
pub fn spawn_pump<R, W>(from: R, to: W) -> JoinHandle<io::Result<u64>>
where
    R: Read + Send + 'static,
    W: Write + Send + 'static,
{
    thread::spawn(move || pump(from, to))
}

/// Forward a runner's stderr to `to` line by line and call `on_link` with the
/// endpoint of every link line. Every line is forwarded, link lines included:
/// they are diagnostics too.
///
/// # Errors
///
/// A read or write failure.
pub fn scan_stderr<R: BufRead, W: Write>(
    mut from: R,
    mut to: W,
    mut on_link: impl FnMut(&str),
) -> io::Result<()> {
    let mut line = String::new();
    loop {
        line.clear();
        if from.read_line(&mut line)? == 0 {
            return Ok(());
        }
        to.write_all(line.as_bytes())?;
        to.flush()?;
        if let Some(endpoint) = parse_link_line(&line) {
            on_link(endpoint);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_lines_round_trip() {
        let l = link_line("/dev/pts/7");
        assert_eq!(l, "nros-runner: link /dev/pts/7");
        assert_eq!(parse_link_line(&format!("{l}\n")), Some("/dev/pts/7"));
        assert_eq!(parse_link_line("nros-runner: link   \n"), None);
        assert_eq!(parse_link_line("qemu: link /dev/pts/7"), None);
    }

    #[test]
    fn the_pump_copies_bytes_unmodified() {
        let input: Vec<u8> = (0..=255u8).cycle().take(20_000).collect();
        let mut out = Vec::new();
        assert_eq!(pump(&input[..], &mut out).unwrap(), 20_000);
        assert_eq!(out, input);
    }

    #[test]
    fn spawned_pump_returns_the_count() {
        let h = spawn_pump(io::Cursor::new(b"hello\r\n".to_vec()), io::sink());
        assert_eq!(h.join().unwrap().unwrap(), 7);
    }

    #[test]
    fn stderr_scan_forwards_everything_and_reports_the_link() {
        let input = "starting qemu\nnros-runner: link tcp/127.0.0.1:7447\nlast\n";
        let mut out = Vec::new();
        let mut links = Vec::new();
        scan_stderr(input.as_bytes(), &mut out, |e| links.push(e.to_string())).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), input);
        assert_eq!(links, ["tcp/127.0.0.1:7447"]);
    }
}
