#!/usr/bin/env python3
"""Every cargo-directory CALL SITE carries the knob half of the key — issue 1616 (W7).

`check-cargo-dir-knob-key.sh` proves `nros_knob_key_fields()` separates two
images that differ only in a knob. That proof was about the HELPER; whether
each caller of `nros_shared_cargo_dir()` / `nros_share_corrosion_cargo_dir()`
actually appends it was an authored belief ("every caller ... appends it"). A
caller that replaced the fields with `set(_nnbe_knob_fields "")` passed, and two
NuttX images differing only in `ZPICO_MAX_QUERYABLES` would then share one
cargo directory (RFC-0094 D4, issue 1025's shape).

So the call sites are HARVESTED from every CMake file (`file_kinds cmake`), and
for each one the key must reach a variable whose LAST assignment before the
call is `nros_knob_key_fields(<var>)` — either directly (`${var}` in the call)
or through one `list(APPEND <key> ${var})` whose `${key}` is in the call.

Two sites are exempt, each with its reason (`scripts/lib/harvest.py`, so a
stale exemption fails): the corrosion wrapper forwards its caller's `${ARGN}`,
and the key probe builds keys on purpose with and without the fields.

Run: python3 scripts/check-cargo-dir-knob-key-sites.py   (selftest runs first)
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "scripts" / "lib"))

import comments  # noqa: E402
import file_kinds  # noqa: E402
import harvest  # noqa: E402

CALL = re.compile(r"\b(nros_shared_cargo_dir|nros_share_corrosion_cargo_dir)\s*\(")
DEF = re.compile(r"\bfunction\s*\(\s*(nros_shared_cargo_dir|nros_share_corrosion_cargo_dir)\b")
KNOB = re.compile(r"\bnros_knob_key_fields\s*\(\s*([A-Za-z_][A-Za-z0-9_]*)\s*\)")
VAR = re.compile(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}")

EXEMPT = {
    "cmake/NanoRosCorrosion.cmake:nros_shared_cargo_dir":
        "the `nros_share_corrosion_cargo_dir` wrapper forwards its caller's ${ARGN}; "
        "its CALLERS are checked here",
    "scripts/lib/shared-cargo-key-probe.cmake:nros_shared_cargo_dir":
        "the key probe builds a key WITHOUT the fields on purpose (the "
        "negative control in check-cargo-dir-knob-key.sh)",
}


def _close(code: str, i: int) -> int:
    depth = 0
    for j in range(i, len(code)):
        if code[j] == "(":
            depth += 1
        elif code[j] == ")":
            depth -= 1
            if depth == 0:
                return j
    return len(code)


def _last_assignment_is_knob(before: str, var: str) -> bool:
    """True if the last statement assigning `var` in `before` is the helper."""
    assign = re.compile(
        r"\b(?:set|unset|nros_knob_key_fields|list\s*\(\s*(?:APPEND|PREPEND|REMOVE_ITEM|"
        r"REMOVE_AT|FILTER|TRANSFORM|INSERT))\s*\(?\s*" + re.escape(var) + r"\b")
    last = None
    for m in assign.finditer(before):
        last = m
    return bool(last) and last.group(0).lstrip().startswith("nros_knob_key_fields")


def sites(text: str):
    """[(callee, ok)] for each call (not definition) in one CMake text."""
    code = comments.strip_comments(text, "cmake")
    out = []
    for m in CALL.finditer(code):
        line_start = code.rfind("\n", 0, m.start()) + 1
        if DEF.search(code[line_start:m.end()]):
            continue
        open_i = m.end() - 1
        args = code[open_i:_close(code, open_i) + 1]
        before = code[:m.start()]
        used = set(VAR.findall(args))
        ok = any(_last_assignment_is_knob(before, v) for v in used)
        if not ok:
            # One level of indirection: `list(APPEND <key> ${knob})`.
            for km in KNOB.finditer(before):
                k = km.group(1)
                if not _last_assignment_is_knob(before, k):
                    continue
                for v in used:
                    app = re.search(r"\blist\s*\(\s*APPEND\s+" + re.escape(v) + r"\b[^)]*\$\{"
                                    + re.escape(k) + r"\}", before)
                    if app:
                        ok = True
        out.append((m.group(1), ok))
    return out


def self_test() -> None:
    good = ("nros_knob_key_fields(_k)\nnros_shared_cargo_dir(_d KEY\n  \"a=b\"\n  ${_k})\n")
    assert sites(good) == [("nros_shared_cargo_dir", True)], sites(good)
    blanked = good.replace("nros_knob_key_fields(_k)\n", "nros_knob_key_fields(_k)\nset(_k \"\")\n")
    assert sites(blanked) == [("nros_shared_cargo_dir", False)], "a blanked knob var must fail"
    missing = "nros_shared_cargo_dir(_d KEY \"a=b\")\n"
    assert sites(missing) == [("nros_shared_cargo_dir", False)]
    via = ("set(_key \"t=1\")\nnros_knob_key_fields(_kf)\nlist(APPEND _key ${_kf})\n"
           "nros_shared_cargo_dir(_dir KEY ${_key})\n")
    assert sites(via) == [("nros_shared_cargo_dir", True)], sites(via)
    commented = "# nros_knob_key_fields(_k)\nnros_shared_cargo_dir(_d KEY ${_k})\n"
    assert sites(commented) == [("nros_shared_cargo_dir", False)]
    assert sites("function(nros_shared_cargo_dir out)\nendfunction()\n") == []


def main() -> int:
    self_test()
    found, bad = [], []
    for rel in file_kinds.files_of_kind("cmake", repo=REPO):
        p = REPO / rel
        if not p.is_file():
            continue
        for callee, ok in sites(p.read_text(errors="replace")):
            key = f"{rel}:{callee}"
            found.append(key)
            if not ok and key not in EXEMPT:
                bad.append(key)
    checked, problems = harvest.reconcile(found, EXEMPT, what="cargo-dir call site")
    if bad or problems:
        print("check-cargo-dir-knob-key-sites: FAILED", file=sys.stderr)
        for b in bad:
            print(f"  {b}: the key does not carry `nros_knob_key_fields()` — two images "
                  f"differing only in a knob would share this cargo directory (RFC-0094 D4)",
                  file=sys.stderr)
        for p in problems:
            print(f"  {p}", file=sys.stderr)
        return 1
    print(f"check-cargo-dir-knob-key-sites: OK — {len(checked)} call site(s) carry the "
          f"knob fields ({len(EXEMPT)} exempt with a reason)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
