#!/usr/bin/env python3
"""Every ratchet baseline keeps the header a person wrote. Issue 1241's class.

A baseline's comments are the part no gate reads, so a `sort` that scrambles
them leaves every lane green. 1241 found it in the prose-issue-ref baseline
and fixed that one file; the gate-selftest baseline then sat on main with its
header in byte order and a spacer lost to `sort -u`. The rules and their
measurements are in `scripts/lib/baseline_shape.py`.

Scope:
  * every tracked `.config/*.txt`, against rules 1 and 2 (sorted comments);
  * every baseline a script declares as `BASELINE` beside a module-level
    `BASELINE_HEADER`, against rule 3 as well — the file's header must be the
    one its writer emits. The writer is IMPORTED, not parsed, so the header is
    compared with the very string `--write-baseline` writes.

A writer with a fixed header should declare `BASELINE_HEADER` and write it;
that is what opts its baseline into rule 3. Finding NO such writer fails —
it means discovery broke, and rule 3 would silently check nothing.

Usage:
    check-baseline-shape.py              # the gate (runs its selftest first)
    check-baseline-shape.py --selftest   # the selftest alone
"""

import importlib.util
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
import baseline_shape
from tracked import tracked  # issue 0721: index lookup, not a walk

DECLARES_HEADER = re.compile(r"^BASELINE_HEADER\s*=", re.MULTILINE)


def writers():
    """{baseline Path: (writer script Path, header)} for rule 3."""
    out = {}
    for i, script in enumerate(tracked("scripts", suffix=".py")):
        if not DECLARES_HEADER.search(script.read_text(encoding="utf-8")):
            continue
        spec = importlib.util.spec_from_file_location(f"_baseline_writer_{i}", script)
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        base = Path(mod.BASELINE)
        if not base.is_absolute():
            base = ROOT / base
        out[base.resolve()] = (script, mod.BASELINE_HEADER)
    return out


def main(argv):
    fails = baseline_shape.selftest()
    if fails:
        print("check-baseline-shape --selftest: FAILED", file=sys.stderr)
        for f in fails:
            print(f"  {f}", file=sys.stderr)
        return 1
    if "--selftest" in argv:
        print("check-baseline-shape --selftest: OK")
        return 0

    ws = writers()
    if not ws:
        print("check-baseline-shape: no script declares BASELINE_HEADER — "
              "discovery is broken, and rule 3 would check nothing.", file=sys.stderr)
        return 1
    files = set(tracked(".config", suffix=".txt")) | set(ws)
    errs = []
    for f in sorted(files):
        rel = f.relative_to(ROOT)
        script, header = ws.get(f, (None, None))
        if not f.is_file():
            errs.append(f"{rel}: declared as BASELINE by "
                        f"{script.relative_to(ROOT)} and not on disk")
            continue
        found = baseline_shape.problems(f.read_text(encoding="utf-8"), header)
        if not found:
            continue
        if script:
            fix = (f"`python3 {script.relative_to(ROOT)} --write-baseline` "
                   f"rewrites it — read the row diff too")
        else:
            fix = (f"restore the header from history (`git log -p -- {rel}`); "
                   f"it has no writer to regenerate it")
        errs.append(f"{rel}:\n    " + "\n    ".join(found) + f"\n    fix: {fix}. "
                    f"Never `sort` a baseline: the rows are a set, the header "
                    f"is prose.")
    if errs:
        print(f"check-baseline-shape: {len(errs)} baseline(s) lost their header's "
              f"shape:\n", file=sys.stderr)
        for e in errs:
            print(f"  {e}\n", file=sys.stderr)
        return 1
    print(f"check-baseline-shape OK — {len(files)} baseline(s); "
          f"{len(ws)} checked against their writer's header")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
