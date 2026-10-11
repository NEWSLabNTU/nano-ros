#!/usr/bin/env python3
"""phase-492 W1 — the C API's `nros_` names, counted down to zero.

RFC-0089 (amended 2026-10-12): the C user API takes upstream's spellings where
the contract conforms and `rclc_` where it is ours. The reviewed map of every
name is `docs/reference/c-api-rename-map.tsv` (`scripts/gen-c-rename-map.py`).
This gate holds two things about the public headers in
`packages/api/nros-c/include/nros/`:

1. EVERY `nros_` / `NROS_` name a header declares is in the map. A new name
   must be classified (renamed, or kept as seam / hook / build) before it
   lands, so the count below cannot be dodged by adding an unmapped name.
2. The number of names still spelled the OLD way that the map renames is a
   RATCHET: it may fall and never rise. Its recorded value is
   `.config/c-api-nros-names-baseline.txt`; lower it in the change that
   lowers the count (`--write-baseline`). Phase-492 ends with it at 0.

Comments and string literals are ignored: a comment that names a retired
spelling is history, not a declaration.

Run: python3 scripts/check-c-api-nros-names.py [--write-baseline]
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MAP = ROOT / "docs" / "reference" / "c-api-rename-map.tsv"
BASELINE = ROOT / ".config" / "c-api-nros-names-baseline.txt"
C_INCLUDE = ROOT / "packages" / "api" / "nros-c" / "include" / "nros"
TOKEN = re.compile(r"\b(nros_[a-z0-9_]+|NROS_[A-Z0-9_]+)\b")
COMMENT = re.compile(r"/\*.*?\*/|//[^\n]*", re.S)
STRING = re.compile(r'"(?:\\.|[^"\\])*"')
RENAMED = {"adopt", "alias", "ours", "ret"}


def load_map():
    rows = {}
    for line in MAP.read_text().splitlines():
        if not line or line.startswith("#"):
            continue
        old, new, cls, _ = line.split("\t", 3)
        rows[old] = (new, cls)
    return rows


def tokens_of(text, name, out):
    code = STRING.sub('""', COMMENT.sub(" ", text))
    for t in TOKEN.findall(code):
        out.setdefault(t, name)
    return out


def header_tokens():
    out = {}
    for f in sorted(C_INCLUDE.glob("*.h")):
        tokens_of(f.read_text(errors="replace"), f.name, out)
    return out


def classify(rows, tokens):
    """(unmapped, pending): names the map does not know, and names still
    spelled the way the map renames."""
    unmapped = sorted(t for t in tokens if t not in rows)
    pending = sorted(t for t in tokens if t in rows and rows[t][1] in RENAMED)
    return unmapped, pending


def self_test():
    """Negative control on the normal path: a comment is not a declaration, a
    kept name is not pending, an unknown name is refused."""
    rows = {"nros_a_t": ("rclc_a_t", "ours"), "NROS_PUBLIC": ("NROS_PUBLIC", "build")}
    text = '/* nros_retired_t */ NROS_PUBLIC nros_a_t f(void); "nros_in_string"; nros_new(void);'
    unmapped, pending = classify(rows, tokens_of(text, "t.h", {}))
    ok = unmapped == ["nros_new"] and pending == ["nros_a_t"]
    if not ok:
        print(f"check-c-api-nros-names: SELF-TEST FAILED: unmapped={unmapped} pending={pending}",
              file=sys.stderr)
    return ok


def main():
    if not self_test():
        return 1
    rows = load_map()
    tokens = header_tokens()
    unmapped, pending = classify(rows, tokens)
    errors = []
    if unmapped:
        errors.append(
            "names the rename map does not classify (add them: "
            "python3 scripts/gen-c-rename-map.py, review, commit):\n"
            + "\n".join(f"    {t}  ({tokens[t]})" for t in unmapped)
        )
    count = len(pending)
    if "--write-baseline" in sys.argv:
        BASELINE.write_text(f"{count}\n")
        print(f"check-c-api-nros-names: baseline written — {count} name(s) still to rename")
        return 1 if errors else 0
    base = int(BASELINE.read_text().split()[0]) if BASELINE.exists() else None
    if base is None:
        errors.append(f"no baseline at {BASELINE.relative_to(ROOT)}; run --write-baseline")
    elif count > base:
        errors.append(
            f"{count} public C name(s) still use the old spelling, up from {base}. "
            "A new name takes its target spelling from the map, never `nros_`."
        )
    elif count < base:
        errors.append(
            f"{count} name(s) left to rename, down from {base} — lock the progress in: "
            "python3 scripts/check-c-api-nros-names.py --write-baseline"
        )
    if errors:
        print("check-c-api-nros-names: FAIL\n  " + "\n  ".join(errors), file=sys.stderr)
        return 1
    print(f"check-c-api-nros-names: OK — {count} name(s) left to rename (phase-492 ends at 0)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
