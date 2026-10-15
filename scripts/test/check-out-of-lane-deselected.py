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
* anything else — `[UNMET PRECONDITION]`, a bare or other-class `[SKIPPED]`,
  an ordinary failure — is a test that read the host before its scope, and
  fails this check.

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


def classify(junit_path):
    """`(passed, deselected, admitted, offenders)` for one junit file.

    `admitted` and `offenders` are `[(name, first line of its verdict)]`. The
    ONE predicate — the self-test goes through it too.
    """
    tree = ET.parse(junit_path)
    passed, deselected, admitted, offenders = 0, 0, [], []
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
        first = first or "(no message)"
        if first.startswith(ADMITTED_PREFIX):
            admitted.append((name, first))
        else:
            offenders.append((name, first))
    return passed, deselected, admitted, offenders


def self_test(tmp_dir):
    def junit(cases):
        body = "".join(cases)
        return f"<testsuites><testsuite name='nros-tests::t'>{body}</testsuite></testsuites>"

    def case(name, err=None):
        if err is None:
            return f"<testcase classname='nros-tests::t' name='{name}'/>"
        return (
            f"<testcase classname='nros-tests::t' name='{name}'><failure/>"
            f"<system-err>thread 't' panicked at t.rs:1:5:\n{err}\nnote: run</system-err>"
            "</testcase>"
        )

    path = os.path.join(tmp_dir, "self-test-junit.xml")
    rows = [
        (case("host_only"), "pass"),
        (case("lane_first", "[SKIPPED:lane] out of lane: QemuBaremetal"), "lane"),
        (case("probe_first", "[UNMET PRECONDITION] qemu-system-arm not found"), "bad"),
        (case("bare_skip", "[SKIPPED] NuttX source tree not found"), "bad"),
        (case("other_class", "[SKIPPED:capability] no west"), "bad"),
        (case("real_red", "assertion failed: x"), "bad"),
        (case("in_lane_missing", ADMITTED_PREFIX + ": build/x/y"), "admitted"),
        # The prefix must START the panic message: quoting it later in an
        # assertion is not the resolver's verdict.
        (case("quoted", "assertion failed: saw " + ADMITTED_PREFIX), "bad"),
    ]
    for c, want in rows:
        with open(path, "w", encoding="utf-8") as fh:
            fh.write(junit([c]))
        p, d, a, o = classify(path)
        got = "pass" if p else "lane" if d else "admitted" if a else "bad" if o else "none"
        assert got == want, f"self-test: {c[:60]}… classified {got}, want {want}"
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
         "--success-output", "never", "--failure-output", "never",
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
    passed, deselected, admitted, offenders = classify(junit)
    if admitted:
        print(f"check-out-of-lane-deselected: {len(admitted)} test(s) the linux scope ADMITS "
              "stopped on a fixture this job does not build (not this check's subject):")
        for name, _why in admitted:
            print(f"  {name}")
    if passed + deselected + len(admitted) + len(offenders) == 0:
        sys.exit("check-out-of-lane-deselected: the selection reached no test")
    if offenders:
        sys.stderr.write(
            f"check-out-of-lane-deselected: FAILED — {len(offenders)} test(s) of a platform "
            "the `linux` scope does not select read the host before the scope:\n"
        )
        for name, why in offenders:
            sys.stderr.write(f"  {name}\n      {why}\n")
        sys.stderr.write(
            "\n  Under a scope that does not select its platform, a test must reach\n"
            "  `lane_skip!` before any probe (issue 1758). Put the scope check FIRST —\n"
            "  `nros_tests::fixtures::lane::require_platform_in_lane(&[PlatformId::…], …)`\n"
            "  at the head of the test or of the helper it calls first — and turn any\n"
            "  hand-written `[SKIPPED]` into `nros_tests::unmet!` after it (issue 1900).\n"
        )
        return 1
    print(
        f"check-out-of-lane-deselected: OK ({deselected} deselected as lane, "
        f"{passed} host-only pass(es), {len(admitted)} admitted, 0 probed the host first)"
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
