#!/usr/bin/env python3
"""issue 1333 — a graph-reading `ros2` command must not consult the daemon.

The ros2cli daemon is keyed on `ROS_DOMAIN_ID` ALONE. That is not a paraphrase:
`ros2cli.daemon.get_port()` is literally `11511 + ROS_DOMAIN_ID`, and
`spawn_daemon` reuses whatever is already bound to that port without checking
anything else about it.

What is NOT in that key is the DISCOVERY CONFIGURATION. `CYCLONEDDS_URI`,
`FASTRTPS_DEFAULT_PROFILES_FILE` and `ZENOH_SESSION_CONFIG_URI` are captured from
whichever process happened to start the daemon, and every later caller on that
domain is served a graph computed under them instead of its own. Since issue
1009 this repo pins the interop bus with exactly those variables, per PROCESS,
into a tempdir that is deleted when that test ends — so the configuration that
leaks is guaranteed to differ from the caller's, and may name a file that no
longer exists.

Measured on Humble (issue 1333), both polarities, deterministic:

  * daemon primed under an ISOLATING `CYCLONEDDS_URI`, caller unrestricted:
    `ros2 node list` prints nothing and `ros2 param list <node>` prints
    `Node not found` — for a talker that is live on that domain and that the
    very same command finds with `--no-daemon`. A false NEGATIVE.
  * daemon primed unrestricted, caller isolating: the daemon reports a node the
    caller's own configuration cannot reach. A false POSITIVE.

The cost of the false negative is not the bug, it is the DIAGNOSIS: one
investigation of a parameters example read it as "the example is broken" four
consecutive times. Same class as issue 1009 — an environment fact that makes a
correct system report as broken, where the wrong answer costs more than the
fault would have.

What this checks
----------------
A line that CONSTRUCTS a shell command running a daemon-consulting `ros2` verb
must carry `--no-daemon`.

"Constructs" is keyed on a shell marker in the POSITION a shell would put it:
a chain or launcher before the verb (`&&`, `timeout `, `$(`) or a redirect after
it (`2>&1`, `2>/dev/null`, `| tee`). That is what separates a command from prose,
and it matters: a bare `ros2 <verb>` grep over this repo reports ~750 lines,
almost all of them assertion messages, doc comments and issue titles that quote
the command they are about. Positioned, the same sweep reports single digits and
every one is a real construction site. A gate nobody can read the output of is a
gate nobody runs.

Position rather than mere presence because the unpositioned rule was written
first and this file's own self-test rejected it: `panic!("ros2 node list failed:
{e} && ...")` carries a marker, after the command it quotes, and is prose.

The verb list is the set that goes through ros2cli's `NodeStrategy`, i.e. the
ones that consult the daemon at all. `ros2 topic echo` / `topic pub` /
`service call` / `action send_goal` create their own node and are deliberately
absent.

The one verb that CANNOT comply
-------------------------------
`ros2 action list` answers `error: unrecognized arguments: --no-daemon` on
Humble (measured; its `-h` never mentions the flag). It is allowlisted, and the
defence for it is one layer down: `nros_tests::unique_ros_domain_id` refuses a
domain whose daemon port is already bound, so the daemon such a call consults is
one no other test primed. That probe is ALSO issue 1333 — its predecessor read
only the SPDP port `7400+250*d`, which a zenoh daemon never binds, so under this
project's default RMW a lingering daemon was invisible to it.

Dependency-free Python 3.10, house style per `scripts/check-ros-env-spelling.py`.
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Verbs that reach ros2cli's daemon (`NodeStrategy`). Ordered longest-first so
# `param describe` cannot be matched as `param` plus noise.
DAEMON_VERBS = (
    "node list", "node info",
    "topic list", "topic info",
    "service list", "service info",
    "action list", "action info",
    "param list", "param get", "param set", "param describe", "param dump",
    "lifecycle nodes", "lifecycle get", "lifecycle set", "lifecycle list",
)

# `ros2`, optional flags, then a verb. The flag run is what lets
# `ros2 --use-python-default-buffering node list` match.
VERB_RE = re.compile(
    r"ros2\s+(?:--?[\w-]+\s+)*(?:" +
    "|".join(v.replace(" ", r"\s+") for v in DAEMON_VERBS) + r")"
)

# A line that RUNS a command carries a marker in the position a shell would put
# it: a chain/launcher BEFORE the verb, or a redirect AFTER it. Position is what
# makes this precise rather than merely suggestive — an assertion message that
# happens to contain `&&` puts it after the command it quotes and carries no
# redirect, so it is excluded, while every real construction site in this tree
# opens with `{env} &&` or `timeout `. (The unpositioned version of this rule
# was written first and its own self-test rejected it.)
SHELL_BEFORE = ("&&", "timeout ", "$(")
SHELL_AFTER = ("2>&1", "2>/dev/null", "| tee", "|tee")

SCANNED_SUFFIXES = (".rs", ".sh", ".bash", ".py", ".just", ".yml", ".yaml")

# Never scanned: build output, vendored trees, generated code.
EXCLUDED_PARTS = ("third-party/", "/generated/", "/build/", "/target/", "build-")

COMMENT_STARTS = ("//", "///", "//!", "#!", "* ", "*/")

REMEDY = (
    "`nros_tests::ros2::ros2_query_cmd(&env_setup, <secs>, \"<verb> <args>\")` "
    "from Rust — it appends `--no-daemon` so the flag is decided once. From "
    "shell, write the flag on the command."
)

CONSEQUENCE = (
    "a daemon another run left on this domain answers from ITS discovery "
    "config, so a live node reads as `Node not found` (or a dead one reads as "
    "present) and the test blames the code under test"
)

# Sites that cannot comply, path-keyed, one reason each. Growing this is the
# reviewable act the gate exists to force.
ALLOWLIST = {
    "packages/testing/nros-tests/tests/ros2_action_e2e.rs":
        "`ros2 action list` REJECTS `--no-daemon` on Humble "
        "(`error: unrecognized arguments`), so this site cannot comply and is "
        "defended by `unique_ros_domain_id` refusing a domain whose daemon "
        "port is bound. Passing the flag here is not a stricter choice, it is "
        "a usage error that polls for 20 s and then reports a DISCOVERY "
        "timeout — which once cost a full box run reading as an actions defect",

    "scripts/check-ros-env-spelling.py":
        "the sibling gate's self-test SPECIMENS: a fixture string that quotes "
        "a `ros2 topic list` command, not a command anything runs",

    # This file. Its own self-test fixtures are specimens of what it forbids —
    # a rule that cannot state what it rejects cannot be tested. Listed for the
    # reason the sibling gate records: `git ls-files` does not list an untracked
    # file, so a gate's first honest run is the one AFTER it is committed.
    "scripts/check-ros2-daemon-queries.py":
        "this gate's own self-test specimens",
}


def tracked_files():
    out = subprocess.run(
        ["git", "ls-files"], cwd=ROOT, capture_output=True, text=True, check=True
    )
    return out.stdout.splitlines()


def scannable(path):
    if not path.endswith(SCANNED_SUFFIXES):
        return False
    return not any(part in path for part in EXCLUDED_PARTS)


def scan_text(text):
    """Yield (lineno, line) for every construction site missing the flag."""
    for lineno, raw in enumerate(text.splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith(COMMENT_STARTS):
            continue
        if "--no-daemon" in line:
            continue
        m = VERB_RE.search(line)
        if not m:
            continue
        before, after = line[: m.start()], line[m.end() :]
        if not (any(t in before for t in SHELL_BEFORE)
                or any(t in after for t in SHELL_AFTER)):
            continue
        yield lineno, line


def self_test(quiet=False):
    # (1) The shapes the gate must CATCH.
    caught = [
        'let c = format!("{env} && timeout 10 ros2 node list 2>&1");',
        'timeout 30 ros2 param list /talker 2>&1 | tee "$OUT/list.log"',
        '.run_text("timeout --foreground 10 ros2 topic info /chatter 2>&1")',
        'out=$(ros2 service list 2>/dev/null)',
        'let c = format!("{env} && ros2 lifecycle nodes 2>&1");',
    ]
    for src in caught:
        assert list(scan_text(src)), f"missed a construction site: {src}"

    # (2) The shapes it must NOT catch.
    ignored = [
        # compliant
        'let c = format!("{env} && timeout 10 ros2 node list --no-daemon 2>&1");',
        'timeout 30 ros2 param get /n p --no-daemon 2>&1 | tee "$OUT/g.log"',
        # prose: assertion messages and panics quoting the command
        'panic!("ros2 node list failed: {e} && the graph was empty");',
        'assert!(x, "`ros2 param list` should enumerate {n}. Output:\\n{all}");',
        # comments
        '// `ros2 topic list` queries the graph via XML-RPC && the daemon',
        '/// Run `ros2 node info` for a node && return the output',
        '# echo "=== ros2 param list ===" && explain',
        # a verb-free ros2 command
        'let c = format!("{env} && timeout 10 ros2 topic echo /chatter 2>&1");',
        'let c = format!("{env} && timeout 10 ros2 run demo_nodes_cpp talker");',
        # no shell marker: a bare mention with a verb
        'let label = "ros2 node list";',
    ]
    for src in ignored:
        assert not list(scan_text(src)), f"false positive on: {src}"

    # (3) The discriminator is POSITION, not mere presence — prove each half
    #     moves, or the rule is really "match everything".
    assert list(scan_text('x && ros2 node list'))          # marker before
    assert list(scan_text('ros2 node list 2>&1'))          # redirect after
    assert not list(scan_text('ros2 node list'))           # neither
    assert not list(scan_text('"ros2 node list failed && gave up"'))
    assert not list(scan_text('x && ros2 bag record 2>&1'))  # not a graph verb

    # (4) Scope filtering.
    for path in ("third-party/x/y.sh", "examples/foo/build/g.sh",
                 "examples/foo/target/x.rs", "docs/notes.md", "a/b.toml",
                 "packages/x/generated/y.rs"):
        assert not scannable(path), f"{path} must not be scanned"
    for path in ("packages/a/b.rs", "scripts/x.sh", "just/check/workflows.just",
                 ".github/workflows/ci.yml"):
        assert scannable(path), f"{path} must be scanned"

    # (5) Every allowlist entry is real and reasoned — a stale exemption can
    #     only ever be reclaimed by accident.
    for path, reason in ALLOWLIST.items():
        assert reason and len(reason) > 20, f"{path}: allowlist entry needs a reason"
        assert (ROOT / path).exists(), f"{path}: allowlisted but no such file"

    if not quiet:
        print("check-ros2-daemon-queries self-test: OK")
    return 0


def main():
    if "--self-test" in sys.argv:
        return self_test()
    self_test(quiet=True)

    findings = []
    scanned = 0
    for path in tracked_files():
        if not scannable(path) or path in ALLOWLIST:
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
            findings.append((path, lineno, line))

    print(f"ros2 daemon queries: {scanned} tracked source files scanned, "
          f"{len(ALLOWLIST)} allowlisted")

    if findings:
        print("\n[FAIL] graph-reading `ros2` command built without "
              "`--no-daemon`:", file=sys.stderr)
        for path, lineno, line in findings:
            print(f"  - {path}:{lineno}", file=sys.stderr)
            print(f"      {line[:160]}", file=sys.stderr)
        print(f"\n  WHAT TO USE INSTEAD: {REMEDY}", file=sys.stderr)
        print(f"\n  WHY IT MATTERS: {CONSEQUENCE}", file=sys.stderr)
        return 1

    print("Every graph-reading `ros2` command passes --no-daemon.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
