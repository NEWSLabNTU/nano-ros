#!/usr/bin/env python3
"""Issue 2758 -- every enum in a header that crosses the C ABI is pinned to int.

The C ABI headers (the rmw vtable, the platform API, the board API) are the
single source of truth, and their Rust side is bindgen output that
`scripts/gen-abi-bindings.sh` generates ON THE HOST. There an unfixed C enum is
`unsigned int`, so bindgen writes `c_uint` into a committed file that every
target compiles. arm-none-eabi's AAPCS default is variable-size enums
(`Tag_ABI_enum_size: small`): the same enum is ONE byte in C there. Issue 2758
measured what that does wherever an enum sits in memory -- a 4-byte store
through a 1-byte out-parameter, and a struct 60 bytes in C and 68 in Rust.

The fix pins the width in the header: each enum carries an enumerator
`= 0x7fffffff`, which makes it `int`-sized on every ABI, and a static assert
`sizeof(T) == sizeof(int)` beside it, so any C or C++ translation unit that
includes the header proves the width on its own target (issue 1330 did the
same for `rcl_log_severity_t`).

THE RULE. For every header bindgen reads -- the entry headers named in
`gen-abi-bindings.sh` and every header they include that resolves under one of
its `-I` directories -- each `enum` definition must have

  * an enumerator whose value is `0x7fffffff` (the width pin), and
  * a static assert of `sizeof(<type>) == sizeof(int)` in the same header,
    spelled out (`_Static_assert` / `static_assert`) or through a macro the
    header set defines whose body is exactly that assert.

The header set is PARSED from `gen-abi-bindings.sh`, never listed here, so a
new ABI header is covered the day bindgen starts reading it. A parse that finds
no bindgen run, or a run with no header, fails rather than passing on nothing.

Buildless. Self-tests on every run.
"""

from __future__ import annotations

import os
import re
import shlex
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
GEN = "scripts/gen-abi-bindings.sh"

PIN_RE = re.compile(r"=\s*(?:0x7[fF]{7}|2147483647|INT_MAX)\b")
ENUM_RE = re.compile(
    r"\b(typedef\s+)?enum\s+(\w+)?\s*\{(.*?)\}\s*(\w+)?\s*;", re.S
)
INCLUDE_RE = re.compile(r'^\s*#\s*include\s*[<"]([^>"]+)[>"]', re.M)
ASSIGN_RE = re.compile(r'^([A-Z_][A-Z0-9_]*)="([^"$`]*)"\s*$', re.M)


def strip_comments(text: str) -> str:
    text = re.sub(r"/\*.*?\*/", " ", text, flags=re.S)
    return re.sub(r"//[^\n]*", " ", text)


# ---------------------------------------------------------------------------
# The header set, from gen-abi-bindings.sh
# ---------------------------------------------------------------------------


def bindgen_runs(script: str) -> list[tuple[list[str], list[str]]]:
    """`[(entry headers, -I dirs)]`, one per bindgen invocation, paths relative
    to the repo root as the script spells them."""
    vars_ = dict(ASSIGN_RE.findall(script))

    def expand(word: str) -> str:
        return re.sub(r"\$\{?(\w+)\}?", lambda m: vars_.get(m.group(1), m.group(0)), word)

    # Wrapper heredocs: `cat > "$name" << 'TAG'` ... `TAG`.
    wrappers: dict[str, list[str]] = {}
    for m in re.finditer(
        r'cat\s*>\s*"\$(\w+)"\s*<<\s*\'?(\w+)\'?\n(.*?)\n\2\n', script, re.S
    ):
        wrappers[m.group(1)] = INCLUDE_RE.findall(m.group(3))

    runs = []
    # A bindgen command: `bindgen <input> \` continued to the line without `\`.
    for m in re.finditer(r"^\s*bindgen\s+((?:[^\n]*\\\n)*[^\n]*)$", script, re.M):
        words = shlex.split(m.group(1).replace("\\\n", " "))
        if not words or words[0].startswith("-"):
            continue  # `bindgen --version` and the like
        entry = words[0]
        incdirs = [expand(w[2:]) for w in words if w.startswith("-I")]
        wm = re.fullmatch(r"\$\{?(\w+)\}?", entry)
        if wm and wm.group(1) in wrappers:
            entries = wrappers[wm.group(1)]
            headers = []
            for inc in entries:
                for d in incdirs:
                    if os.path.isfile(os.path.join(ROOT, d, inc)):
                        headers.append(os.path.join(d, inc))
                        break
                else:
                    headers.append("<unresolved>" + inc)
        else:
            headers = [expand(entry)]
        runs.append((headers, incdirs))
    return runs


def header_closure(runs) -> list[str]:
    seen: list[str] = []
    for headers, incdirs in runs:
        todo = list(headers)
        while todo:
            h = todo.pop()
            if h in seen:
                continue
            seen.append(h)
            text = open(os.path.join(ROOT, h)).read()
            here = os.path.dirname(h)
            for inc in INCLUDE_RE.findall(text):
                for d in [here] + incdirs:
                    if os.path.isfile(os.path.join(ROOT, d, inc)):
                        todo.append(os.path.normpath(os.path.join(d, inc)))
                        break
    return sorted(seen)


# ---------------------------------------------------------------------------
# The rule
# ---------------------------------------------------------------------------


def assert_macros(texts: list[str]) -> set[str]:
    """Macros `M(T)` whose body is a `sizeof(T) == sizeof(int)` assert. Every
    definition of the name must be one (a C and a C++ arm, say)."""
    good: dict[str, bool] = {}
    for text in texts:
        flat = strip_comments(text).replace("\\\n", " ")
        for m in re.finditer(r"^\s*#\s*define\s+(\w+)\((\w+)\)\s*(.*)$", flat, re.M):
            name, arg, body = m.groups()
            ok = re.search(
                r"sizeof\s*\(\s*%s\s*\)\s*==\s*sizeof\s*\(\s*int\s*\)" % re.escape(arg), body
            ) is not None
            good[name] = good.get(name, True) and ok
    return {n for n, ok in good.items() if ok}


def violations(path: str, text: str, macros: set[str]) -> list[str]:
    out = []
    code = strip_comments(text)
    for m in ENUM_RE.finditer(code):
        is_typedef, tag, body, name = m.groups()
        if is_typedef and name:
            spell = re.escape(name)
            label = name
        elif tag:
            spell = r"enum\s+" + re.escape(tag)
            label = "enum " + tag
        else:
            out.append(f"{path}: an anonymous enum cannot be asserted; name it")
            continue
        if not PIN_RE.search(body):
            out.append(f"{path}: {label} has no width pin (an enumerator = 0x7fffffff)")
        raw = re.search(
            r"(?:_Static_assert|static_assert)\s*\(\s*sizeof\s*\(\s*%s\s*\)\s*==\s*sizeof\s*\(\s*int\s*\)"
            % spell,
            code,
        )
        via = any(
            re.search(r"\b%s\s*\(\s*%s\s*\)" % (re.escape(mac), spell), code) for mac in macros
        )
        if not raw and not via:
            out.append(f"{path}: {label} has no `sizeof({label}) == sizeof(int)` static assert")
    return out


# ---------------------------------------------------------------------------
# Self-test -- on every run
# ---------------------------------------------------------------------------


def self_test() -> None:
    macro = (
        "#define ASSERT_W(T) _Static_assert(sizeof(T) == sizeof(int), #T)\n"
    )
    good = macro + "typedef enum a_t { A0 = 0, A_PIN_ = 0x7fffffff, } a_t;\nASSERT_W(a_t);\n"
    raw = (
        "enum b { B0, B_PIN_ = 0x7fffffff };\n"
        "_Static_assert(sizeof(enum b) == sizeof(int), \"b\");\n"
    )
    no_pin = macro + "typedef enum c_t { C0 = 0 } c_t;\nASSERT_W(c_t);\n"
    no_assert = "typedef enum d_t { D0 = 0, D_PIN_ = 0x7fffffff } d_t;\n"
    commented = "typedef enum e_t { E0 = 0 /* = 0x7fffffff */ } e_t;\n" + macro + "ASSERT_W(e_t);\n"
    wrong_macro = (
        "#define ASSERT_W(T) _Static_assert(sizeof(T) <= 4, #T)\n"
        "typedef enum f_t { F0 = 0, F_PIN_ = 0x7fffffff } f_t;\nASSERT_W(f_t);\n"
    )
    cases = [
        (good, 0), (raw, 0), (no_pin, 1), (no_assert, 1), (commented, 1), (wrong_macro, 1),
    ]
    for i, (text, want) in enumerate(cases):
        got = len(violations(f"case{i}.h", text, assert_macros([text])))
        if got != want:
            sys.exit(f"check-abi-enum-width: SELFTEST FAIL -- case {i} gave {got} violation(s), want {want}")
    script = (
        'API="packages/core/nros-rmw-abi"\n'
        "w=\"$(mktemp)\"\n"
        "cat > \"$w\" << 'EOF'\n#include <nros/rmw_ret.h>\nEOF\n"
        'bindgen "$w" \\\n    --use-core \\\n    -- -I"$API/include"\n'
        'bindgen "$API/include/nros/rmw_event.h" -- -I"$API/include"\n'
    )
    runs = bindgen_runs(script)
    want = [
        (["packages/core/nros-rmw-abi/include/nros/rmw_ret.h"], ["packages/core/nros-rmw-abi/include"]),
        (["packages/core/nros-rmw-abi/include/nros/rmw_event.h"], ["packages/core/nros-rmw-abi/include"]),
    ]
    if runs != want:
        sys.exit(f"check-abi-enum-width: SELFTEST FAIL -- the script parse gave {runs!r}")


def main(argv: list[str]) -> int:
    self_test()
    if "--self-test" in argv:
        print("check-abi-enum-width: self-test ok")
        return 0
    runs = bindgen_runs(open(os.path.join(ROOT, GEN)).read())
    if not runs:
        print(f"check-abi-enum-width: found NO bindgen run in {GEN}; the parse is stale", file=sys.stderr)
        return 1
    bad = [h for headers, _ in runs for h in headers if h.startswith("<unresolved>")]
    if bad or any(not headers for headers, _ in runs):
        print(f"check-abi-enum-width: a bindgen input in {GEN} did not resolve: {bad}", file=sys.stderr)
        return 1
    headers = header_closure(runs)
    texts = {h: open(os.path.join(ROOT, h)).read() for h in headers}
    macros = assert_macros(list(texts.values()))
    found = []
    enums = 0
    for h in headers:
        enums += len(ENUM_RE.findall(strip_comments(texts[h])))
        found += violations(h, texts[h], macros)
    if found:
        print("check-abi-enum-width: an enum in a C ABI header is not pinned to int width:", file=sys.stderr)
        for v in found:
            print("  " + v, file=sys.stderr)
        print(
            "  Its Rust mirror is host bindgen output (`c_uint`); on arm-none-eabi an\n"
            "  unpinned C enum is 1 byte (issue 2758). Add `<PREFIX>_FORCE_INT_WIDTH_ =\n"
            "  0x7fffffff` as its last enumerator and NROS_RMW_ASSERT_ENUM_INT_WIDTH(<type>)\n"
            "  (or a `sizeof(<type>) == sizeof(int)` static assert) beside it.",
            file=sys.stderr,
        )
        return 1
    print(f"check-abi-enum-width: {enums} enum(s) in {len(headers)} ABI header(s), all int-width pinned")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
