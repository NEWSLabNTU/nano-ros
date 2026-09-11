#!/usr/bin/env python3
"""Every `[tool.*]` in nros-sdk-index.toml declares a dist or a recorded
reason — issue 1273 / RFC-0099 D4 / phase-447 C2.

WHY THIS GATE EXISTS

`plan_install` (packages/cli/nros-cli-core/src/orchestration/sdk_index.rs)
already prefers a prebuilt: `Provenance::read` -> `dist_for(host)` ->
`[tool.*.source]`, in that order. So a tool that source-builds during
`nros setup` is never "a tool that must be compiled" — it is a tool for which
this index carries no `dist.<host>` row. RFC-0099 D4 measured the cost: a
source build takes the whole machine while the network sits idle, so it is a
serialisation point removed, not only minutes, and the fix belongs in the
index, never in CLI code (issue 1273's own framing).

That makes "no dist" a fact worth tracking on its own, same as a script with
no selftest (`check-gate-selftests`) or a knob with no floor
(`check-c-array-pool-floors`): every dist-less tool needs EITHER a `dist` row
OR a documented reason it has none, and the set of "reason" tools may only
SHRINK as dist rows get added.

WHY A BASELINE RATCHET, NOT A ONE-TIME SURVEY

A survey answers "how many today". A ratchet answers it on every push and
refuses to let it grow back: `.config/dist-or-reason-baseline.txt` is exactly
the dist-less tool set as of the day it was written, and this gate fails in
BOTH directions —

  * a NEW dist-less tool with no baseline line: the debt grew silently, the
    same shape `check-gate-selftests` calls the issue-0743 class;
  * a baseline line whose tool now HAS a dist, or no longer exists: the
    baseline is stale in the SAFE direction, which is still wrong — a ratchet
    that only ever tightens on the READER's honesty is not a ratchet.

The reason text itself lives in nros-sdk-index.toml, next to `[tool.<name>]`,
where the next person editing that tool will actually read it — the baseline
line is a one-line pointer for the ratchet's bookkeeping, not the record.

Usage::

    check-dist-or-reason.py                  # the gate
    check-dist-or-reason.py --audit          # full picture, never fails
    check-dist-or-reason.py --write-baseline # after adding/removing a dist row
    check-dist-or-reason.py --selftest       # the negative controls alone
"""

from __future__ import annotations

import sys
from pathlib import Path

try:
    import tomllib  # Python 3.11+
except ModuleNotFoundError:  # 3.10 backport, same spelling as the sibling gates
    import tomli as tomllib

ROOT = Path(__file__).resolve().parents[2]
INDEX = ROOT / "nros-sdk-index.toml"
BASELINE = ROOT / ".config" / "dist-or-reason-baseline.txt"

# Module-level so `check-baseline-shape` can hold the file to it (rule 3).
BASELINE_HEADER = (
    "# `[tool.*]` entries in nros-sdk-index.toml with no `dist.<host>` row, each\n"
    "# with the reason it has none. A RATCHET, not an allowlist: RFC-0099 D4 says a\n"
    "# source build never means \"this tool must be compiled\" — it means the index\n"
    "# has no dist row for the host — so every name below is a standing decision\n"
    "# recorded in the index's own comment beside `[tool.<name>]`, never a TODO.\n"
    "#\n"
    "# `check-dist-or-reason` fails when a tool here GAINS a `dist.<host>` (remove\n"
    "# its line) or a dist-less tool appears with NO line at all (add one, with the\n"
    "# reason written in the index next to the entry, not only here) — so the debt\n"
    "# can only shrink and a fixed gap cannot silently linger, the issue-0743 class\n"
    "# one ratchet over.\n"
    "#\n"
    "# Regenerate: python3 scripts/sdk/check-dist-or-reason.py --write-baseline\n"
)


def load_index(path: Path) -> dict:
    """{tool name: has_dist bool}, parsed straight from the TOML — no second
    schema. A missing/unparsable index is a TOOL failure, not a finding of
    zero tools (mirrors nros_grep_q's rc>=2 rule one layer up: a check that
    cannot read its input must not report a verdict about it)."""
    if not path.is_file():
        sys.exit(f"check-dist-or-reason: index missing at {path}")
    try:
        with path.open("rb") as fh:
            data = tomllib.load(fh)
    except Exception as e:  # noqa: BLE001 — report, do not draw a conclusion
        sys.exit(f"check-dist-or-reason: {path} did not parse: {e}")
    tools = data.get("tool", {})
    if not tools:
        sys.exit(f"check-dist-or-reason: {path} declares no [tool.*] at all — "
                  "that is a parse-shape bug, not an empty index.")
    return {name: bool(t.get("dist")) for name, t in tools.items()}


def dist_less(tools: dict) -> set[str]:
    return {name for name, has_dist in tools.items() if not has_dist}


def _display(path: Path) -> str:
    try:
        return str(path.relative_to(ROOT))
    except ValueError:
        return str(path)


def load_baseline() -> dict[str, str]:
    """{tool name: raw line}, from the data rows (non-blank, non-`#`)."""
    if not BASELINE.exists():
        raise SystemExit(
            f"check-dist-or-reason: baseline missing at {BASELINE}.\n"
            "  It is tracked, so its absence is a PATH bug, not an empty ratchet.\n"
            "  Regenerate deliberately with --write-baseline."
        )
    out = {}
    with BASELINE.open(encoding="utf-8") as fh:
        for line in fh:
            stripped = line.strip()
            if not stripped or stripped.startswith("#"):
                continue
            name = stripped.split()[0]
            out[name] = line.rstrip("\n")
    return out


def write_baseline(tools: dict) -> None:
    prev = load_baseline() if BASELINE.exists() else {}
    names = sorted(dist_less(tools))
    with BASELINE.open("w", encoding="utf-8") as fh:
        fh.write(BASELINE_HEADER)
        for name in names:
            if name in prev:
                fh.write(prev[name] + "\n")
            else:
                fh.write(f"{name}    # TODO: record why no dist row exists, "
                          f"here AND in nros-sdk-index.toml\n")
    print(f"wrote {BASELINE} — {len(names)} tool(s) with no dist")


def main(argv: list[str]) -> int:
    tools = load_index(INDEX)

    if "--write-baseline" in argv:
        write_baseline(tools)
        return 0

    current = dist_less(tools)

    if "--audit" in argv:
        print(f"[tool.*] entries: {len(tools)}")
        print(f"  {len(tools) - len(current):3d}  carry a dist.<host> row")
        print(f"  {len(current):3d}  do not:")
        for name in sorted(current):
            print(f"        {name}")
        return 0

    baseline = load_baseline()
    errs = []

    for name in sorted(current):
        if name not in baseline:
            errs.append(
                f"{name}: no `dist.<host>` row and no recorded reason.\n"
                f"      Either give it a `dist.<host>` (the [tool.zephyr-sdk]\n"
                f"      pattern if upstream publishes one), or record WHY not,\n"
                f"      in nros-sdk-index.toml next to [tool.{name}] AND as a\n"
                f"      line in {_display(BASELINE)}."
            )

    for name, line in sorted(baseline.items()):
        if name not in tools:
            errs.append(
                f"{name}: in the baseline but names no [tool.*] in the index "
                f"any more.\n      Delete the line — a stale entry is inert "
                f"while reading as tracked debt."
            )
        elif name not in current:
            errs.append(
                f"{name}: now carries a `dist.<host>` — remove it from the "
                f"baseline.\n      The ratchet only tightens; leaving it here "
                f"lets the gap be silently reopened later."
            )

    if errs:
        print(f"check-dist-or-reason: {len(errs)} problem(s):\n", file=sys.stderr)
        for e in errs:
            print(f"  - {e}", file=sys.stderr)
        print(f"\n  Baseline: {_display(BASELINE)} "
              f"(--write-baseline after fixing, --audit for the full picture)",
              file=sys.stderr)
        return 1

    print(f"check-dist-or-reason OK — {len(tools) - len(current)}/{len(tools)} "
          f"[tool.*] carry a dist; {len(current)} have a recorded reason and "
          f"may only decrease.")
    return 0


def self_test(quiet: bool = True) -> list[str]:
    """Prove the classifier can fail, on EVERY invocation — a negative control
    nobody runs decays into a comment (the check-board-tiers.py rule)."""
    import io
    import tempfile
    from contextlib import redirect_stdout, redirect_stderr

    fails: list[str] = []

    def expect(name, got, want):
        if got != want:
            fails.append(f"{name}: got {got!r}, want {want!r}")

    good_index = (
        b'[tool.has-dist]\nversion = "1"\n'
        b'dist.linux-x86_64 = { url = "https://example/a", sha256 = "aa" }\n'
        b'[tool.reasoned]\nversion = "1"\n'
    )
    good_baseline = BASELINE_HEADER + "reasoned    # a reason\n"
    bad_baseline_missing = BASELINE_HEADER  # `reasoned` has no line
    bad_baseline_stale = BASELINE_HEADER + "reasoned    # a reason\nhas-dist    # stale\n"

    with tempfile.TemporaryDirectory() as td:
        tdp = Path(td)
        idx = tdp / "index.toml"
        base = tdp / "baseline.txt"
        global INDEX, BASELINE
        real_index, real_baseline = INDEX, BASELINE
        try:
            INDEX, BASELINE = idx, base

            idx.write_bytes(good_index)
            base.write_text(good_baseline, encoding="utf-8")
            buf = io.StringIO()
            with redirect_stdout(buf), redirect_stderr(buf):
                rc = main([])
            expect("a fully-reasoned dist-less set passes", rc, 0)

            base.write_text(bad_baseline_missing, encoding="utf-8")
            buf = io.StringIO()
            with redirect_stdout(buf), redirect_stderr(buf):
                rc = main([])
            expect("an undeclared dist-less tool fails", rc, 1)
            expect("... and names it", "reasoned" in buf.getvalue(), True)

            base.write_text(bad_baseline_stale, encoding="utf-8")
            buf = io.StringIO()
            with redirect_stdout(buf), redirect_stderr(buf):
                rc = main([])
            expect("a baseline entry for a tool that now has a dist fails", rc, 1)
            expect("... and names it", "has-dist" in buf.getvalue(), True)

            # A tool retired from the index entirely: baseline outlives it.
            idx.write_bytes(
                b'[tool.reasoned]\nversion = "1"\n'
                b'dist.linux-x86_64 = { url = "https://example/a", sha256 = "aa" }\n'
            )
            base.write_text(good_baseline, encoding="utf-8")
            buf = io.StringIO()
            with redirect_stdout(buf), redirect_stderr(buf):
                rc = main([])
            expect("a baseline entry naming no tool fails", rc, 1)

            # write-baseline preserves an existing reason and TODOs a new one.
            idx.write_bytes(good_index)
            base.write_text(BASELINE_HEADER, encoding="utf-8")
            buf = io.StringIO()
            with redirect_stdout(buf):
                main(["--write-baseline"])
            written = base.read_text(encoding="utf-8")
            expect("write-baseline TODOs a fresh entry", "TODO" in written, True)
        finally:
            INDEX, BASELINE = real_index, real_baseline

    if not quiet:
        if fails:
            print("check-dist-or-reason --selftest: FAILED", file=sys.stderr)
            for f in fails:
                print(f"  {f}", file=sys.stderr)
        else:
            print("check-dist-or-reason --selftest: OK")
    return fails


if __name__ == "__main__":
    argv = sys.argv[1:]
    if "--selftest" in argv or "--self-test" in argv:
        sys.exit(1 if self_test(quiet=False) else 0)
    # Runs on the NORMAL path too — a selftest behind a flag alone is prose by
    # the next release (check-gate-selftests' own rule, applied to itself).
    fails = self_test(quiet=True)
    if fails:
        print("check-dist-or-reason: internal selftest FAILED (this is a bug "
              "in the gate, not in nros-sdk-index.toml):", file=sys.stderr)
        for f in fails:
            print(f"  {f}", file=sys.stderr)
        sys.exit(2)
    sys.exit(main(argv))
