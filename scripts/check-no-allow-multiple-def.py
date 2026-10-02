#!/usr/bin/env python3
"""Phase 251 / issue 1618 — forbid `--allow-multiple-definition` in the build system.

The flag lets two different functions with the same name coexist and binds
callers to whichever copy the linker meets first (archive order / --gc-sections
dependent) — the #48-class wrong-copy hazard. The safe default is "duplicate
defined symbol => link error". `-z muldefs` is the SAME flag under ld's other
spelling (issue 0425 reached for it), so both spellings are one rule.

A use fails unless its file is in the audited allowlist
(`scripts/allow-multiple-def-allowlist.txt`) with the EXACT number of uses the
file carries and a reason + owning issue. Exact, both ways: one more use in an
allowlisted file is a new, unaudited use; one fewer is progress the list must
record (phase-472 W9 — a count that may only fall is forced down).

POPULATION (issue 1618). The rule is about BUILD FILES, wherever they live, so
the population is the file KINDS a link line can be written in —
`scripts/lib/file_kinds.py` `cmake shell just make ci jinja` — not a directory
list. The old list (`cmake/**`, `scripts/**`, `just/**`, examples/packages
CMake, the root files) read none of `zephyr/CMakeLists.txt`,
`integrations/nuttx/Make.defs` or `integrations/s32ds/makefile.defs`, so four
live uses sat beside an allowlist that said "ANY use fails the gate".

A COMMENT that names the flag is not a use (`scripts/lib/comments.py`).

Run: python3 scripts/check-no-allow-multiple-def.py   (selftest runs first)
"""
from __future__ import annotations

import os
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "scripts" / "lib"))

import comments  # noqa: E402
import file_kinds  # noqa: E402
import population  # noqa: E402

GATE = "no-allow-multiple-def"
ALLOWLIST = "scripts/allow-multiple-def-allowlist.txt"
KINDS = ("cmake", "shell", "just", "make", "ci", "jinja")
FLAG_RE = re.compile(r"allow[-_]multiple[-_]definition|\bmuldefs\b")

# The stripper each kind's comments need. `make` comments are `#`-to-EOL like
# shell; a jinja template is matched RAW (no stripper models it), which fails
# closed: a comment there counts as a use.
_KIND_LANG = {"cmake": "cmake", "shell": "sh", "just": "just", "make": "sh", "ci": "yaml"}


def lang_of(path: str) -> str | None:
    lang = comments.lang_for(path)
    if lang:
        return lang
    for kind in sorted(file_kinds.kind_of(path)):
        if kind in _KIND_LANG:
            return _KIND_LANG[kind]
    if path.endswith(".cmake.in"):
        return "cmake"
    return None


def uses_in(text: str, lang: str | None) -> list[int]:
    """1-based line numbers of real (non-comment) uses of the flag."""
    code = comments.strip_comments(text, lang) if lang else text
    return [i for i, ln in enumerate(code.splitlines(), 1) if FLAG_RE.search(ln)]


def parse_allowlist(text: str) -> tuple[dict, list[str]]:
    """`<path> <count>  # reason (issue)` rows -> {path: count}; malformed rows."""
    table, bad = {}, []
    for raw in text.splitlines():
        body, _, reason = raw.partition("#")
        body = body.strip()
        if not body:
            continue
        parts = body.split()
        if len(parts) != 2 or not parts[1].isdigit() or int(parts[1]) < 1:
            bad.append(f"{raw!r}: want `<path> <count>  # reason (issue NNNN)`")
        elif not re.search(r"\b(issue|#)\s*\d{3,4}\b", reason):
            bad.append(f"{raw!r}: the reason must name an owning issue")
        elif parts[0] in table:
            bad.append(f"{raw!r}: duplicate row")
        else:
            table[parts[0]] = int(parts[1])
    return table, bad


def judge(found: dict, allowed: dict) -> list[str]:
    """`found` = {path: [lines]}; returns the failure lines (empty = clean)."""
    errs = []
    for path, lines in sorted(found.items()):
        want = allowed.get(path, 0)
        if len(lines) > want:
            where = ", ".join(f"{path}:{n}" for n in lines)
            errs.append(
                f"{path}: {len(lines)} use(s), allowlist permits {want} — {where}")
    for path, want in sorted(allowed.items()):
        have = len(found.get(path, []))
        if have < want:
            errs.append(
                f"{path}: allowlist permits {want} use(s) but the file has {have} — "
                f"lower (or drop) the row in {ALLOWLIST}; the count may only fall")
    return errs


def self_test() -> None:
    # Normal path: a real use, ld's `-z muldefs` spelling, and comments that
    # merely NAME the flag (must not count).
    cm = ("# --allow-multiple-definition is banned\n"
          "zephyr_ld_options(-Wl,--allow-multiple-definition)\n"
          "target_link_options(t PRIVATE -Wl,-z,muldefs)  # muldefs\n")
    assert uses_in(cm, "cmake") == [2, 3], uses_in(cm, "cmake")
    mk = "# Phase 157 --allow-multiple-definition\nEXTRA_LIBPATHS += --allow-multiple-definition\n"
    assert uses_in(mk, "sh") == [2]
    assert lang_of("integrations/s32ds/makefile.defs") == "sh"
    assert lang_of("integrations/nuttx/Make.defs") == "sh"
    assert lang_of("zephyr/CMakeLists.txt") == "cmake"
    # Exact counts, both directions.
    assert judge({"a": [1, 2]}, {"a": 2}) == []
    assert judge({"a": [1, 2, 9]}, {"a": 2}), "a THIRD use in an allowlisted file must fail"
    assert judge({"a": [1]}, {"a": 2}), "a fallen count must force the row down"
    assert judge({"b": [4]}, {}), "an unlisted use must fail"
    assert judge({}, {"a": 1}), "a stale row must fail"
    table, bad = parse_allowlist(
        "x/y 2  # reason (issue 1636)\nx/z 1  # no owner\nx/w  # no count (issue 1636)\n")
    assert table == {"x/y": 2} and len(bad) == 2, (table, bad)


def main() -> int:
    self_test()
    os.chdir(REPO)
    files = file_kinds.files_of_kind(*KINDS)
    if not population.require_population(files, "build file(s)", gate=GATE):
        return 1
    allowed, bad = parse_allowlist(Path(ALLOWLIST).read_text())
    found = {}
    for f in files:
        if not os.path.isfile(f):
            continue
        lines = uses_in(Path(f).read_text(errors="replace"), lang_of(f))
        if lines:
            found[f] = lines
    errs = [f"malformed allowlist row {b}" for b in bad] + judge(found, allowed)
    if errs:
        print(f"✗ {GATE}: `--allow-multiple-definition` / `-z muldefs` outside the audit:",
              file=sys.stderr)
        for e in errs:
            print(f"   {e}", file=sys.stderr)
        print("   Remove the flag (a duplicate defined symbol must be a link error), or —\n"
              f"   if genuinely unavoidable — record `<path> <count>` in {ALLOWLIST}\n"
              "   with a reason and an owning issue.", file=sys.stderr)
        return 1
    n = sum(allowed.values())
    print(f"✓ {GATE}: {n} audited use(s) in {len(allowed)} file(s), all allowlisted (target: 0).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
