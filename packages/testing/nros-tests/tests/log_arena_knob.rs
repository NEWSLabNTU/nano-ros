//! phase-479 W5 (RFC-0102 D5) — the runtime-logger arena is a KNOB, and an
//! image's own statement outranks its board's.
//!
//! `NROS_LOG_DYNAMIC_LOGGERS` replaced the `dynamic-loggers-<N>` cargo
//! features because features UNION across a build with no precedence: two
//! crates picking different sizes silently got the smallest, and nothing could
//! say "this image overrides the board". The knob rides the RFC-0049 ladder
//! (image env > Kconfig / board > 16), so the question this test asks of a
//! BUILT image is exactly the one the features could not answer.
//!
//! The fixture (`bins/log-arena-probe`) is built for the `native` board, whose
//! descriptor states one number, with an `[image.native] env` that states
//! another. Both are READ here rather than restated, and asserted to differ —
//! from each other and from the builtin 16 — so the test cannot pass on a value
//! that leaked from the wrong rung.
//!
//! The same capacity is then asked of the two places an operator sizes the knob
//! from: the BOOT RECORD, read out of the live process exactly as a dump off a
//! board would be (`read-boot-report.py`), and `just mem-report` on the ELF.
//! No compilation here: the fixture is built by `build-test-fixtures`.

use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
};

use nros_tests::{fixtures, fixtures::RequireFixture, output, project_root};

/// The loggers the probe creates (`bins/log-arena-probe/src/main.rs`).
const PROBE_LOGGERS: usize = 5;

/// The crate's builtin, which neither rung may coincide with.
const BUILTIN: usize = 16;

fn read_toml(rel: &str) -> toml::Value {
    let path = project_root().join(rel);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    toml::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

/// `[board.knobs.log] dynamic_loggers` of the `native` board.
fn board_statement() -> usize {
    let v = read_toml("packages/boards/linux/nros-board.toml");
    let boards = v["board"]
        .as_array()
        .expect("`[[board]]` in the native board file");
    let n = boards
        .iter()
        .find(|b| {
            b["names"]
                .as_array()
                .is_some_and(|names| names.iter().any(|n| n.as_str() == Some("native")))
        })
        .expect("the `native` board")["knobs"]["log"]["dynamic_loggers"]
        .as_integer()
        .expect("the native board states `[board.knobs.log] dynamic_loggers`");
    usize::try_from(n).expect("a count")
}

/// `NROS_LOG_DYNAMIC_LOGGERS` in the probe's `[image.native] env`.
fn image_statement() -> usize {
    let v = read_toml("packages/testing/nros-tests/bins/log-arena-probe/system.toml");
    v["image"]["native"]["env"]["NROS_LOG_DYNAMIC_LOGGERS"]
        .as_str()
        .expect("the probe image states NROS_LOG_DYNAMIC_LOGGERS")
        .parse()
        .expect("a count")
}

/// The record's address in the LIVE process: the ELF's symbol address, plus the
/// load base when the executable is position-independent (a host Rust binary
/// is PIE by default; `/proc/<pid>/maps` names where it was mapped).
fn runtime_address(pid: u32, elf: &Path, sym_addr: u64) -> u64 {
    let header = std::fs::read(elf).expect("read the probe ELF");
    // e_type at offset 16, little-endian on every host this runs on: 3 = ET_DYN.
    let et_dyn = header.get(16..18) == Some(&[3, 0]);
    if !et_dyn {
        return sym_addr;
    }
    let exe = std::fs::canonicalize(elf).expect("canonical probe path");
    let maps = std::fs::read_to_string(format!("/proc/{pid}/maps")).expect("read maps");
    let base = maps
        .lines()
        .find_map(|l| {
            let mut f = l.split_whitespace();
            let range = f.next()?;
            let _perms = f.next()?;
            let offset = f.next()?;
            let path = f.nth(2)?;
            (offset.trim_start_matches('0').is_empty() && Path::new(path) == exe).then(|| {
                u64::from_str_radix(range.split('-').next().unwrap(), 16).expect("hex base")
            })
        })
        .unwrap_or_else(|| panic!("{} is not mapped in /proc/{pid}/maps", exe.display()));
    base + sym_addr
}

/// `(succeeded, stdout, stdout + stderr)`. The decoder runs its self-test on
/// every invocation and says so on stderr, so a value is parsed from stdout
/// alone and the combined text is what a failure message shows.
fn python(script: &str, args: &[&std::ffi::OsStr]) -> (bool, String, String) {
    let out = Command::new("python3")
        .arg(project_root().join(script))
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("run {script}: {e}"));
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

#[test]
fn an_image_env_outranks_the_board_for_the_runtime_logger_arena() {
    let board = board_statement();
    let image = image_statement();
    assert_ne!(
        board, image,
        "the board and the image state the same number, so the result could not say which won"
    );
    assert_ne!(
        image, BUILTIN,
        "the image states the builtin, which hides a dropped rung"
    );
    assert_ne!(
        board, BUILTIN,
        "the board states the builtin, which hides a dropped rung"
    );

    let bin = fixtures::build_log_arena_probe().require("prebuilt log-arena-probe");

    let mut child = Command::new(bin)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap_or_else(|e| panic!("spawn {}: {e}", bin.display()));
    let pid = child.id();
    let mut line = String::new();
    BufReader::new(child.stdout.take().expect("piped stdout"))
        .read_line(&mut line)
        .expect("read the probe's line");

    // 1. The binary's own reading.
    let rest = line
        .trim()
        .strip_prefix(output::LOG_ARENA_PROBE_LINE)
        .unwrap_or_else(|| panic!("unexpected probe output: {line:?}"));
    let field = |k: &str| -> usize {
        rest.split_whitespace()
            .find_map(|kv| kv.strip_prefix(&format!("{k}=")))
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| panic!("no `{k}=` in {line:?}"))
    };
    assert_eq!(
        field("capacity"),
        image,
        "the image's `[image.native] env` NROS_LOG_DYNAMIC_LOGGERS={image} did not win over the \
         board's {board} (a capacity of {board} means the APP rung was dropped; {BUILTIN} means \
         both were)"
    );
    assert_eq!(field("in_use"), PROBE_LOGGERS);

    // 2. The boot record, read out of the live process as a board dump would be.
    let (ok, addr_line, all) = python(
        "scripts/read-boot-report.py",
        &["--addr-only".as_ref(), bin.as_os_str()],
    );
    assert!(
        ok,
        "no boot record in the probe (NROS_BOOT_REPORT=1 is in its image env):\n{all}"
    );
    let (addr, len) = addr_line
        .trim()
        .split_once(' ')
        .map(|(a, l)| {
            (
                u64::from_str_radix(a.trim_start_matches("0x"), 16).expect("hex address"),
                l.parse::<usize>().expect("length"),
            )
        })
        .unwrap_or_else(|| panic!("unreadable --addr-only line {addr_line:?}"));
    let blob = nros_tests::zephyr::read_process_memory(pid, runtime_address(pid, bin, addr), len)
        .unwrap_or_else(|e| panic!("read the boot record: {e}"));
    let dump = tempfile::NamedTempFile::new().expect("temp file");
    std::fs::write(dump.path(), &blob).expect("write the dump");
    let (_, _, report) = python(
        "scripts/read-boot-report.py",
        &[bin.as_os_str(), dump.path().as_os_str()],
    );
    let want = format!("{PROBE_LOGGERS} of {image} slots");
    assert!(
        report.lines().any(|l| l
            .trim_start()
            .starts_with(output::BOOT_REPORT_RUNTIME_LOGGERS)
            && l.contains(&want)),
        "the boot record does not say `{want}`:\n{report}"
    );

    // 3. `just mem-report`, from the ELF alone.
    let (ok, _, mem) = python("scripts/nros-mem-report.py", &[bin.as_os_str()]);
    assert!(ok, "mem-report failed:\n{mem}");
    let want = format!(
        "{}{image} loggers",
        output::MEM_REPORT_RUNTIME_LOGGER_CAPACITY
    );
    assert!(
        mem.lines()
            .any(|l| l.trim_start().starts_with(want.trim_start())),
        "mem-report does not state `{want}`:\n{mem}"
    );

    drop(child.stdin.take());
    let _ = child.wait();
}

/// issue 1037 — the rest of the logging tenant rides the same ladder, and the
/// platform clock comes from the platform's CAPABILITY.
///
/// The probe's image env states every `NROS_LOG_*` knob at a non-builtin
/// value; its line reads each one back from the BUILT binary. `clock=1`
/// with no `platform-clock` feature anywhere in the leaf's graph is the
/// capability path: the `native` board's platform is `posix`, whose
/// `nros-platform.toml` declares `[capabilities] clock = true`.
#[test]
fn every_log_knob_is_read_back_from_the_image_env() {
    let v = read_toml("packages/testing/nros-tests/bins/log-arena-probe/system.toml");
    let env = &v["image"]["native"]["env"];
    let stated = |k: &str| -> String {
        env[k]
            .as_str()
            .unwrap_or_else(|| panic!("the probe image states {k}"))
            .to_string()
    };
    // Builtins (packages/core/nros-log/build.rs), which no statement may equal.
    let want: [(&str, &str, usize, usize); 4] = [
        (
            "max_level",
            "NROS_LOG_MAX_LEVEL",
            ["trace", "debug", "info", "warn", "error", "fatal", "off"]
                .iter()
                .position(|l| *l == stated("NROS_LOG_MAX_LEVEL"))
                .expect("a level name"),
            0,
        ),
        (
            "buffer",
            "NROS_LOG_BUFFER_SIZE",
            stated("NROS_LOG_BUFFER_SIZE").parse().expect("a count"),
            256,
        ),
        (
            "early",
            "NROS_LOG_EARLY_RECORDS",
            stated("NROS_LOG_EARLY_RECORDS").parse().expect("a count"),
            4,
        ),
        (
            "rosout",
            "NROS_LOG_ROSOUT_RECORDS",
            stated("NROS_LOG_ROSOUT_RECORDS").parse().expect("a count"),
            16,
        ),
    ];

    let bin = fixtures::build_log_arena_probe().require("prebuilt log-arena-probe");
    let mut child = Command::new(bin)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap_or_else(|e| panic!("spawn {}: {e}", bin.display()));
    let mut line = String::new();
    BufReader::new(child.stdout.take().expect("piped stdout"))
        .read_line(&mut line)
        .expect("read the probe's line");
    drop(child.stdin.take());
    let _ = child.wait();

    let rest = line
        .trim()
        .strip_prefix(output::LOG_ARENA_PROBE_LINE)
        .unwrap_or_else(|| panic!("unexpected probe output: {line:?}"));
    let field = |k: &str| -> usize {
        rest.split_whitespace()
            .find_map(|kv| kv.strip_prefix(&format!("{k}=")))
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| panic!("no `{k}=` in {line:?}"))
    };
    for (key, knob, value, builtin) in want {
        assert_ne!(
            value, builtin,
            "the probe states {knob} at its builtin, which hides a dropped rung"
        );
        assert_eq!(
            field(key),
            value,
            "{knob}: the image's `[image.native] env` states {value}, the binary reads back \
             {} ({builtin} is the builtin, i.e. the APP rung was dropped)",
            field(key)
        );
    }
    assert_eq!(
        field("clock"),
        1,
        "the posix platform declares `[capabilities] clock = true` and the probe links its \
         port, so the platform clock must be compiled in WITHOUT the `platform-clock` feature \
         (issue 1037)"
    );
}
