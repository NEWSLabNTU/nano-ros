#!/usr/bin/env python3
"""Phase-513 R7 — every Zephyr cargo build of the C/C++ lane carries the panic ending.

`zephyr/CMakeLists.txt` resolves the image's panic policy once, into
`_nros_panic_suffix` (`,panic-platform`, `,panic-halt`, or empty for
`PANIC own`). The suffix travels BESIDE the feature strings, not inside them,
so each cargo build has to append it itself.

Issue 2010 is the build that did not. The mixed workspace links one archive,
the `nros_ws_runtime` umbrella, and its feature list was built from
`_nros_cpp_features` alone. The umbrella's `nros-cpp` dependency is
`default-features = false`, so once phase-359 W10 stage C removed `std` (and
libstd's handler with it) that archive had no `#[panic_handler]`, and every
Zephyr mixed image stopped linking. The nros-c and nros-cpp builds beside it
were fine, because they appended the suffix.

`check-image-panic-policy` reads the Rust side (`nros::main!(panic = …)`) and
says outright that it cannot see the C/C++ side. This gate is that side, for
the one file that builds it on Zephyr.

THE RULE, read from `zephyr/CMakeLists.txt` with comments stripped, outside the
body of `_nros_zephyr_api_cargo_build` (which forwards the string it is given):

  1. Every `_nros_zephyr_api_cargo_build(<pkg> <features>)` call names
     `${_nros_panic_suffix}` in `<features>`.
  2. Every `nros_write_runtime_umbrella_crate(...)` call's `CPP_FEATURES` names
     it — directly, or through a variable whose LAST assignment before the call
     does.
  3. Every `nros_cargo_build(...)` call with `FEATURES` names it there. One
     without `FEATURES` must build a `MANIFEST_PATH` (a generated crate whose
     features rule 2 already checked); one with neither is refused.

The cmake lane's umbrella (`cmake/NanoRosRuntimeCrate.cmake`) adds the feature
through `nros_panic_policy_feature` and is not this file's spelling.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts" / "lib"))
import comments  # noqa: E402  phase-472 W3 — a comment is not code

TARGET = "zephyr/CMakeLists.txt"
SUFFIX = "${_nros_panic_suffix}"
HELPER = "_nros_zephyr_api_cargo_build"


def calls(text: str, name: str) -> list[tuple[int, str]]:
    """(offset, argument text) of every `name(...)` call, parens balanced."""
    out = []
    for m in re.finditer(rf"(?<![\w$]){re.escape(name)}\s*\(", text):
        depth, i = 1, m.end()
        while i < len(text) and depth:
            depth += {"(": 1, ")": -1}.get(text[i], 0)
            i += 1
        out.append((m.start(), text[m.end():i - 1]))
    return out


def helper_body(text: str) -> tuple[int, int]:
    m = re.search(rf"function\s*\(\s*{HELPER}\b", text)
    if not m:
        return (-1, -1)
    e = re.search(r"\bendfunction\s*\(", text[m.end():])
    return (m.start(), m.end() + (e.end() if e else len(text)))


def last_assignment(text: str, var: str, before: int) -> str | None:
    """The argument text of the last `set(var …)` / `string(… var …)` before."""
    best = None
    for off, args in calls(text, "set") + calls(text, "string"):
        if off >= before:
            continue
        toks = args.split()
        if not toks:
            continue
        if (toks[0] == var) or (len(toks) > 1 and var in toks[1:4] and toks[0] != var):
            if best is None or off > best[0]:
                best = (off, args)
    return best[1] if best else None


def line_of(text: str, off: int) -> int:
    return text.count("\n", 0, off) + 1


def keyword_value(args: str, key: str) -> str | None:
    m = re.search(rf"\b{key}\b\s+(.*?)(?=\s+\b[A-Z_]{{3,}}\b\s|\s*$)", args, re.S)
    return m.group(1).strip() if m else None


def problems(path: str, raw: str) -> list[str]:
    text = comments.strip_comments(raw, "cmake")
    lo, hi = helper_body(text)
    inside = lambda off: lo <= off < hi  # noqa: E731
    out = []

    for off, args in calls(text, HELPER):
        if inside(off):
            continue
        if SUFFIX not in args:
            out.append(f"{path}:{line_of(text, off)}: `{HELPER}` features omit {SUFFIX}")

    for off, args in calls(text, "nros_write_runtime_umbrella_crate"):
        val = keyword_value(args, "CPP_FEATURES") or ""
        ok = SUFFIX in val
        m = re.fullmatch(r"\$\{(\w+)\}", val)
        if not ok and m:
            a = last_assignment(text, m.group(1), off)
            ok = a is not None and SUFFIX in a
        if not ok:
            out.append(
                f"{path}:{line_of(text, off)}: `nros_write_runtime_umbrella_crate` "
                f"CPP_FEATURES `{val}` does not carry {SUFFIX}"
            )

    for off, args in calls(text, "nros_cargo_build"):
        if inside(off):
            continue
        feats = keyword_value(args, "FEATURES")
        if feats is not None:
            if SUFFIX not in feats:
                out.append(f"{path}:{line_of(text, off)}: `nros_cargo_build` FEATURES omit {SUFFIX}")
        elif not re.search(r"\bMANIFEST_PATH\b", args):
            out.append(
                f"{path}:{line_of(text, off)}: `nros_cargo_build` has neither "
                f"FEATURES nor a generated MANIFEST_PATH, so nothing carries {SUFFIX}"
            )
    return out


def self_test() -> None:
    helper = (
        f"function({HELPER} pkg features)\n"
        '  nros_cargo_build(PACKAGE ${pkg} FEATURES "${features}")\n'
        "endfunction()\n"
    )
    good = helper + (
        f'{HELPER}(nros-c "${{_f}}{SUFFIX}")\n'
        '# nros_cargo_build(PACKAGE x FEATURES "a")\n'
        f'string(REPLACE "," ";" _umb "${{_cpp}}{SUFFIX}")\n'
        "nros_write_runtime_umbrella_crate(\n  OUT_DIR x\n  BACKEND zenoh\n"
        "  NODE_DIRS ${dirs}\n  CPP_FEATURES ${_umb})\n"
        'nros_cargo_build(\n  PACKAGE u\n  MANIFEST_PATH "x/Cargo.toml"\n)\n'
    )
    assert problems("z", good) == [], problems("z", good)

    # The issue-2010 shape: the umbrella's list is built without the suffix.
    bad = good.replace(f'"${{_cpp}}{SUFFIX}"', '"${_cpp}"')
    got = problems("z", bad)
    assert len(got) == 1 and "umbrella" in got[0], got

    # A later assignment WITH the suffix after the call does not rescue it.
    late = bad + f'string(REPLACE "," ";" _umb "${{_cpp}}{SUFFIX}")\n'
    assert len(problems("z", late)) == 1, problems("z", late)

    # A helper call without it; a direct FEATURES build without it.
    got = problems("z", good.replace(f'"${{_f}}{SUFFIX}"', '"${_f}"'))
    assert len(got) == 1 and HELPER in got[0], got
    direct = good + 'nros_cargo_build(PACKAGE p FEATURES "${_f}")\n'
    assert len(problems("z", direct)) == 1, problems("z", direct)

    # Neither FEATURES nor MANIFEST_PATH.
    bare = good + "nros_cargo_build(PACKAGE p)\n"
    got = problems("z", bare)
    assert len(got) == 1 and "neither" in got[0], got


def main(argv: list[str]) -> int:
    self_test()
    if "--self-test" in argv:
        print("check-zephyr-panic-suffix: self-test ok")
        return 0
    path = REPO / TARGET
    raw = path.read_text()
    text = comments.strip_comments(raw, "cmake")
    found = problems(TARGET, raw)
    n = sum(len(calls(text, c)) for c in (HELPER, "nros_write_runtime_umbrella_crate", "nros_cargo_build"))
    if helper_body(text)[0] < 0:
        found.append(f"{TARGET}: `{HELPER}` is not defined any more; update this gate")
    if found:
        print("check-zephyr-panic-suffix: a Zephyr cargo build drops the panic ending")
        for f in found:
            print("  " + f)
        print(
            f"  Append {SUFFIX} to its features (phase-513 R7, issue 2010).\n"
            "  `PANIC own` sets the suffix empty, so appending it is always right."
        )
        return 1
    print(f"check-zephyr-panic-suffix: {n} cargo build call(s) in {TARGET}, each carries the panic ending")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
