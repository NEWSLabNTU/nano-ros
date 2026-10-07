#!/usr/bin/env python3
"""The compile-check rows a lane must BUILD for the tests it runs — issue 1656.

`test-lane-contracts` runs the targets the gate-image census admitted
(`.config/lane-admission/gate.txt`). Some of them read a build-stage compile
stamp or verdict (`require_compile_check`, `require_compile_verdict`), and a
compile tier may resolve such a stamp only if the lane PRODUCES it
(`check-lane-contracts`). Which stamps those are is a question about the
tests' source, so it is DERIVED here — never a hand list beside the admission
list, which would drift from it one admitted target at a time.

The census needs the same rows for the opposite reason: it runs every target
with what the lane provisions, and a target whose stamp it did not stage
classifies FIXTURE and is never admitted (issue 1656's 2026-10-03 finding:
four verdict targets left the gate lane exactly that way).

    lane-compile-stamps.py --admission .config/lane-admission/gate.txt
        the stamp rows the ADMITTED tests read (what `test-lane-contracts` builds)
    lane-compile-stamps.py --census
        the stamp rows every compile-resolver target reads (what the census stages)

Prints ONE comma-separated line, the spelling `NROS_FIXTURE_IDS` takes. Only
rows whose whole artifact is a stamp or verdict are named
(`STAMP_COMPILE_CHECK_BUILDERS` in `fixtures-manifest.py`): a row that builds a
binary a test runs is a fixture, whatever resolver reaches it.

Both modes call `check-lane-contracts.py`'s own functions, so the gate checks
the very set this prints.
"""

import importlib.util
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def _contracts():
    path = os.path.join(ROOT, "scripts", "check-lane-contracts.py")
    spec = importlib.util.spec_from_file_location("nros_check_lane_contracts", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def main(argv):
    if argv[:1] == ["--census"] and len(argv) == 1:
        ids = _contracts().census_stamp_ids()
    elif argv[:1] == ["--admission"] and len(argv) == 2:
        m = re.fullmatch(r"(?:.*/)?\.config/lane-admission/([A-Za-z0-9_-]+)\.txt", argv[1])
        if not m:
            sys.stderr.write("lane-compile-stamps: --admission takes "
                             ".config/lane-admission/<lane>.txt\n")
            return 2
        ids = _contracts().admission_stamp_ids(m.group(1))
    else:
        sys.stderr.write(__doc__)
        return 2
    print(",".join(sorted(ids)))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
