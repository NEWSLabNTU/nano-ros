//! The serial runner against a pty pair: the runner opens the pty's device
//! side as its port, the test plays the board on the other side. No hardware.

use std::{
    fs::File,
    io::{Read, Write},
    os::fd::{FromRawFd, OwnedFd},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

const BIN: &str = env!("CARGO_BIN_EXE_nros-runner-serial");

/// A pty pair: the controller side (the "board") and the device's path.
fn pty() -> (File, String) {
    // SAFETY: plain libc calls on a descriptor this function owns; `name` is
    // a buffer `ptsname_r` fills with a NUL-terminated path.
    unsafe {
        let fd = libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY);
        assert!(fd >= 0, "posix_openpt: {}", std::io::Error::last_os_error());
        // Not inherited by the runner: a child holding the controller side
        // open would keep the "board" attached after the test drops it.
        assert_eq!(libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC), 0);
        assert_eq!(libc::grantpt(fd), 0);
        assert_eq!(libc::unlockpt(fd), 0);
        let mut name = [0 as libc::c_char; 128];
        assert_eq!(libc::ptsname_r(fd, name.as_mut_ptr(), name.len()), 0);
        let path = std::ffi::CStr::from_ptr(name.as_ptr())
            .to_string_lossy()
            .into_owned();
        (File::from(OwnedFd::from_raw_fd(fd)), path)
    }
}

struct Dir(PathBuf);
impl Dir {
    fn new(tag: &str) -> Self {
        let d =
            std::env::temp_dir().join(format!("nros-runner-serial-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        Dir(d)
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn plan(dir: &Dir, console: &str, args: &str) -> PathBuf {
    let p = dir.0.join("plan.json");
    std::fs::write(
        &p,
        format!(
            r#"{{"image":"i","verb":"console","runner":{{"package":"nros_runner_serial","name":"serial"}},
            "target":{{"flasher":"west"}},"console":{{"kind":"serial"{console}}},"args":{{{args}}}}}"#
        ),
    )
    .unwrap();
    p
}

/// Read from `r` on a thread until `want` has arrived or 10 s pass.
fn read_until(mut r: impl Read + Send + 'static, want: &'static [u8]) -> Vec<u8> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut got = Vec::new();
        let mut b = [0u8; 256];
        while !got.windows(want.len()).any(|w| w == want) {
            match r.read(&mut b) {
                Ok(0) | Err(_) => break,
                Ok(n) => got.extend_from_slice(&b[..n]),
            }
        }
        let _ = tx.send(got);
    });
    rx.recv_timeout(Duration::from_secs(10))
        .unwrap_or_else(|_| panic!("timed out waiting for {:?}", String::from_utf8_lossy(want)))
}

fn spawn(plan: &Path) -> Child {
    Command::new(BIN)
        .arg("console")
        .arg("--plan")
        .arg(plan)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

#[test]
fn the_console_streams_both_ways_with_the_line_ending() {
    let (board, port) = pty();
    let dir = Dir::new("both");
    let plan = plan(
        &dir,
        &format!(r#","port":"{port}","baud":9600"#),
        r#""send":"status","line_ending":"crlf""#,
    );
    let mut child = spawn(&plan);

    // `--send` reaches the board first, with the plan's line ending.
    let got = read_until(board.try_clone().unwrap(), b"status\r\n");
    assert!(got.ends_with(b"status\r\n"), "{got:?}");

    // What the board prints reaches stdout unmodified (no newline mapping).
    let mut b = board.try_clone().unwrap();
    b.write_all(b"Publishing: 0\nraw\r\n").unwrap();
    let out = read_until(child.stdout.take().unwrap(), b"raw\r\n");
    assert_eq!(out, b"Publishing: 0\nraw\r\n");

    // stdin goes to the board, translated.
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"reboot\n")
        .unwrap();
    child.stdin.as_mut().unwrap().flush().unwrap();
    let got = read_until(board.try_clone().unwrap(), b"reboot\r\n");
    assert!(got.ends_with(b"reboot\r\n"), "{got:?}");

    // The console runs until it is stopped.
    assert!(
        child.try_wait().unwrap().is_none(),
        "the runner exited on its own"
    );
    child.kill().unwrap();
    child.wait().unwrap();
}

#[test]
fn a_port_that_goes_away_is_the_runners_failure() {
    let (board, port) = pty();
    let dir = Dir::new("gone");
    let plan = plan(&dir, &format!(r#","port":"{port}""#), r#""send":"hi""#);
    let mut child = spawn(&plan);
    let _ = read_until(board.try_clone().unwrap(), b"hi\n");
    drop(board);
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break s;
        }
        assert!(Instant::now() < deadline, "the runner outlived its port");
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(status.code(), Some(2));
}

#[test]
fn no_port_and_a_bad_baud_are_refused_with_exit_2() {
    let dir = Dir::new("refused");
    let out = Command::new(BIN)
        .args(["console", "--plan"])
        .arg(plan(&dir, "", ""))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("nros bench scan"));

    let (_board, port) = pty();
    let out = Command::new(BIN)
        .args(["console", "--plan"])
        .arg(plan(&dir, &format!(r#","port":"{port}","baud":31337"#), ""))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("31337"));
}
