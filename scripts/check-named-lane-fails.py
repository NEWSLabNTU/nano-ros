#!/usr/bin/env python3
"""phase-411 W2 — a platform you NAMED may not skip its way to green.

# What came before, and is not being undone

Issue 0599 gave a fixture lane a THIRD verdict: `nros_lane_skip` prints a
`NROS_LANE_SKIP:` marker and exits 78 (EX_CONFIG), and the fan-out records
SKIPPED instead of the `exit 0` that used to read as OK. Issue 0650 extended it
to lanes whose STEPS have separate preconditions. Both are correct and both
stay. This gate does not revert either; it adds the one distinction neither
could express.

# The distinction

A skip is legitimate when the platform was INCLUDED — by a preset, or by the
broad default — because the operator asked for a lane and never claimed to have
provisioned every member of it. A developer working on Zephyr does not provision
FreeRTOS, NuttX, ThreadX and ESP-IDF, and a run that demanded all of them is a
run nobody performs.

It is not legitimate when the platform was NAMED. `just zephyr build-fixtures`
on an unprovisioned host prints "ZEPHYR_WORKSPACE not set up — run `just zephyr
setup`" and then exits 78, and the run is green. The user typed `zephyr`. There
is no lane to disambiguate and no second reading of the command line; the
message is already the remedy, it is simply not a skip.

    Named -> must work.  Unnamed -> may skip, and is always reported.

# What this gate holds

1. THE BEHAVIOUR, by running it. `scripts/build/lane-skip.sh` is sourced and
   both directions are exercised for every entry point, on the normal path of
   this script — see `self_test`. A named-platform skip regressing to a pass is
   exactly the mutation the gate exists to catch, so the gate must fail when it
   is made, and the only way to know that is to make it.

2. THE SOURCE LINE. A recipe that calls `nros_lane_*` must `source
   scripts/build/lane-skip.sh` in its own body. Recipes are separate processes,
   so an unsourced call is `command not found` — which `set -e` turns into rc
   127 and a message naming nothing. Three recipes were in this state before
   this gate; `just nuttx build-examples` on a host without NuttX died on
   `nros_lane_skip_note: command not found` instead of reporting the skip its
   author wrote.

3. THE SCOPE DECLARATION. In a platform module, a recipe using the whole-recipe
   `nros_lane_skip` must declare `nros_lane_platform <lane>`, or be listed in
   NOT_A_PLATFORM_LANE with a reason. This is where a MISSED site becomes loud:
   the runtime default is already "named" (see lane-skip.sh), so a forgotten
   declaration cannot silently keep today's behaviour, and a deliberately
   exempt recipe has to say why in this file.

4. THE FAN-OUT SETS THE SIGNAL. `build-test-fixtures-leaves` has two drivers
   (the serial jobserver launcher and the generated make graph) and both must
   set `NROS_LANE_INCLUDED`. If one stops, its platforms become NAMED and go
   red — loud, but the point of asserting it here is that "loud" should not be
   how it is discovered.

Run: python3 scripts/check-named-lane-fails.py
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# phase-472 W3 — ONE comment stripper. Rules 2-4 read what a recipe DOES, and
# comments used to count: a full-line `#` was skipped by a private filter, but a
# TRAILING `true # ; nros_lane_platform px4` still declared the lane, and rule 4
# counted `NROS_LANE_INCLUDED` in the comments that explain it.
sys.path.insert(0, os.path.join(ROOT, "scripts", "lib"))
import comments  # noqa: E402
from check_just_sources import just_sources  # noqa: E402 — phase-472 W2
LANE_SKIP = os.path.join(ROOT, "scripts", "build", "lane-skip.sh")

# The nine platform modules the fixture fan-out schedules, by the FILES that
# implement them. `zephyr` is three files; `qemu` is `qemu-baremetal.just`.
PLATFORM_MODULE_FILES = (
    "esp32.just",
    "freertos.just",
    "native.just",
    "nuttx.just",
    "px4.just",
    "qemu-baremetal.just",
    "threadx-linux.just",
    "threadx-riscv64.just",
    "zephyr.just",
    "zephyr-ci.just",
    "zephyr-dev.just",
    "zephyr-setup.just",
)

# (file, recipe) -> why this recipe is NOT a platform lane despite living in a
# platform module and using `nros_lane_skip`. A skip here is not about a
# platform anyone named, so the named/included rule does not apply.
#
# Keep this list short and keep the reasons specific. "It was failing" is not a
# reason; the fix for that is a `nros_lane_platform` line.
NOT_A_PLATFORM_LANE = {
    ("qemu-baremetal.just", "test-rtic-main-e2e"): (
        "The absent prerequisite is the ROS zenoh router, which is not part of "
        "this platform's provisioning (RFC-0075: we ship no router)."
    ),
}

# A `just` recipe header at column 0. Excludes assignments (`X := …`), module
# imports and attribute lines (`[private]`), which the body scan then attaches
# to the following recipe as `just` itself does.
RECIPE_HEADER = re.compile(r"^([@a-zA-Z_][a-zA-Z0-9_-]*)[^:=\n]*:(?!=)")

# A call at COMMAND POSITION — start of line, or after a separator. Not a bare
# `\bnros_lane_skip\b`: that also matches the function name quoted inside an
# `echo`, and a recipe explaining the protocol in prose is not using it.
_CMD = r"(?:^|[;&|(!]|\bthen\b|\belse\b|\bdo\b)[ \t]*"
# Every public function lane-skip.sh DEFINES, harvested — not enumerated. The
# enumerated list this replaced had drifted: it named `scope`/`scope_note`,
# which no longer exist, and missed `platform` and `out_of_scope_note`
# (phase-472 audit), so a recipe calling only those two was never asked.
# Not `nros_lane_[a-z_]+`: `nros_lane_coords_file` lives in another script.
def _lane_skip_functions(path=LANE_SKIP):
    with open(path, encoding="utf-8") as fh:
        names = re.findall(r"^(nros_lane_[a-z_]+)\(\)", fh.read(), re.M)
    if not names:
        sys.exit(f"check-named-lane-fails: harvested no functions from {path}")
    return names


CALLS_ANY = re.compile(
    _CMD + r"(?:" + "|".join(sorted(_lane_skip_functions(), key=len, reverse=True)) + r")\b",
    re.M,
)
# The whole-recipe form: `nros_lane_skip` NOT followed by `_`.
CALLS_WHOLE_SKIP = re.compile(_CMD + r"nros_lane_skip(?![_a-zA-Z0-9])", re.M)
DECLARES_PLATFORM = re.compile(_CMD + r"nros_lane_platform[ \t]+\S", re.M)
SOURCES = re.compile(r"^\s*(source|\.)\s+\S*scripts/build/lane-skip\.sh")


def recipes(text):
    """[(name, body)] for one justfile. Body excludes the header line."""
    out = []
    lines = text.splitlines()
    name = None
    body = []
    for line in lines:
        if line[:1] not in ("", " ", "\t", "#"):
            m = RECIPE_HEADER.match(line)
            if name is not None:
                out.append((name, "\n".join(body)))
                name, body = None, []
            if m:
                name = m.group(1).lstrip("@")
                body = []
            continue
        if name is not None:
            body.append(line)
    if name is not None:
        out.append((name, "\n".join(body)))
    return out


def audit_file(rel, text):
    """[(recipe, why)] — every violation of rules 2 and 3 in one justfile."""
    bad = []
    is_platform = os.path.basename(rel) in PLATFORM_MODULE_FILES
    for name, body in recipes(text):
        code = comments.strip_comments(body, "just")
        if not CALLS_ANY.search(code):
            continue
        if not any(SOURCES.match(l) for l in code.splitlines()):
            bad.append(
                (
                    name,
                    "calls nros_lane_* but never sources scripts/build/lane-skip.sh "
                    "— a recipe is its own process, so the call is `command not found`",
                )
            )
        if not is_platform or not CALLS_WHOLE_SKIP.search(code):
            continue
        if DECLARES_PLATFORM.search(code):
            continue
        if (os.path.basename(rel), name) in NOT_A_PLATFORM_LANE:
            continue
        bad.append(
            (
                name,
                "uses the whole-recipe `nros_lane_skip` in a platform module but "
                "declares no `nros_lane_platform <lane>` — a NAMED platform would "
                "still skip here (phase-411 W2)",
            )
        )
    return bad


def lane_included_sites(just_text):
    """Rule 4's count: `NROS_LANE_INCLUDED=` ASSIGNMENTS in code.

    Not the bare word, which the comments explaining the rule also contain —
    they held rule 4 at two "sites" with every real one deleted."""
    return len(re.findall(r"\bNROS_LANE_INCLUDED=", comments.strip_comments(just_text, "just")))


# ---------------------------------------------------------------------------
# The selftest, on the normal path.


def _bash(script, env=None, reason=""):
    """Run `script` with lane-skip.sh sourced. The reason arrives as `$1`.

    NOT interpolated into the script text, and the distinction is not
    theoretical: the real remedy strings contain backticks (``run `just zephyr
    setup` ``), and the first version of this selftest pasted one into a
    double-quoted bash string. Bash ran it. The gate hung for four minutes
    while `just zephyr setup` downloaded a 248 MB Zephyr SDK into the worktree.
    The recipes themselves are safe — `just` hands bash `\\``-escaped text — but
    a harness that re-quotes their strings is not, so it stops re-quoting them.
    """
    e = dict(os.environ)
    e.pop("NROS_LANE_INCLUDED", None)
    if env:
        e.update(env)
    return subprocess.run(
        ["bash", "-c", f". {LANE_SKIP}\n{script}", "lane-skip-selftest", reason],
        cwd=ROOT,
        env=e,
        capture_output=True,
        text=True,
        timeout=60,
    )


def self_test():
    """Both directions, for every entry point.

    A classifier that stopped classifying looks exactly like a pass, and the
    mutation this gate guards against — "named skips again" — is invisible to
    any amount of grepping. So run it.
    """
    bad = []
    included = {"NROS_LANE_INCLUDED": "zephyr"}

    def check(label, script, env, want_rc, want_in_output=None, forbid=None, reason=""):
        r = _bash(script, env, reason)
        out = r.stdout + r.stderr
        if r.returncode != want_rc:
            bad.append(
                f"self-test: {label}: expected rc {want_rc}, got {r.returncode}\n{out}"
            )
            return
        if want_in_output and want_in_output not in out:
            bad.append(f"self-test: {label}: expected {want_in_output!r} in output\n{out}")
        if forbid and forbid in out:
            bad.append(f"self-test: {label}: unexpected {forbid!r} in output\n{out}")

    # 1. The whole-recipe skip. A declared platform lane, named, must FAIL —
    #    carrying the site's own remedy text — and must not print the marker
    #    that the fan-out reads as SKIPPED.
    # The exact string zephyr-ci.just prints, backticks and all — so the
    # selftest proves the operator gets the remedy VERBATIM under the new
    # verdict, which is the whole claim of "reuse the text, change the exit".
    remedy = "ZEPHYR_WORKSPACE not set up — run `just zephyr setup`"
    check(
        "nros_lane_skip: named + scope declared",
        'nros_lane_platform zephyr; nros_lane_skip "$1"',
        None,
        1,
        want_in_output=remedy,
        forbid="NROS_LANE_SKIP:",
        reason=remedy,
    )
    check(
        "nros_lane_skip: included + scope declared",
        'nros_lane_platform zephyr; nros_lane_skip "$1"',
        included,
        78,
        want_in_output="NROS_LANE_SKIP: " + remedy,
        reason=remedy,
    )
    # 2. No scope declared — a check gate or a license-gated recipe. 0599's
    #    behaviour must be untouched in BOTH directions.
    check(
        "nros_lane_skip: named, no scope (a check gate)",
        'nros_lane_skip "$1"',
        None,
        78,
        want_in_output="NROS_LANE_SKIP:",
        reason="no network: submodule pins were NOT verified",
    )
    check(
        "nros_lane_skip: included, no scope",
        'nros_lane_skip "$1"',
        included,
        78,
        want_in_output="NROS_LANE_SKIP:",
        reason="no network",
    )
    # 3. The step form carries its lane, so it needs no declaration.
    check(
        "nros_lane_skip_note: named",
        'nros_lane_skip_note nuttx "$1"',
        None,
        1,
        want_in_output="arm-none-eabi-gcc not found",
        reason="arm-none-eabi-gcc not found",
    )
    check(
        "nros_lane_skip_note: included",
        'nros_lane_skip_reset nuttx; nros_lane_skip_note nuttx "$1"; echo NOTED',
        included,
        0,
        want_in_output="NOTED",
        reason="arm-none-eabi-gcc not found",
    )
    # 4. A lane NARROWING is not a missing prerequisite and never fails.
    check(
        "nros_lane_out_of_scope_note: named",
        'nros_lane_skip_reset nuttx; nros_lane_out_of_scope_note nuttx "$1"; echo NOTED',
        None,
        0,
        want_in_output="NOTED",
        reason="no nuttx-riscv coordinate in this run's lane",
    )
    # 5. …and it still reaches the flush, which still reports SKIPPED rather
    #    than claiming the lane built its fixtures. Named does not suppress the
    #    report; it only forbids a prerequisite skip.
    check(
        "nros_lane_skip_flush: named lane with a narrowing note",
        'nros_lane_skip_reset nuttx; nros_lane_out_of_scope_note nuttx "$1"; nros_lane_skip_flush nuttx "NuttX fixtures built."',
        None,
        78,
        want_in_output="NROS_LANE_SKIP:",
        forbid="NuttX fixtures built.",
        reason="no nuttx-riscv coordinate",
    )
    check(
        "nros_lane_skip_flush: nothing skipped",
        'nros_lane_skip_reset nuttx; nros_lane_skip_flush nuttx "NuttX fixtures built."',
        None,
        0,
        want_in_output="NuttX fixtures built.",
    )
    # 6. The static half, both directions, on synthetic bodies — so a
    #    classifier that stops classifying is caught too.
    must_flag = (
        "build-fixtures:\n"
        "    source scripts/build/lane-skip.sh\n"
        '    nros_lane_skip "no sdk"\n',
        "build-fixtures:\n" '    nros_lane_skip_note zephyr "no sdk"\n',
    )
    must_pass = (
        "build-fixtures:\n"
        "    source scripts/build/lane-skip.sh\n"
        "    nros_lane_platform zephyr\n"
        '    nros_lane_skip "no sdk"\n',
        "build-fixtures:\n"
        "    source scripts/build/lane-skip.sh\n"
        '    nros_lane_skip_note zephyr "no sdk"\n',
        "build-fixtures:\n" '    echo "nros_lane_skip is only prose here"\n',
    )
    for body in must_flag:
        if not audit_file("zephyr-ci.just", body):
            bad.append(f"self-test: expected a violation for:\n{body}")
    for body in must_pass:
        got = audit_file("zephyr-ci.just", body)
        if got:
            bad.append(f"self-test: unexpected violation {got} for:\n{body}")
    # 7. A non-platform file is out of scope for rule 3 but not for rule 2.
    if audit_file("check.just", must_flag[0]):
        bad.append("self-test: rule 3 must not apply outside the platform modules")
    # 8. phase-472 W3 — a COMMENT declares nothing, sources nothing and sets
    #    nothing, whether it is a whole line or trailing.
    comments.self_test()
    for body in (
        "build-fixtures:\n"
        "    source scripts/build/lane-skip.sh\n"
        "    true # ; nros_lane_platform zephyr\n"
        '    nros_lane_skip "no sdk"\n',
        "build-fixtures:\n"
        "    # source scripts/build/lane-skip.sh\n"
        '    nros_lane_skip_note zephyr "no sdk"\n',
    ):
        if not audit_file("zephyr-ci.just", body):
            bad.append(f"self-test: a comment satisfied a rule for:\n{body}")
    if lane_included_sites("# NROS_LANE_INCLUDED=x is set below\nfoo:\n    true\n"):
        bad.append("self-test: rule 4 counted NROS_LANE_INCLUDED in a comment")
    if lane_included_sites('foo:\n    env "NROS_LANE_INCLUDED=$p" just x\n') != 1:
        bad.append("self-test: rule 4 missed a real NROS_LANE_INCLUDED= assignment")

    # phase-472 W2 — the population is the justfile GRAPH: a recipe in a file
    # behind `mod check` + `import` is audited, and `nros_lane_platform` alone
    # (a function the old enumerated CALLS_ANY did not name) needs the source.
    import tempfile
    with tempfile.TemporaryDirectory() as tmp:
        os.makedirs(os.path.join(tmp, "just", "check"))
        for rel, text in (
            ("justfile", "mod check 'just/check.just'\n"),
            (os.path.join("just", "check.just"), "import 'check/a.just'\n"),
            (os.path.join("just", "check", "a.just"),
             "gate-a:\n    #!/usr/bin/env bash\n    nros_lane_platform zephyr\n"),
        ):
            with open(os.path.join(tmp, rel), "w", encoding="utf-8") as fh:
                fh.write(text)
        got = audit_tree(tmp)
        if not any(rel.endswith(os.path.join("check", "a.just")) for rel, _n, _w in got):
            bad.append(f"self-test: a `just/check/*.just` recipe was not audited: {got}")

    if bad:
        for b in bad:
            sys.stderr.write(b + "\n")
        sys.stderr.write(
            "\ncheck-named-lane-fails: the gate's OWN selftest failed — the "
            "protocol in scripts/build/lane-skip.sh no longer behaves as "
            "phase-411 W2 specifies.\n"
        )
        sys.exit(2)


def population(root):
    """[(rel, path)] — every justfile `just` loads (phase-472 W2).

    The graph, not `justfile` + `just/*.just`: the flat listing never read the
    13 files of `mod check` (`just/check/*.just`), so a gate recipe calling
    `nros_lane_skip` without sourcing lane-skip.sh read as clean.
    """
    return [(os.path.relpath(p, root), p) for p in just_sources(root)]


def audit_tree(root):
    failures = []
    for rel, path in population(root):
        text = open(path, encoding="utf-8").read()
        for name, why in audit_file(rel, text):
            failures.append((rel, name, why))
    return failures


def main():
    self_test()

    files = population(ROOT)
    failures = audit_tree(ROOT)

    # Rule 4 — both fan-out drivers hand their children the INCLUDED signal.
    root_just = open(os.path.join(ROOT, "justfile"), encoding="utf-8").read()
    n_signals = lane_included_sites(root_just)
    if n_signals < 2:
        failures.append(
            (
                "justfile",
                "build-test-fixtures-leaves",
                "sets NROS_LANE_INCLUDED at fewer than 2 sites — it has TWO "
                "drivers (the serial jobserver launcher and the generated make "
                "graph) and a driver that does not set it turns every platform "
                "it schedules into a NAMED one",
            )
        )

    if failures:
        sys.stderr.write("check-named-lane-fails: FAILED\n\n")
        for rel, name, why in failures:
            sys.stderr.write(f"  {rel}: recipe `{name}`:\n      {why}\n\n")
        sys.stderr.write(
            "  Named -> must work. Unnamed -> may skip, and is reported "
            "(phase-411 W2).\n"
            "    nros_lane_platform <lane>       this recipe IS that platform lane\n"
            "    nros_lane_skip_note <lane>   a STEP's prerequisite is missing\n"
            "    nros_lane_out_of_scope_note <lane>  this run's LANE selected no such\n"
            "                                 coordinate — never a failure\n"
            "  A recipe that is genuinely not a platform lane goes in\n"
            "  NOT_A_PLATFORM_LANE with its reason, not into a missing line.\n"
        )
        return 1

    print(
        f"named-lane failure protocol: OK ({len(files)} justfile(s), "
        f"{len(NOT_A_PLATFORM_LANE)} documented non-lane(s))"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
