#!/usr/bin/env python3
"""issue 1723 — a deadline on a ROS 2 process must escalate to SIGKILL.

`timeout N ros2 ...` does not bound a waiting ros2 CLI. rclpy installs a SIGTERM
handler once `rclpy.init()` returns, and the handler does not end the process:
it triggers rclpy's guard conditions and returns. Over rmw_zenoh_cpp (Humble
0.1.9) the CLI then keeps waiting. Measured 2026-10-07 with a live router, one
SIGTERM to a running `ros2 topic echo --no-daemon`: survived 3 of 3 on zenoh,
ended 3 of 3 on Cyclone and on Fast-DDS.

GNU `timeout` sends SIGTERM and, without `--kill-after`, nothing else — it waits
as long as the child does. `timeout --foreground 5` (the harness's spelling):
3 of 3 echoes still alive at 25 s. Plain `timeout 5` signals the child AND its
own process group, so the CLI usually gets two SIGTERMs and the second one kills
it; usually is not always, and 4 of 10 daemon-spawning echoes were alive at
25 s. The host carried four `timeout 20 ros2 topic echo` peers that had lived
four days, each holding a zenoh session.

What this checks
----------------
A `timeout` in SHELL position whose command is `ros2` or `python3` (every
`python3` this repo bounds that way is an rclpy peer) must not be written
literally. Use the one spelling for the language:

  * Rust — `nros_tests::ros2::ros2_deadline(secs)`, i.e.
    `format!("{env} && {} ros2 ...", ros2_deadline(10))`;
  * shell — `. scripts/lib/ros2-deadline.sh`, then
    `"${NROS_ROS2_DEADLINE[@]}" 30 ros2 ...`.

A literal that already carries `--kill-after` is refused too: the bound is right
but the grace is then a second number, and the next copy drops the flag.

Both spellings state the grace; this gate also holds the two numbers EQUAL.

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

RUST_HELPER = "packages/testing/nros-tests/src/ros2.rs"
SHELL_HELPER = "scripts/lib/ros2-deadline.sh"

SCANNED_SUFFIXES = (".rs", ".sh", ".bash", ".py", ".just", ".yml", ".yaml")
SCANNED_NAMES = ("justfile", "Dockerfile")
EXCLUDED_PARTS = ("third-party/", "/generated/", "/build/", "/target/", "build-")
COMMENT_STARTS = ("//", "#", "* ", "*/")

# Commands that are a ROS 2 process when bounded by `timeout` here.
ROS_COMMANDS = ("ros2", "python3", "python")

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

REMEDY = (
    "Rust: `format!(\"{env} && {} ros2 ...\", nros_tests::ros2::ros2_deadline(N))`. "
    "Shell: `. scripts/lib/ros2-deadline.sh` then `\"${NROS_ROS2_DEADLINE[@]}\" N ros2 ...`."
)
CONSEQUENCE = (
    "rclpy swallows the deadline's SIGTERM (measured over rmw_zenoh_cpp), so "
    "`timeout` waits as long as the CLI does: the cell hangs until something "
    "outside it gives up, and the orphan keeps a session on the bus"
)

ALLOWLIST = {
    ("scripts/check-ros2-cli-deadline.py", "*"):
        "this gate's own self-test specimens, which quote the shapes it refuses",
    ("scripts/check-ros2-daemon-queries.py", "*"):
        "the sibling gate's self-test SPECIMENS: strings that quote a `timeout N "
        "ros2 node list` command to test the `--no-daemon` rule, run by nothing",
}
ALLOW = Exemptions(ALLOWLIST, what="literal ros2 deadline")
ALLOW_NEIGHBOURS = [
    ("scripts/check-ros-env-spelling.py", "*"),
    ("packages/testing/nros-tests/src/ros2.rs", "*"),
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


def scan_text(text):
    """Yield (lineno, line) for every literal deadline on a ROS 2 process."""
    for lineno, line in logical_lines(text):
        if not line or line.startswith(COMMENT_STARTS):
            continue
        for m in re.finditer(r"(?<![\w-])timeout\s+", line):
            if not in_shell_position(line[: m.start()]):
                continue
            cmd = bounded_command(line[m.end():])
            if cmd in ROS_COMMANDS:
                yield lineno, line
                break


def grace_values():
    """The grace as each helper states it, in seconds."""
    rs = (ROOT / RUST_HELPER).read_text(encoding="utf-8")
    sh = (ROOT / SHELL_HELPER).read_text(encoding="utf-8")
    m_rs = re.search(r"ROS2_KILL_GRACE:\s*Duration\s*=\s*Duration::from_secs\((\d+)\)", rs)
    m_sh = re.search(r"^NROS_ROS2_KILL_GRACE_S=(\d+)$", sh, re.M)
    return (int(m_rs.group(1)) if m_rs else None,
            int(m_sh.group(1)) if m_sh else None)


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
    caught = [
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
        # bounded, but a second spelling of the grace
        'timeout --kill-after=3s 30 ros2 node list --no-daemon',
        'timeout -k 3 30 ros2 node list --no-daemon',
    ]
    for src in caught:
        assert list(scan_text(src)), f"missed a literal deadline: {src!r}"

    ignored = [
        # the shared spellings
        'format!("{env} && {} ros2 topic echo /c", ros2_deadline(10))',
        '"${NROS_ROS2_DEADLINE[@]}" 30 ros2 service call /add_two_ints',
        # not a ROS 2 process
        'timeout 6 examples/threadx-linux/c/service-server/build/c_service_server',
        'timeout 15 env LD_LIBRARY_PATH="$X" "$NROS_CLIENT_BIN" > "$OUT" 2>&1',
        'timeout 60 qemu-system-arm -M mps2-an385',
        # prose
        '// `timeout N ros2 ...` does not bound a waiting CLI',
        '# timeout 10 ros2 topic echo /c',
        'panic!("the `timeout 10 ros2 service call` hung");',
        'assert!(ok, "ros2 echo timeout fired before ros2 replied");',
        'let timeout = Duration::from_secs(10); // then ros2 runs',
    ]
    for src in ignored:
        assert not list(scan_text(src)), f"false positive on: {src!r}"

    # Each half of the discriminator moves on its own.
    assert list(scan_text("x && timeout 5 ros2 topic echo /c"))    # shell leader
    assert not list(scan_text("x timeout 5 ros2 topic echo /c")) # prose word before
    assert not list(scan_text("&& timeout fires ros2"))            # no duration
    assert not list(scan_text("&& timeout 5 rosbag"))              # not a ROS 2 command
    # Continuation joining is what catches the split form.
    assert list(scan_text("timeout 5 env A=b \\\n  ros2 topic echo /c"))
    assert not list(scan_text("timeout 5 env A=b\n  ros2 topic echo /c"))

    for path in ("third-party/x.sh", "a/build/x.sh", "a/target/x.rs", "docs/a.md",
                 "packages/x/generated/y.rs"):
        assert not scannable(path), path
    for path in ("packages/a/b.rs", "scripts/x.sh", "just/check/x.just",
                 ".github/workflows/ci.yml", "docker/can-demo/Dockerfile"):
        assert scannable(path), path

    rs, sh = grace_values()
    assert rs is not None, f"{RUST_HELPER}: no `ROS2_KILL_GRACE` found"
    assert sh is not None, f"{SHELL_HELPER}: no `NROS_ROS2_KILL_GRACE_S` found"
    assert ALLOW.check(ALLOW_NEIGHBOURS) == [], ALLOW.check(ALLOW_NEIGHBOURS)
    if not quiet:
        print("check-ros2-cli-deadline self-test: OK")
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
        for lineno, line in scan_text(text):
            if not ALLOW.covers((path, "*")):
                findings.append((path, lineno, line))
    for key in ALLOW.stale():
        findings.append((key[0], 0, f"STALE allowlist entry {key!r}: nothing it names is found"))

    rs, sh = grace_values()
    if rs != sh:
        findings.append((SHELL_HELPER, 0,
                         f"kill grace disagrees: {RUST_HELPER} says {rs}s, "
                         f"{SHELL_HELPER} says {sh}s — one number, two languages"))

    print(f"ros2 CLI deadlines: {scanned} tracked source files scanned, "
          f"kill grace {rs}s (Rust) / {sh}s (shell)")
    if findings:
        print("\n[FAIL] a ROS 2 process bounded by a literal `timeout`:", file=sys.stderr)
        for path, lineno, line in findings:
            print(f"  - {path}:{lineno}", file=sys.stderr)
            print(f"      {line[:160]}", file=sys.stderr)
        print(f"\n  WHAT TO USE INSTEAD: {REMEDY}", file=sys.stderr)
        print(f"\n  WHY IT MATTERS: {CONSEQUENCE}", file=sys.stderr)
        return 1
    print("Every ROS 2 deadline uses the shared spelling, which escalates to SIGKILL.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
