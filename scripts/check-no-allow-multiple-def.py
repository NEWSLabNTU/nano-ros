#!/usr/bin/env python3
"""Phase 251 / issue 1618 — forbid `--allow-multiple-definition` in the build system.

The flag lets two different functions with the same name coexist and binds
callers to whichever copy the linker meets first (archive order / --gc-sections
dependent) — the #48-class wrong-copy hazard. The safe default is "duplicate
defined symbol => link error". `-z muldefs` is the SAME flag under ld's other
spelling (issue 0425 reached for it), so both spellings are one rule.

ABSOLUTE (issue 1664): every use fails. There is no allowlist any more —
issues 1636, 1645 and 1664 removed the last four uses by fixing what each one
masked (each was measured as a real duplicate set, `REGISTRY` among them), so
the audited list reached its target of zero and was deleted. A use that seems
unavoidable is a duplicate symbol to fix at the source, not a row to add.

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


def judge(found: dict) -> list[str]:
    """`found` = {path: [lines]}; returns the failure lines (empty = clean)."""
    return [f"{path}: {len(lines)} use(s) — " + ", ".join(f"{path}:{n}" for n in lines)
            for path, lines in sorted(found.items())]


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
    # Absolute: any use, in any file, fails; none passes.
    assert judge({}) == []
    assert judge({"integrations/s32ds/makefile.defs": [48]}), "a single use must fail"


def main() -> int:
    self_test()
    os.chdir(REPO)
    files = file_kinds.files_of_kind(*KINDS)
    if not population.require_population(files, "build file(s)", gate=GATE):
        return 1
    found = {}
    for f in files:
        if not os.path.isfile(f):
            continue
        lines = uses_in(Path(f).read_text(errors="replace"), lang_of(f))
        if lines:
            found[f] = lines
    errs = judge(found)
    if errs:
        print(f"✗ {GATE}: `--allow-multiple-definition` / `-z muldefs` is forbidden:",
              file=sys.stderr)
        for e in errs:
            print(f"   {e}", file=sys.stderr)
        print("   Remove the flag. A duplicate defined symbol must be a link error: fix\n"
              "   the duplicate at its source (one runtime archive per image — issues\n"
              "   1636, 1645, 1664). There is no allowlist.", file=sys.stderr)
        return 1
    print(f"✓ {GATE}: 0 uses in {len(files)} build file(s).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
