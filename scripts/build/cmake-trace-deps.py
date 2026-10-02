#!/usr/bin/env python3
"""Issue 1620 — the listfiles a cmake CONFIGURE executed, as Make dep-info.

`cmake --trace-format=json-v1 --trace-redirect=<file>` writes one JSON object
per executed command, each naming the listfile it came from (`"file"`). That is
cmake's own record of what it READ, and — unlike `CMakeFiles/Makefile.cmake` or
`build.ninja`'s RERUN_CMAKE edge — it exists when the configure FAILS, which is
the only kind of configure a `cmake-configure-verdict` row runs on purpose.

Written as `verdict: <file> <file> …` so `dep-closure.py`, which already reads
every `*.d` under a row's build dir, folds it into the row's signature with no
new reader. Paths outside the checkout are kept here and dropped there, by the
one filter that decides "is this ours to watch".

Usage: cmake-trace-deps.py <trace.json> <out.d>
"""

from __future__ import annotations

import json
import sys
from pathlib import Path


def main() -> int:
    if len(sys.argv) != 3:
        sys.stderr.write("usage: cmake-trace-deps.py <trace.json> <out.d>\n")
        return 2
    trace, out = Path(sys.argv[1]), Path(sys.argv[2])
    files: set[str] = set()
    try:
        lines = trace.read_text(errors="replace").splitlines()
    except OSError as exc:
        # No trace means NO closure, and an empty closure hashes to a valid
        # signature that watches nothing outside the row's dir. Refuse.
        sys.stderr.write(f"cmake-trace-deps: cannot read {trace}: {exc}\n")
        return 1
    for line in lines:
        line = line.strip()
        if not line:
            continue
        try:
            rec = json.loads(line)
        except json.JSONDecodeError:
            continue
        f = rec.get("file") if isinstance(rec, dict) else None
        if f:
            files.add(f)
    if not files:
        sys.stderr.write(
            f"cmake-trace-deps: {trace} names no listfile — cmake did not trace, "
            "so this row would have no dependency closure\n"
        )
        return 1
    esc = (p.replace(" ", "\\ ") for p in sorted(files))
    out.write_text("verdict: " + " ".join(esc) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
