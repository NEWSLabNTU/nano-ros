#!/usr/bin/env python3
"""Issues 1723 + 1741 — a deadline on a process must escalate to SIGKILL.

GNU `timeout` sends SIGTERM and, without `--kill-after`, nothing else — it waits
as long as the child does. So `timeout N <cmd>` bounds nothing for a process
that HANDLES SIGTERM, and two kinds of process we bound do:

* a ROS 2 CLI (issue 1723). rclpy installs a SIGTERM handler once
  `rclpy.init()` returns; over rmw_zenoh_cpp it triggers guard conditions and
  returns, and the waiting CLI keeps waiting. Measured 2026-10-07: one SIGTERM
  to a running `ros2 topic echo --no-daemon` survived 3 of 3 on zenoh. The host
  carried four `timeout 20 ros2 topic echo` peers that had lived four days.
* a nano-ros IMAGE (issue 1741). The threadx-linux C examples install
  `signal(SIGTERM, handler)`; when the image's RTOS scheduler is wedged the
  graceful shutdown that handler starts never runs. One
  `timeout 6 …/c_service_server` lived 34 days. The image now ends itself
  (`nros_threadx_linux_install_termination_guard`), but a deadline must not
  depend on the process it bounds being correct — and an image's "handler"
  is user code, in any of five RTOS simulations, written by anyone.

What this checks
----------------
1. A literal `timeout` in SHELL position, whatever it bounds, unless the bounded
   command is a host tool in SAFE_COMMANDS (it ends on SIGTERM and leaves
   nothing behind) or the site is in the exemption table WITH A REASON. The rule
   is FAIL-CLOSED on the command: `"$bin"`, `"$CLIENT"` and `zephyr.exe` are all
   images, and a syntactic check cannot tell which variables are. A literal
   that already carries `--kill-after` is refused too: the bound is right but
   the grace is then a second number, and the next copy drops the flag.
2. `Command::new("timeout")` in Rust — the second shape the same literal took
   (`.args(["6", bin])` bounded a native entry four times) — outside the helper.
3. A RETIRED spelling (`NROS_ROS2_DEADLINE`, `ros2-deadline.sh`,
   `ros2_deadline(`, `ROS2_KILL_GRACE`). It is a rename, and a shell array
   that is no longer defined expands to NOTHING: a racing branch still writing
   `"${NROS_ROS2_DEADLINE[@]}" 30 ros2 …` would run `30 ros2 …` and fail, or
   worse, an unbounded command — so the old names are refused, not left to rot.
4. The grace is ONE number in two languages (held equal), and the image's own
   termination grace is BELOW it, so an image ends by its own hand before any
   harness escalation has to fire.

Use the one spelling for the language and shape:

  * Rust, shell string — `nros_tests::process::deadline(secs)`, i.e.
    `format!("{env} && {} ros2 ...", deadline(10))`;
  * Rust, direct spawn — `nros_tests::process::deadline_command(secs, &bin)`;
  * shell — `. scripts/lib/deadline.sh`, then `"${NROS_DEADLINE[@]}" 30 <cmd>`.

"Shell position" is what separates a command from prose: `timeout` at the start
of a logical line, or after `&&` `||` `;` `|` `(` `$(` `"` `'` `exec`
`run_bg` `then` `do`, or after an env assignment (`X=y timeout ...`). A
backslash-continued line is joined first, so `timeout 15 env A=b \\` + `ros2 ...`
is one command. Comment lines are skipped.

Dependency-free Python 3.10, house style per `scripts/check-ros2-daemon-queries.py`.
"""

import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
from exemptions import Exemptions  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent

RUST_HELPER = "packages/testing/nros-tests/src/process.rs"
SHELL_HELPER = "scripts/lib/deadline.sh"
IMAGE_GUARD = "packages/boards/nros-board-threadx-linux/c/board_threadx_linux.c"

SCANNED_SUFFIXES = (".rs", ".sh", ".bash", ".py", ".just", ".yml", ".yaml")
SCANNED_NAMES = ("justfile", "Dockerfile")
EXCLUDED_PARTS = ("third-party/", "/generated/", "/build/", "/target/", "build-")
COMMENT_STARTS = ("//", "#", "* ", "*/")

# Host tools that end on SIGTERM and spawn nothing that outlives them. Adding a
# name here is a claim about that program; an IMAGE never belongs here.
SAFE_COMMANDS = ("rustup", "ninja", "cargo", "sleep")

# Tokens `timeout` may sit behind and still be the command word.
SHELL_LEADERS = ("&&", "||", ";", "|", "(", "$(", '"', "'", "exec", "run_bg",
                 "then", "do", "{", "!")
# Tokens between the duration and the command that do not change which program
# runs: `env`, `stdbuf -oL`, `setsid`, `nice`, and `VAR=value` assignments.
PASS_THROUGH = ("env", "stdbuf", "setsid", "nice", "exec")
ASSIGN_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*=")
# A duration: `10`, `10s`, `{secs}`, `{timeout_s}`, `"$X"`, `$X`, `${X}`, `{{X}}`.
DURATION_RE = re.compile(r"""^["']?(\d+(\.\d+)?[smhd]?|\{[^}]*\}+|\$\{?\w+\}?)["']?$""")
# Options of `timeout` that take a separate argument.
OPT_WITH_ARG = ("-k", "-s", "--kill-after", "--signal")

COMMAND_TIMEOUT_RE = re.compile(r'Command::new\(\s*"timeout"\s*\)')
RETIRED = ("NROS_ROS2_DEADLINE", "NROS_ROS2_KILL_GRACE_S", "ros2-deadline.sh",
           "ros2_deadline(", "ROS2_KILL_GRACE")

REMEDY = (
    "Rust: `format!(\"{env} && {} <cmd>\", nros_tests::process::deadline(N))`, or "
    "`nros_tests::process::deadline_command(N, &bin)` for a direct spawn. "
    "Shell: `. scripts/lib/deadline.sh` then `\"${NROS_DEADLINE[@]}\" N <cmd>`."
)
CONSEQUENCE = (
    "a process that handles SIGTERM outlives a single-signal `timeout` — rclpy "
    "over zenoh (issue 1723) and a wedged nano-ros image (issue 1741, one lived "
    "34 days) both did — so the cell hangs until something outside it gives up, "
    "and the orphan keeps its session on the bus"
)

# Keyed on (path, bounded command): the reason is about THAT command in THAT
# file, and the same file bounding anything else is not covered.
ALLOWLIST = {
    ("scripts/check-process-deadline.py", "*"):
        "this gate's own self-test specimens, which quote the shapes it refuses",
    ("scripts/check-ros2-daemon-queries.py", "*"):
        "the sibling gate's self-test SPECIMENS: strings that quote a `timeout N "
        "ros2 node list` command to test the `--no-daemon` rule, run by nothing",
    ("packages/testing/nros-tests/src/process.rs", "bash"):
        "the orphan-ledger tests' MARKER process: a `timeout --foreground 120 "
        "bash -c 'sleep <marker> & wait'` tree whose exact shape (and the "
        "missing kill-after) is what the test measures; it is reaped by the "
        "test's own group kill",
    ("scripts/check-hook-repo-side-effects.sh", "bash"):
        "runs OUR pre-push hook script, a plain bash script with no SIGTERM "
        "trap, inside the gate's own probe wrapper",
    ("scripts/ci/runner-container.sh", "bash"):
        "a Dockerfile `RUN` that smoke-starts the router at image BUILD time: "
        "no checkout exists in the build context to source the helper from, and "
        "`docker build` itself is the outer bound",
}
ALLOW = Exemptions(ALLOWLIST, what="literal deadline")
ALLOW_NEIGHBOURS = [
    ("scripts/check-ros-env-spelling.py", "*"),
    ("packages/testing/nros-tests/src/process.rs", "c_service_server"),
    ("packages/testing/nros-tests/src/ros2.rs", "*"),
    ("scripts/check-hook-repo-side-effects.sh", "ros2"),
    ("scripts/ci/runner-container.sh", "zephyr.exe"),
]


def logical_lines(text):
    """Yield (first_lineno, joined_line) with backslash continuations joined."""
    buf, start = [], None
    for lineno, raw in enumerate(text.splitlines(), 1):
        if start is None:
            start = lineno
        stripped = raw.rstrip()
        if stripped.endswith("\\"):
            buf.append(stripped[:-1].strip())
            continue
        buf.append(stripped.strip())
        yield start, " ".join(b for b in buf if b)
        buf, start = [], None
    if buf:
        yield start, " ".join(b for b in buf if b)


def in_shell_position(before):
    b = before.rstrip()
    if not b:
        return True
    if any(b.endswith(t) for t in SHELL_LEADERS):
        return True
    last = b.split()[-1]
    return bool(ASSIGN_RE.match(last)) and "`" not in last


def bounded_command(rest):
    """`rest` follows `timeout `. Return the command word it runs, or None when
    the text is not a `timeout` invocation at all (prose)."""
    toks = rest.split()
    i = 0
    while i < len(toks) and toks[i].startswith("-"):
        t = toks[i]
        i += 2 if (t in OPT_WITH_ARG) else 1
    if i >= len(toks) or not DURATION_RE.match(toks[i]):
        return None
    i += 1
    while i < len(toks):
        t = toks[i].strip("\"'")
        if t in PASS_THROUGH or ASSIGN_RE.match(t):
            i += 1
            while i < len(toks) and toks[i].startswith("-"):
                i += 1
            continue
        return t.rsplit("/", 1)[-1]
    return None


def scan_text(text, rust=False):
    """Yield (lineno, command, line) for every literal deadline that does not
    escalate. `command` is the bounded program, or a tag for the non-shell
    shapes (`Command::new("timeout")`, a retired spelling)."""
    for lineno, line in logical_lines(text):
        if not line or line.startswith(COMMENT_STARTS):
            continue
        retired = next((r for r in RETIRED if r in line), None)
        if retired:
            yield lineno, f"retired:{retired}", line
            continue
        if rust and COMMAND_TIMEOUT_RE.search(line):
            yield lineno, "Command::new(\"timeout\")", line
            continue
        for m in re.finditer(r"(?<![\w-])timeout\s+", line):
            if not in_shell_position(line[: m.start()]):
                continue
            cmd = bounded_command(line[m.end():])
            if cmd is not None and cmd not in SAFE_COMMANDS:
                yield lineno, cmd, line
                break


def grace_values():
    """(Rust harness grace s, shell harness grace s, image grace ms)."""
    rs = (ROOT / RUST_HELPER).read_text(encoding="utf-8")
    sh = (ROOT / SHELL_HELPER).read_text(encoding="utf-8")
    img = (ROOT / IMAGE_GUARD).read_text(encoding="utf-8")
    m_rs = re.search(r"pub const KILL_GRACE:\s*Duration\s*=\s*Duration::from_secs\((\d+)\)", rs)
    m_sh = re.search(r"^NROS_KILL_GRACE_S=(\d+)$", sh, re.M)
    m_img = re.search(r"^#define NROS_THREADX_LINUX_TERM_GRACE_MS (\d+)$", img, re.M)
    return (int(m_rs.group(1)) if m_rs else None,
            int(m_sh.group(1)) if m_sh else None,
            int(m_img.group(1)) if m_img else None)


def scannable(path):
    name = path.rsplit("/", 1)[-1]
    if not (path.endswith(SCANNED_SUFFIXES) or name in SCANNED_NAMES):
        return False
    return not any(part in path for part in EXCLUDED_PARTS)


def tracked_files():
    out = subprocess.run(["git", "ls-files"], cwd=ROOT, capture_output=True,
                         text=True, check=True)
    return out.stdout.splitlines()


def self_test(quiet=False):
    def hits(src, rust=False):
        return list(scan_text(src, rust=rust))

    caught = [
        # ROS 2 processes (issue 1723)
        'let script = format!("{env} && timeout 10 ros2 {subcommand} 2>&1");',
        '"{env_setup} && timeout --foreground 15 ros2 service list --no-daemon 2>&1"',
        '"timeout {timeout_s} ros2 topic echo --once {topic} {ros_type} 2>&1"',
        '.run("timeout 60 ros2 action send_goal /fibonacci x \'{order: 5}\'")',
        'timeout "$NROS_E2E_DEADLINE_S" env LD_LIBRARY_PATH="$X" \\\n    ros2 topic echo /c',
        'X="$LOGS/a.json5" timeout 30 ros2 service call /add_two_ints \\\n  t "{a: 1}"',
        '    exec timeout 60 ros2 topic echo /chatter std_msgs/msg/String',
        'run_bg timeout 90 ros2 action send_goal --feedback /fibonacci \\',
        '"{env_setup} && timeout {secs} stdbuf -oL \\\n ros2 topic hz {topic} 2>&1"',
        '"{env_setup} && timeout --foreground 60 python3 -u - 2>&1 <<\'EOF\'"',
        'format!("timeout {timeout_s} python3 - <<\'NROS_PYEOF\'\\n{script}")',
        # nano-ros images and emulators (issue 1741) — fail-closed on the command
        'timeout 6 examples/threadx-linux/c/service-server/build/c_service_server',
        'timeout 15 env LD_LIBRARY_PATH="$X" "$NROS_CLIENT_BIN" > "$OUT" 2>&1',
        'timeout 60 qemu-system-arm -M mps2-an385',
        '    timeout 15 ./tests/zephyr-c-smoke/build/zephyr/zephyr.exe',
        'RUST_LOG=info timeout 120 "./$bin" >/tmp/talker.log 2>&1 &',
        '"timeout 60 {}",',
        # bounded, but a second spelling of the grace
        'timeout --kill-after=3s 30 ros2 node list --no-daemon',
        'timeout -k 3 30 "$bin"',
        # retired spellings of the helper
        '"${NROS_ROS2_DEADLINE[@]}" 30 ros2 service call /add_two_ints',
        '. "$REPO_ROOT/scripts/lib/ros2-deadline.sh"',
        'let d = nros_tests::ros2::ros2_deadline(10);',
    ]
    for src in caught:
        assert hits(src), f"missed a literal deadline: {src!r}"
    rust_caught = [
        'let out = Command::new("timeout")\n        .args(["6", bin.to_str().unwrap()])',
        'std::process::Command::new( "timeout" ).arg("60").arg(&client_bin)',
    ]
    for src in rust_caught:
        assert hits(src, rust=True), f"missed a Rust timeout spawn: {src!r}"
        assert not hits(src, rust=False), f"Rust shape matched outside Rust: {src!r}"

    ignored = [
        # the shared spellings
        'format!("{env} && {} ros2 topic echo /c", deadline(10))',
        '"${NROS_DEADLINE[@]}" 30 ros2 service call /add_two_ints',
        '"${NROS_DEADLINE[@]}" 15 ./tests/zephyr-c-smoke/build/zephyr/zephyr.exe',
        'let out = nros_tests::process::deadline_command(6, &bin).output()',
        # a host tool that ends on SIGTERM
        'list_out="$(timeout 5s rustup toolchain list 2>/dev/null)"',
        'out="$(timeout 120 ninja -C "$build" 2>&1)"',
        # prose
        '// `timeout N ros2 ...` does not bound a waiting CLI',
        '# timeout 10 ros2 topic echo /c',
        'panic!("the `timeout 10 ros2 service call` hung");',
        'assert!(ok, "ros2 echo timeout fired before ros2 replied");',
        'let timeout = Duration::from_secs(10); // then ros2 runs',
        'let t = Command::new("timeout_helper");',
    ]
    for src in ignored:
        assert not hits(src, rust=True), f"false positive on: {src!r}"

    # Each half of the discriminator moves on its own.
    assert hits("x && timeout 5 ros2 topic echo /c")       # shell leader
    assert not hits("x timeout 5 ros2 topic echo /c")      # prose word before
    assert not hits("&& timeout fires ros2")               # no duration
    assert not hits("&& timeout 5 rustup show")            # safe host tool
    assert hits("&& timeout 5 rosbag")                     # anything else: fail-closed
    # Continuation joining is what catches the split form.
    assert hits("timeout 5 env A=b \\\n  ros2 topic echo /c")
    assert not hits("timeout 5 env A=b\n  ros2 topic echo /c")

    for path in ("third-party/x.sh", "a/build/x.sh", "a/target/x.rs", "docs/a.md",
                 "packages/x/generated/y.rs"):
        assert not scannable(path), path
    for path in ("packages/a/b.rs", "scripts/x.sh", "just/check/x.just",
                 ".github/workflows/ci.yml", "docker/can-demo/Dockerfile"):
        assert scannable(path), path

    rs, sh, img = grace_values()
    assert rs is not None, f"{RUST_HELPER}: no `pub const KILL_GRACE` found"
    assert sh is not None, f"{SHELL_HELPER}: no `NROS_KILL_GRACE_S` found"
    assert img is not None, f"{IMAGE_GUARD}: no `NROS_THREADX_LINUX_TERM_GRACE_MS` found"
    assert ALLOW.check(ALLOW_NEIGHBOURS) == [], ALLOW.check(ALLOW_NEIGHBOURS)
    if not quiet:
        print("check-process-deadline self-test: OK")
    return 0


def main():
    if "--self-test" in sys.argv:
        return self_test()
    self_test(quiet=True)

    findings = []
    scanned = 0
    for path in tracked_files():
        if not scannable(path):
            continue
        full = ROOT / path
        if not full.is_file():
            continue
        try:
            text = full.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        scanned += 1
        rust = path.endswith(".rs") and path != RUST_HELPER
        for lineno, cmd, line in scan_text(text, rust=rust):
            if ALLOW.covers((path, "*")) or ALLOW.covers((path, cmd)):
                continue
            findings.append((path, lineno, cmd, line))
    for key in ALLOW.stale():
        findings.append((key[0], 0, "-", f"STALE allowlist entry {key!r}: nothing it names is found"))

    rs, sh, img = grace_values()
    if rs != sh:
        findings.append((SHELL_HELPER, 0, "-",
                         f"kill grace disagrees: {RUST_HELPER} says {rs}s, "
                         f"{SHELL_HELPER} says {sh}s — one number, two languages"))
    if img is None or rs is None or img >= rs * 1000:
        findings.append((IMAGE_GUARD, 0, "-",
                         f"the image's own termination grace ({img} ms) must be BELOW "
                         f"the harness kill grace ({rs} s), so an image ends by its own "
                         f"hand before any escalation fires"))

    print(f"process deadlines: {scanned} tracked source files scanned, "
          f"kill grace {rs}s (Rust) / {sh}s (shell), image grace {img} ms")
    if findings:
        print("\n[FAIL] a process bounded by a deadline that does not escalate:",
              file=sys.stderr)
        for path, lineno, cmd, line in findings:
            print(f"  - {path}:{lineno}  [{cmd}]", file=sys.stderr)
            print(f"      {line[:160]}", file=sys.stderr)
        print(f"\n  WHAT TO USE INSTEAD: {REMEDY}", file=sys.stderr)
        print(f"\n  WHY IT MATTERS: {CONSEQUENCE}", file=sys.stderr)
        return 1
    print("Every process deadline uses the shared spelling, which escalates to SIGKILL.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
