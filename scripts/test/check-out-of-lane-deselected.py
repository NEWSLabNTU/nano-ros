#!/usr/bin/env python3
"""Issue 1900 — a test of a platform the lane did not select is DESELECTED,
whatever the host has.

THE RULE (issue 1758)

What a run does not attempt is decided by its SCOPE — the lane's coordinates
(`NROS_TEST_COORDS`) and the capabilities it does not claim
(`NROS_TEST_UNCLAIMED`) — and the scope check comes before any probe of the
host. A test of a platform the scope did not select must therefore reach
`lane_skip!` (`[SKIPPED:lane]`) on EVERY host. If it probes first, its verdict
tracks the host instead: `[UNMET PRECONDITION]` where the emulator is missing,
a bare `[SKIPPED]` where somebody hand-wrote one, PASS where the emulator
happens to exist.

WHY A RUNTIME CHECK

The defect is an ORDER inside a test body, and the helpers a test reaches
first (`require_qemu`, `Platform::require`, `run_pubsub_cell`, …) are where the
order lives — a static reading cannot tell a lane-first helper from a probe
without a hand list of which is which (the issue's heuristic over-reported 126
sites). Running the tests answers it exactly, and costs seconds: under a
`linux` scope every selected test either deselects at its first statement or
is a host-only check, so nothing boots and no fixture is resolved.

`check-skip-budget` cannot see this class: it reads the junit of a run on the
host that ran it, so a probe that PASSES there is invisible, and an
`[UNMET PRECONDITION]` is just a red to it. Hiding the emulators and RTOS trees
here is what makes the verdict host-independent — it is what a Linux runner
without them sees, which is the host every other platform's test must still
deselect on.

WHAT IT RUNS

* scope: `linux` — `nros_scope_coords_file linux` and `nros_lane_unclaimed
  linux`, exactly what `just test linux` exports.
* selection: every test the NAME carries a non-host platform token for — the
  negation of `scripts/test/lane-filter.sh linux`, whose tokens are derived
  from `PlatformId`. The name filter no longer narrows any run (issue 1900);
  here it names the tests the coordinate scope alone must deselect.
* host: the first rung of every emulator and RTOS-tree resolver points at
  `/nonexistent`.

VERDICT, per selected test:

* PASS — a host-only check that needs none of what was hidden;
* `[SKIPPED:lane]` — deselected by the scope, which is the point;
* ADMITTED — the scope SELECTED it (a platform-token name on a test whose
  coordinate is `linux`, e.g. `px4_bridge_compile`) and it stopped on the
  resolver's in-lane "fixture MISSING" verdict, because this job builds no
  fixture. Listed by name, not failed: that is the scope answering yes, and
  fixture coverage is `test-all`'s question, not this one's;
* PROBED — `[UNMET PRECONDITION]`, or a bare or other-class `[SKIPPED]`: a
  test that read the host before its scope. This is the check's subject, and
  it fails it;
* RAN AND FAILED — the test RAN under the scope and failed with no skip
  marker at all: an ordinary assertion, or nextest terminating it for time
  (issue 2069). That is not evidence about scope order, so it is reported as
  its own class with its own remedy — never as "read the host first", which
  sends the reader to fix an order that is already right. Whether it FAILS
  this check depends on whether another merge-gating lane answers for the
  same test:
  - an ordinary failure always fails. Most of these host-only tests run in no
    other lane (`test-unit` excludes `nros-tests`; `test-lane-contracts` runs
    only what `.config/lane-admission/gate.txt` admits — 6 of the 18 measured
    on 2026-10-27), and with the emulators hidden here a probe that panics
    WITHOUT a marker (an `.expect()` on a resolver) only shows up in this run;
  - a timeout fails unless the test is admitted to `test-lane-contracts`,
    which runs it under the default profile in the same `ci gate` and the same
    merge-queue batch. There this run's timeout only repeats — under a heavier
    parallel load — a verdict the other lane already gave, and failing on it
    ejects a whole batch over load (issue 2069: `example_shape
    zephyr_leaf_buildrs_uses_shared_bake`, 0.589 s in the other lane, 60.019 s
    TIMEOUT here). It is printed, loudly, and does not fail.

How a timeout is recognised is MEASURED, not guessed: nextest 0.9.140 writes
`<failure type="test timeout"/>` — no `message` attribute and no panic text,
which is why issue 2069's verdict read `(no message)`. An ordinary panic is
`type="test failure with exit code 101"` with a `message`.

What it cannot see: a test that resolves a fixture no manifest row attributes
(a `[[compile_check_fixture]]`, built module-level) with no scope check before
it reads as ADMITTED here — the resolver never asks the lane for such a path.
`freertos_firmware_entry.rs` was one (issue 1900) and asks first now.

`--self-test` checks the classifier on synthetic junit, both directions.
"""

import argparse
import os
import subprocess
import sys
import xml.etree.ElementTree as ET

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
sys.path.insert(0, os.path.join(ROOT, "scripts", "test"))
import skip_marker  # noqa: E402  THE classifier (issue 0658: one spelling)

# The first rung of each resolver a platform test probes: `qemu_system_path`
# (`QEMU_SYSTEM_<ARCH>`) and `nros_build_paths::locate` (`<SOURCE>_DIR`), both
# env-first. Pointing them at a path that does not exist is what a host
# without those tools answers.
HIDDEN = {
    "QEMU_SYSTEM_ARM": "/nonexistent/qemu-system-arm",
    "QEMU_SYSTEM_AARCH64": "/nonexistent/qemu-system-aarch64",
    "QEMU_SYSTEM_RISCV32": "/nonexistent/qemu-system-riscv32",
    "QEMU_SYSTEM_RISCV64": "/nonexistent/qemu-system-riscv64",
    "FREERTOS_DIR": "/nonexistent/freertos",
    "LWIP_DIR": "/nonexistent/lwip",
    "THREADX_DIR": "/nonexistent/threadx",
    "NETX_DIR": "/nonexistent/netxduo",
    "NUTTX_DIR": "/nonexistent/nuttx",
    "NUTTX_APPS_DIR": "/nonexistent/nuttx-apps",
}

PROFILE = "out-of-lane"

# The resolver's in-lane MISSING verdict (`fixtures/binaries/mod.rs`,
# `require_prebuilt_fixture_binary`): it panics only after the lane has been
# asked and said "in lane", so it is the scope ADMITTING the test.
ADMITTED_PREFIX = "Test fixture binary MISSING for an in-lane coordinate"


# nextest's junit for a test it terminated for time (measured on 0.9.140 with
# `slow-timeout = { period = "1s", terminate-after = 2 }`): an EMPTY
# `<failure type="test timeout"/>`.
TIMEOUT_TYPE = "test timeout"

# `nros_tests::unmet!` (`packages/testing/nros-tests/src/lib.rs`).
UNMET_PREFIX = "[UNMET PRECONDITION]"

ADMISSION = os.path.join(ROOT, ".config", "lane-admission", "gate.txt")


def lane_contracts_admission(path=ADMISSION):
    """What `test-lane-contracts` runs, parsed as its recipe parses it: a bare
    target admits it whole, `<target>::<test>` admits one test."""
    rows = set()
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = "".join(line.split("#", 1)[0].split())
            if line:
                rows.add(line)
    return rows


def covered_elsewhere(name, admission):
    """True when `test-lane-contracts` runs this `nros-tests::<target> <test>`."""
    classname, _, test = name.partition(" ")
    target = classname.split("::", 1)[1] if "::" in classname else "lib"
    return target in admission or f"{target}::{test}" in admission


def classify(junit_path, admission=frozenset()):
    """`(passed, deselected, admitted, probed, failed, timed_out)` for one junit
    file.

    Every list is `[(name, first line of its verdict)]`; a `timed_out` row
    carries a third field, whether `test-lane-contracts` also runs the test
    (per `admission`). The ONE predicate — the self-test goes through it too.
    """
    tree = ET.parse(junit_path)
    passed, deselected = 0, 0
    admitted, probed, failed, timed_out = [], [], [], []
    for case in tree.iter("testcase"):
        name = f"{case.get('classname', '')} {case.get('name', '')}".strip()
        bad = [n for n in case if n.tag in ("failure", "error")]
        if not bad:
            if case.find("skipped") is None:
                passed += 1
            continue
        node = bad[0]
        streams = skip_marker.testcase_streams(case)
        cls = skip_marker.skip_class_in((node.get("message"), node.text), streams)
        if cls == "lane":
            deselected += 1
            continue
        first = ""
        for text in [m for st in streams for m in skip_marker.panic_messages(st)] + [
            node.get("message") or "", node.text or ""
        ]:
            text = text.strip()
            if text:
                first = text.splitlines()[0]
                break
        if cls is not None or first.startswith(UNMET_PREFIX):
            probed.append((name, first))
        elif node.get("type") == TIMEOUT_TYPE:
            timed_out.append((name, first or "(terminated for time)",
                              covered_elsewhere(name, admission)))
        elif first.startswith(ADMITTED_PREFIX):
            admitted.append((name, first))
        else:
            failed.append((name, first or "(no message)"))
    return passed, deselected, admitted, probed, failed, timed_out


def self_test(tmp_dir):
    def junit(cases):
        body = "".join(cases)
        return f"<testsuites><testsuite name='nros-tests::t'>{body}</testsuite></testsuites>"

    def case(name, err=None, target="t"):
        if err is None:
            return f"<testcase classname='nros-tests::{target}' name='{name}'/>"
        return (
            f"<testcase classname='nros-tests::{target}' name='{name}'>"
            "<failure type='test failure with exit code 101'/>"
            f"<system-err>thread 't' panicked at t.rs:1:5:\n{err}\nnote: run</system-err>"
            "</testcase>"
        )

    def timeout(name, target="t"):
        # Exactly what nextest 0.9.140 wrote for a terminated test.
        return (
            f"<testcase classname='nros-tests::{target}' name='{name}'>"
            f"<failure type='{TIMEOUT_TYPE}'/>"
            "<system-out>\nrunning 1 test\n</system-out><system-err></system-err>"
            "</testcase>"
        )

    path = os.path.join(tmp_dir, "self-test-junit.xml")
    admission = {"covered", "partial::the_one"}
    rows = [
        (case("host_only"), "pass"),
        (case("lane_first", "[SKIPPED:lane] out of lane: QemuBaremetal"), "lane"),
        (case("probe_first", UNMET_PREFIX + " qemu-system-arm not found"), "probed"),
        (case("bare_skip", "[SKIPPED] NuttX source tree not found"), "probed"),
        (case("other_class", "[SKIPPED:capability] no west"), "probed"),
        # Issue 2069 — a test that RAN and failed is not a scope violation,
        # whether or not another lane runs it…
        (case("real_red", "assertion failed: x"), "failed"),
        (case("covered_red", "assertion failed: y", target="covered"), "failed"),
        # …and `[UNMET PRECONDITION]` quoted later in an assertion is not the
        # marker: it must START the panic message.
        (case("quoted_unmet", "assertion failed: saw " + UNMET_PREFIX), "failed"),
        (case("in_lane_missing", ADMITTED_PREFIX + ": build/x/y"), "admitted"),
        # The prefix must START the panic message: quoting it later in an
        # assertion is not the resolver's verdict.
        (case("quoted", "assertion failed: saw " + ADMITTED_PREFIX), "failed"),
        # A timeout is its own class, never a probe — and whether it fails is
        # whether another lane runs the test (a whole target, or one test of it).
        (timeout("slow"), "timeout-fails"),
        (timeout("slow", target="covered"), "timeout-covered"),
        (timeout("the_one", target="partial"), "timeout-covered"),
        (timeout("a_sibling", target="partial"), "timeout-fails"),
    ]
    for c, want in rows:
        with open(path, "w", encoding="utf-8") as fh:
            fh.write(junit([c]))
        p, d, a, pr, f, t = classify(path, admission)
        got = (
            "pass" if p else "lane" if d else "admitted" if a else "probed" if pr
            else "failed" if f
            else ("timeout-covered" if t[0][2] else "timeout-fails") if t else "none"
        )
        assert got == want, f"self-test: {c[:70]}… classified {got}, want {want}"
    # The admission parse is the recipe's: the real file must admit the test
    # issue 2069 timed out, or the tolerance above can never apply to it.
    real = lane_contracts_admission()
    assert covered_elsewhere(
        "nros-tests::example_shape zephyr_leaf_buildrs_uses_shared_bake", real
    ), "self-test: gate.txt no longer admits example_shape — re-read issue 2069"
    os.remove(path)


def run(out_dir):
    env = dict(os.environ)
    sh = (
        "set -e; . scripts/build/fixture-lane.sh; "
        "printf '%s\\n' \"$(nros_scope_coords_file linux)\"; "
        "printf '%s\\n' \"$(nros_lane_unclaimed linux)\""
    )
    res = subprocess.run(["bash", "-c", sh], cwd=ROOT, capture_output=True, text=True)
    if res.returncode != 0:
        sys.stderr.write(res.stderr)
        sys.exit("check-out-of-lane-deselected: cannot compute the linux scope")
    coords, unclaimed = (res.stdout.split("\n") + ["", ""])[:2]
    if not coords or not os.path.isfile(coords):
        sys.exit(
            "check-out-of-lane-deselected: the linux scope wrote no coordinate file — "
            "an unscoped run would deselect nothing and this check would measure nothing"
        )

    lf = subprocess.run(
        ["bash", "scripts/test/lane-filter.sh", "linux"],
        cwd=ROOT, capture_output=True, text=True,
    )
    lines = [l for l in lf.stdout.splitlines() if l.strip()]
    if lf.returncode != 0 or not lines:
        sys.stderr.write(lf.stderr)
        sys.exit("check-out-of-lane-deselected: lane-filter.sh linux produced no tokens")
    # lane-filter's lines are the conjunction that EXCLUDES the platform-token
    # tests; its negation selects exactly them.
    selection = "not (" + " and ".join(f"({l})" for l in lines) + ")"

    env.update(HIDDEN)
    env["NROS_FIXTURE_LANE"] = "linux"
    env["NROS_TEST_COORDS"] = coords
    env["NROS_TEST_UNCLAIMED"] = unclaimed

    cargo_args = subprocess.run(
        ["bash", "-c", ". scripts/build/cargo.sh; nros_cargo_nextest_args"],
        cwd=ROOT, capture_output=True, text=True, check=True,
    ).stdout.split()
    junit = os.path.join(ROOT, "target", "nextest", PROFILE, "junit.xml")
    if os.path.exists(junit):
        os.remove(junit)
    cmd = (
        ["cargo", "nextest", "run", "-p", "nros-tests", *cargo_args,
         "-P", PROFILE, "--no-fail-fast",
         "--failure-output", "never",
         "-E", selection]
    )
    print(f"check-out-of-lane-deselected: linux scope ({coords}), emulators/RTOS trees hidden")
    rc = subprocess.run(cmd, cwd=ROOT, env=env).returncode
    # nextest: 0 = all passed, 100 = some tests failed (expected: the lane
    # skips are panics). Anything else is a build or setup failure, which must
    # never be read as "nothing offended".
    if rc not in (0, 100) or not os.path.isfile(junit):
        sys.exit(f"check-out-of-lane-deselected: nextest exited {rc} with no junit — "
                 "the selection did not run")
    passed, deselected, admitted, probed, failed, timed_out = classify(
        junit, lane_contracts_admission())
    if admitted:
        print(f"check-out-of-lane-deselected: {len(admitted)} test(s) the linux scope ADMITS "
              "stopped on a fixture this job does not build (not this check's subject):")
        for name, _why in admitted:
            print(f"  {name}")
    total = passed + deselected + len(admitted) + len(probed) + len(failed) + len(timed_out)
    if total == 0:
        sys.exit("check-out-of-lane-deselected: the selection reached no test")
    covered_timeouts = [(n, w) for n, w, cov in timed_out if cov]
    own_timeouts = [(n, w) for n, w, cov in timed_out if not cov]
    if covered_timeouts:
        # Loud, and not a failure: see the module doc, "RAN AND FAILED".
        sys.stderr.write(
            f"check-out-of-lane-deselected: WARNING — {len(covered_timeouts)} test(s) RAN "
            "under the scope and were TERMINATED FOR TIME (not a scope-order defect, and "
            "not failed here):\n"
        )
        for name, _why in covered_timeouts:
            sys.stderr.write(f"  {name}\n")
        sys.stderr.write(
            "  `test-lane-contracts` runs each of these under the default profile in the\n"
            "  same lane, and its verdict is the one that stands; a timeout only here is\n"
            "  this run's parallel load (issue 2069). If one recurs, time it solo:\n"
            "      cargo nextest run -p nros-tests --test <target> -E 'test(<name>)'\n"
        )
    red = False
    if failed or own_timeouts:
        red = True
        sys.stderr.write(
            f"check-out-of-lane-deselected: FAILED — {len(failed) + len(own_timeouts)} "
            "test(s) RAN under the `linux` scope and failed. Not a scope-order defect\n"
            "  (no skip marker: nothing probed ahead of the scope), but no other\n"
            "  merge-gating lane answers for these, so the verdict stands here:\n"
        )
        for name, why in failed:
            sys.stderr.write(f"  {name}\n      {why}\n")
        for name, _why in own_timeouts:
            sys.stderr.write(f"  {name}\n      TIMEOUT (nextest terminated it for time)\n")
        sys.stderr.write(
            "\n  Debug it as an ordinary red: re-run it SOLO in this environment\n"
            "  (`just check out-of-lane-deselected` hides the emulators and RTOS trees —\n"
            "  an `.expect()` on a resolver fails here and passes on a host that has the\n"
            "  tool; that is a missing `nros_tests::unmet!`, issue 1758). A timeout that\n"
            "  passes solo is load, not a hang (issue 2069).\n"
        )
    if probed:
        red = True
        sys.stderr.write(
            f"check-out-of-lane-deselected: FAILED — {len(probed)} test(s) of a platform "
            "the `linux` scope does not select read the host before the scope:\n"
        )
        for name, why in probed:
            sys.stderr.write(f"  {name}\n      {why}\n")
        sys.stderr.write(
            "\n  Under a scope that does not select its platform, a test must reach\n"
            "  `lane_skip!` before any probe (issue 1758). Put the scope check FIRST —\n"
            "  `nros_tests::fixtures::lane::require_platform_in_lane(&[PlatformId::…], …)`\n"
            "  at the head of the test or of the helper it calls first — and turn any\n"
            "  hand-written `[SKIPPED]` into `nros_tests::unmet!` after it (issue 1900).\n"
        )
    if red:
        return 1
    print(
        f"check-out-of-lane-deselected: OK ({deselected} deselected as lane, "
        f"{passed} host-only pass(es), {len(admitted)} admitted, "
        f"{len(covered_timeouts)} timeout(s) another lane answers for, "
        "0 probed the host first, 0 ran and failed)"
    )
    return 0


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--self-test", action="store_true",
                    help="check the classifier only (no cargo)")
    args = ap.parse_args()
    tmp = os.path.join(ROOT, "target", "nextest")
    os.makedirs(tmp, exist_ok=True)
    self_test(tmp)
    if args.self_test:
        print("check-out-of-lane-deselected: self-test OK")
        return 0
    return run(tmp)


if __name__ == "__main__":
    sys.exit(main())
