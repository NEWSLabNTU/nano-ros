#!/usr/bin/env python3
"""phase-488 — how many lines a stock rclcpp example needs edited to build here.

`upstream/<name>.cpp` is the file from ros2/examples VERBATIM (Apache-2.0,
header kept); `ported/<name>.cpp` is that file after the fewest edits that make
it compile, link and boot against nros-cpp. The count is the number of lines a
`diff -U0` adds or removes, and `edits.json` records it per example together
with WHY each remaining edit is needed.

`--check` fails when a count moves in EITHER direction:

  * up   — a regression: a header change made a stock rclcpp line stop porting.
  * down — a drop-in landed and the record is stale; re-run with `--write` and
           move the cause out of `remaining` so the porting guide stays true.

`--write` rewrites the counts (never the causes, which are prose a person owns).
Exits non-zero on any missing pair, so a deleted file cannot read as zero edits.
"""

import difflib
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
RECORD = HERE / "edits.json"


def edited_lines(upstream: Path, ported: Path) -> int:
    a = upstream.read_text().splitlines()
    b = ported.read_text().splitlines()
    n = 0
    for line in difflib.unified_diff(a, b, n=0, lineterm=""):
        if line.startswith(("+++", "---")):
            continue
        if line.startswith(("+", "-")):
            n += 1
    return n


def main() -> int:
    mode = sys.argv[1] if len(sys.argv) > 1 else "--check"
    if mode not in ("--check", "--write", "--report"):
        print(f"usage: {sys.argv[0]} [--check|--write|--report]", file=sys.stderr)
        return 2
    record = json.loads(RECORD.read_text())
    examples = record["examples"]
    upstream_names = {p.stem for p in (HERE / "upstream").glob("*.cpp")}
    ported_names = {p.stem for p in (HERE / "ported").glob("*.cpp")}
    errors = []
    for missing in sorted(upstream_names ^ ported_names):
        errors.append(f"{missing}: has no upstream/ported PAIR")
    for name in sorted(upstream_names ^ set(examples)):
        errors.append(f"{name}: upstream/ and edits.json disagree about whether it exists")
    if errors:
        for e in errors:
            print(f"ERROR: {e}", file=sys.stderr)
        return 1

    drift = []
    total = 0
    for name in sorted(examples):
        n = edited_lines(HERE / "upstream" / f"{name}.cpp", HERE / "ported" / f"{name}.cpp")
        total += n
        want = examples[name]["edited_lines"]
        if mode == "--report":
            print(f"  {name:45s} {n:4d}")
        if n != want:
            drift.append((name, want, n))
            examples[name]["edited_lines"] = n
    if mode == "--report":
        print(f"  {'total':45s} {total:4d}")
        return 0
    if mode == "--write":
        RECORD.write_text(json.dumps(record, indent=2) + "\n")
        print(f"edits.json: {len(drift)} count(s) updated, {total} edited lines in total")
        return 0
    if drift:
        for name, want, got in drift:
            way = "MORE edits — a regression" if got > want else "FEWER edits — record is stale"
            print(f"ERROR: {name}: edits.json says {want}, the diff says {got} ({way})",
                  file=sys.stderr)
        print("Fix the header (regression), or run `edit_count.py --write` and update the "
              "`remaining` causes plus the porting guide (improvement).", file=sys.stderr)
        return 1
    print(f"  ported rclcpp examples: {len(examples)} files, {total} edited lines, as recorded")
    return 0


if __name__ == "__main__":
    sys.exit(main())
