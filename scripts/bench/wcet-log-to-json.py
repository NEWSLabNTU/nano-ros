#!/usr/bin/env python3
"""Issue 0403 item 1 — lift the WCET bench's machine half out of its log.

# Why the artifact is built here and not by the bench

`wcet-cycles-qemu` is a `no_std` Cortex-M image whose only output channel is
semihosting stdout. It cannot open a file, so it cannot write an artifact. What
it CAN do is print the same numbers twice: once as prose for a human, once as
`NROS_WCET_V1`-marked TSV for a tool. This script is the tool.

That split also puts the parsing where it can be tested. The producer needs
hardware — QEMU does not implement DWT cycle counting, so the bench refuses
there (issue 0403's other half) and emits no measurements at all. The parser
does not need hardware, and `--self-test` exercises it.

# What the artifact must carry, and why

From issue 0403's Direction:

  * per measurement: `min` / `max` / `mean`, `iterations`, and the identity of
    what was measured;
  * the counter's validity, "so a stale file cannot be re-read as good";
  * the conditions — CPU, clock rate, build profile, commit — because "cycles
    convert to the `ms` the mapper wants only through a clock rate, so an
    artifact without one is not convertible".

# The two absences this refuses to paper over

**A refused run has no artifact.** A log with no measurements is not an
artifact with zero measurements. Issue 0259 is what happens when an absence is
allowed to read as a number, and zero is the most optimistic value a WCET can
take, so the mistake always errs toward "schedulable". This exits non-zero and
writes nothing.

**No clock rate means not convertible.** The bench cannot read the part's real
clock, so it emits none, and `clock_hz` stays `null` with `convertible: false`
beside it. A consumer that needs `ms` must refuse such a file rather than pick a
plausible rate — inventing one is the manufactured-WCET failure that issue 0404
exists to prevent, one layer earlier.
"""

import json
import os
import sys

MARKER = "NROS_WCET_V1"
SCHEMA = "nros.wcet.measurements/1"


def parse(text):
    """Parse marked lines out of a bench log.

    Returns `(conditions, measurements)`. Unmarked lines are the human prose and
    are ignored; a malformed marked line is a hard error rather than a skip,
    because silently dropping one measurement from a WCET set is the same class
    of quiet loss this whole issue is about.
    """
    conditions = {
        "cpu": None,
        "clock_hz": None,
        "profile": None,
        "commit": None,
        "counter_valid": False,
    }
    measurements = []
    for lineno, raw in enumerate(text.splitlines(), 1):
        idx = raw.find(MARKER)
        if idx < 0:
            continue
        fields = raw[idx:].split("\t")
        kind = fields[1] if len(fields) > 1 else ""
        if kind == "measurement":
            if len(fields) != 7:
                raise ValueError(
                    f"line {lineno}: measurement needs 7 fields, got {len(fields)}: {raw!r}"
                )
            _, _, name, mn, mx, mean, iters = fields
            measurements.append(
                {
                    "name": name,
                    "min_cycles": int(mn),
                    "max_cycles": int(mx),
                    "mean_cycles": int(mean),
                    "iterations": int(iters),
                }
            )
        elif kind in ("cpu", "profile", "commit"):
            conditions[kind] = fields[2]
        elif kind == "clock_hz":
            conditions["clock_hz"] = int(fields[2])
        elif kind == "counter_valid":
            conditions["counter_valid"] = fields[2] == "true"
        else:
            raise ValueError(f"line {lineno}: unknown {MARKER} record {kind!r}")
    return conditions, measurements


def build_artifact(conditions, measurements):
    return {
        "schema": SCHEMA,
        "conditions": conditions,
        # Cycles become the mapper's `ms` only through a clock rate. Stated as a
        # field so a consumer checks it instead of assuming.
        "convertible_to_time": conditions["clock_hz"] is not None,
        "measurements": measurements,
    }


def convert(log_path, out_path):
    # A missing log is an ordinary condition — the lane may not have run — so it
    # gets a sentence, not a traceback.
    if not os.path.isfile(log_path):
        sys.stderr.write(
            f"wcet-log-to-json: no such log: {log_path}\n"
            "  Run the bench first (`just qemu test-wcet`), or point at the log\n"
            "  it wrote — the path is `test-logs/<stamp>/qemu-wcet-bench.log`.\n"
        )
        return 2
    with open(log_path, encoding="utf-8", errors="replace") as fh:
        conditions, measurements = parse(fh.read())

    if not measurements:
        sys.stderr.write(
            f"wcet-log-to-json: {log_path} contains no {MARKER} measurements.\n"
            "  A run that could not measure has produced no evidence, so there is\n"
            "  no artifact to write — an empty one would be indistinguishable from\n"
            "  a run where everything took zero cycles, and zero is the most\n"
            "  optimistic WCET there is (issues 0403, 0259).\n"
            "  On QEMU this is expected: the DWT cycle counter does not count.\n"
        )
        return 1

    artifact = build_artifact(conditions, measurements)
    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(artifact, fh, indent=2, sort_keys=True)
        fh.write("\n")
    note = "" if artifact["convertible_to_time"] else "  (no clock_hz — NOT convertible to time)"
    print(
        f"wcet-log-to-json: {len(measurements)} measurement(s) -> {out_path}{note}"
    )
    return 0


def self_test():
    good = "\n".join(
        [
            "  nros WCET Benchmark (Cortex-M3)",
            f"{MARKER}\tcounter_valid\ttrue",
            f"{MARKER}\tcpu\tcortex-m3",
            f"{MARKER}\tprofile\trelease",
            f"{MARKER}\tcommit\tabc123def456",
            "  serialize Int32: min=10 max=14 avg=11 cycles",
            f"{MARKER}\tmeasurement\tserialize Int32\t10\t14\t11\t100",
            f"{MARKER}\tmeasurement\tcrc32 (64B)\t50\t61\t53\t100",
        ]
    )
    cond, meas = parse(good)
    assert len(meas) == 2, meas
    assert meas[0] == {
        "name": "serialize Int32",
        "min_cycles": 10,
        "max_cycles": 14,
        "mean_cycles": 11,
        "iterations": 100,
    }, meas[0]
    assert cond["cpu"] == "cortex-m3" and cond["commit"] == "abc123def456", cond
    assert cond["counter_valid"] is True

    art = build_artifact(cond, meas)
    assert art["convertible_to_time"] is False, (
        "no clock_hz was emitted, so the artifact must NOT claim it converts to time"
    )

    cond2 = dict(cond, clock_hz=168_000_000)
    assert build_artifact(cond2, meas)["convertible_to_time"] is True

    # A refused run: prose only, no marked measurements.
    refused = "\n".join(
        [
            "FAIL: the DWT cycle counter is not counting.",
            "[FAIL]",
        ]
    )
    _, meas2 = parse(refused)
    assert meas2 == [], "a refused run must yield no measurements, not zeros"

    # Prose that merely mentions zeros must never become data.
    _, meas3 = parse("  serialize Int32: min=0 max=0 avg=0 cycles")
    assert meas3 == [], "unmarked prose must not be parsed as a measurement"

    # A malformed record is an error, not a silent skip.
    try:
        parse(f"{MARKER}\tmeasurement\tonly\t3\tfields")
    except ValueError:
        pass
    else:  # pragma: no cover
        sys.stderr.write("self-test: a malformed measurement must raise\n")
        return 2

    try:
        parse(f"{MARKER}\tsomething_new\tx")
    except ValueError:
        pass
    else:  # pragma: no cover
        sys.stderr.write("self-test: an unknown record kind must raise\n")
        return 2

    drift = producer_format_drift()
    if drift:
        sys.stderr.write(f"self-test: {drift}\n")
        return 2

    print("wcet-log-to-json self-test: OK (8 cases)")
    return 0


def producer_format_drift():
    """Check the parser against the PRODUCER's real format string, not a fixture.

    Every case above is written against a hand-made sample, and a hand-made
    sample is a mirror: it can agree with the parser forever while the bench
    that actually emits the lines drifts away from both. That is the failure
    this tree has hit repeatedly (the sizes-header mirror, the FFI struct
    mirrors), and the fix is always the same — check the real thing.

    Returns a complaint, or None. Silent when the source is not reachable: this
    runs on every conversion, and a converter should not fail because it was
    invoked from outside a checkout.
    """
    here = os.path.dirname(os.path.abspath(__file__))
    main_rs = os.path.join(
        here, "..", "..", "packages", "testing", "nros-bench",
        "wcet-cycles-qemu", "src", "main.rs",
    )
    main_rs = os.path.normpath(main_rs)
    if not os.path.isfile(main_rs):
        return None
    try:
        src = open(main_rs, encoding="utf-8").read()
    except OSError:
        return None

    if MARKER not in src:
        return (
            f"{main_rs} no longer emits {MARKER} — the producer and this parser "
            "have diverged, so every future log will parse as empty"
        )
    # The measurement record is the one with a field count this parser hard-codes.
    for line in src.splitlines():
        if f'"{MARKER}\\tmeasurement' in line.replace("\\t", "\\t"):
            # `MARKER \t measurement \t name \t min \t max \t mean \t iters` = 7.
            placeholders = line.count("{}")
            if placeholders != 5:
                return (
                    f"{main_rs}: the measurement line emits {placeholders} values, "
                    "but this parser expects 5 (name, min, max, mean, iterations). "
                    "Producer and parser must move together"
                )
            return None
    return (
        f"{main_rs}: found no `{MARKER}\\tmeasurement` emission — if the record was "
        "renamed, this parser needs the same change"
    )


def main():
    if "--self-test" in sys.argv:
        sys.exit(self_test())
    if len(sys.argv) != 3:
        sys.stderr.write(
            "usage: wcet-log-to-json.py <bench.log> <out.json>\n"
            "       wcet-log-to-json.py --self-test\n"
        )
        sys.exit(2)
    rc = self_test_quiet()
    if rc:
        sys.exit(rc)
    sys.exit(convert(sys.argv[1], sys.argv[2]))


def self_test_quiet():
    """Run the self-test with its output suppressed.

    The parser runs on every conversion, so it verifies itself on every
    conversion — the idiom `check-feature-contract` and friends use, for the
    same reason: a self-test nobody runs is a comment.
    """
    import contextlib
    import io

    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        return self_test()


if __name__ == "__main__":
    main()
