#!/usr/bin/env python3
"""Every `[tool.*]` proves it RUNS, or says in writing why it does not.

WHY THIS EXISTS — phase-447 C1, RFC-0099 D5

A dist is ABI-bound and nothing proved it runs on the host that installed it.
`host_key()` is `<os>-<arch>` — no OS version, no libc — and `nano-ros-sdk`
builds its dists on bare `ubuntu-22.04` runners, so every Linux x86_64 host is
offered them, including hosts that cannot run them. C1 moved `failing_smoke`
onto the install path so an unrunnable dist fails at unpack, naming the probe
and what it printed, instead of surfacing later as a bare loader error with no
mention of provisioning.

That fix covers exactly the tools that DECLARE a `smoke` probe, and
`failing_smoke`'s own doc states the rest of the truth:

    Absent `smoke` means no opinion, not a pass — most dists have none yet,
    and silence must not read as coverage.

So the install-path check without this ratchet is a loud failure over a small
minority and a silent success over everything else, which is the shape it was
written to remove. 12 of 25 tools declared a probe when this landed.

WHAT IT CHECKS

Every `[tool.<name>]` in `nros-sdk-index.toml` either

  * declares `smoke = [{ run = .., expect = .. }]`, or
  * has a line in `.config/smoke-or-reason-baseline.txt` giving a REASON.

The baseline may only SHRINK — the same shape as
`.config/gate-selftest-baseline.txt` and the c-array-pool `UNCLASSIFIED_CEILING`:

  * a tool that gained a probe must have its baseline line DELETED (a line that
    outlives its debt is how a ratchet quietly becomes an allowlist);
  * a baseline line naming a tool the index no longer has is stale and goes;
  * a new tool with neither is a failure, so it must argue for having no probe
    rather than getting silence for free.

A reason is prose, not a placeholder: it says what would be probed and why
nobody probes it. `corrosion` is the clean case — it installs CMake package
files and no executable at all, so there is nothing to run and the argument is
permanent. Most of the rest are debt, and read like it on purpose.

WHAT IT DOES NOT CHECK

Whether a declared probe is CORRECT, or whether the tool is installed. Those
are questions about a host and a dist, not about the tree: the install path
asks the first (`nros setup`) and `check-dist-runtime-deps` the second.

Usage:  check-smoke-or-reason.py [--index PATH] [--baseline PATH]
"""

import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
INDEX = os.path.join(ROOT, "nros-sdk-index.toml")
BASELINE = os.path.join(ROOT, ".config", "smoke-or-reason-baseline.txt")

# A reason shorter than this is a placeholder ("todo", "n/a", "later"). The
# number is arbitrary; requiring one at all is not.
MIN_REASON = 24


def load_index(path):
    try:
        import tomllib as toml
    except ModuleNotFoundError:
        import tomli as toml
    with open(path, "rb") as fh:
        return toml.load(fh)


def parse_baseline(text):
    """`{tool: reason}` plus the malformed lines, in file order."""
    rows, bad = {}, []
    for lineno, raw in enumerate(text.splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        if ":" not in line:
            bad.append((lineno, raw, "no `<tool>: <reason>` separator"))
            continue
        name, reason = line.split(":", 1)
        name, reason = name.strip(), reason.strip()
        if name in rows:
            bad.append((lineno, raw, "duplicate entry for %s" % name))
            continue
        rows[name] = (lineno, reason)
    return rows, bad


def audit(index, baseline_rows):
    """[(kind, name, detail)] — everything the ratchet refuses."""
    tools = index.get("tool", {})
    problems = []

    for name, tool in sorted(tools.items()):
        if tool.get("smoke"):
            continue
        if name not in baseline_rows:
            problems.append(
                (
                    "unargued",
                    name,
                    "no `smoke` probe and no baseline line — declare a probe, or "
                    "add `%s: <why not>` to the baseline" % name,
                )
            )
            continue
        _, reason = baseline_rows[name]
        if len(reason) < MIN_REASON:
            problems.append(
                (
                    "no-reason",
                    name,
                    "baseline line has no real reason (%d chars; a reason says what "
                    "would be probed and why nobody does)" % len(reason),
                )
            )

    for name, (lineno, _) in sorted(baseline_rows.items(), key=lambda kv: kv[1][0]):
        if name not in tools:
            problems.append(
                (
                    "stale",
                    name,
                    "baseline line %d names no [tool.%s] — delete it" % (lineno, name),
                )
            )
        elif tools[name].get("smoke"):
            problems.append(
                (
                    "ratchet",
                    name,
                    "[tool.%s] declares a `smoke` probe now — delete baseline line %d "
                    "(the ratchet SHRANK; leaving the line makes it an allowlist)"
                    % (name, lineno),
                )
            )
    return problems


def self_test():
    """Prove each rule can fail — a negative control nobody runs is a comment."""
    probe = [{"run": "bin/x --version", "expect": "x 1.0"}]
    good_reason = "installs CMake package files and no executable at all"
    assert len(good_reason) >= MIN_REASON

    def kinds(index, text):
        rows, bad = parse_baseline(text)
        return [k for k, _, _ in audit(index, rows)], bad

    checks = []

    # The clean state: one probed tool, one argued tool, nothing else.
    index = {"tool": {"probed": {"smoke": probe}, "argued": {}}}
    got, bad = kinds(index, "# header\nargued: %s\n" % good_reason)
    checks.append(("a probed tool and an argued tool are clean", got == [] and bad == []))

    # A new tool with neither is refused — the rule the ratchet exists for.
    index2 = {"tool": dict(index["tool"], fresh={})}
    got, _ = kinds(index2, "argued: %s\n" % good_reason)
    checks.append(("a new unprobed tool with no line fails", got == ["unargued"]))

    # A tool that GAINED a probe must lose its line: this is the only-shrink
    # half, and without it the file rots into an allowlist.
    index3 = {"tool": {"argued": {"smoke": probe}}}
    got, _ = kinds(index3, "argued: %s\n" % good_reason)
    checks.append(("a line that outlived its debt fails", got == ["ratchet"]))

    # A line naming a tool that is gone is stale.
    got, _ = kinds(index, "argued: %s\ndeparted: %s\n" % (good_reason, good_reason))
    checks.append(("a line naming no tool fails", got == ["stale"]))

    # A reason must be a reason.
    got, _ = kinds(index, "argued: todo\n")
    checks.append(("a placeholder reason fails", got == ["no-reason"]))

    # Malformed lines are reported, not silently skipped into "argued".
    _, bad = kinds(index, "argued %s\n" % good_reason)
    checks.append(("a line with no separator is malformed", len(bad) == 1))
    _, bad = kinds(index, "argued: %s\nargued: %s\n" % (good_reason, good_reason))
    checks.append(("a duplicate entry is malformed", len(bad) == 1))

    failed = [name for name, ok in checks if not ok]
    if failed:
        for name in failed:
            print("check-smoke-or-reason self-test: FAIL %s" % name, file=sys.stderr)
        raise SystemExit(1)


def main(argv):
    index_path, baseline_path = INDEX, BASELINE
    while argv:
        flag = argv.pop(0)
        if flag == "--index":
            index_path = argv.pop(0)
        elif flag == "--baseline":
            baseline_path = argv.pop(0)
        elif flag == "--self-test":
            return 0  # already ran, below
        else:
            print("check-smoke-or-reason: unknown argument %s" % flag, file=sys.stderr)
            return 2

    # A missing input is named, never assumed empty: an absent baseline read as
    # `{}` would fail every unprobed tool with a message about the wrong thing,
    # and an absent index would report OK over nothing.
    for path, what in ((index_path, "the SDK index"), (baseline_path, "the ratchet baseline")):
        if not os.path.isfile(path):
            print(
                "check-smoke-or-reason: %s is missing at %s" % (what, path),
                file=sys.stderr,
            )
            return 2

    index = load_index(index_path)
    with open(baseline_path, encoding="utf-8") as fh:
        rows, malformed = parse_baseline(fh.read())

    if malformed:
        print("check-smoke-or-reason: the baseline has unreadable line(s):\n", file=sys.stderr)
        for lineno, raw, why in malformed:
            print("  line %d: %s  — %s" % (lineno, raw.strip(), why), file=sys.stderr)
        print(
            "\n  Format is one `<tool>: <reason>` per line; `#` comments.",
            file=sys.stderr,
        )
        return 1

    problems = audit(index, rows)
    if problems:
        print(
            "check-smoke-or-reason: %d [tool.*] entry/entries neither prove they run "
            "nor say why not:\n" % len(problems),
            file=sys.stderr,
        )
        for kind, name, detail in problems:
            print("  [%s]  %s — %s" % (kind, name, detail), file=sys.stderr)
        print(
            "\n  A `smoke` probe is what makes `nros setup` fail at UNPACK rather than\n"
            "  at first use (phase-447 C1). Absent `smoke` is no opinion, not a pass —\n"
            "  so a tool with no probe owes a reason, and %s\n"
            "  may only shrink." % os.path.relpath(baseline_path, ROOT),
            file=sys.stderr,
        )
        return 1

    tools = index.get("tool", {})
    probed = sum(1 for t in tools.values() if t.get("smoke"))
    print(
        "check-smoke-or-reason OK — %d of %d [tool.*] declare a `smoke` probe; "
        "the other %d each carry a reason." % (probed, len(tools), len(rows))
    )
    return 0


if __name__ == "__main__":
    self_test()
    sys.exit(main(sys.argv[1:]))
