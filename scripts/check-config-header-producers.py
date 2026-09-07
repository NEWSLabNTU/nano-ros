#!/usr/bin/env python3
"""Every producer of `nros_config_generated.h` states the codegen version range.

phase-413 W2. RFC-0090 makes every generated C/C++ header open with

    #include <nros/nros_config_generated.h>
    #ifndef NROS_CODEGEN_VERSION
    #error "nros: the generated config header did not define NROS_CODEGEN_VERSION..."

so the guard is fail-closed by design: a config header that does not define the
macros stops the build. That is right, and it means every producer of that
header has to define them or the guard fires on artifacts nobody changed.

Phase-429 W1 taught THREE producers and missed a fourth. The one it missed is
`packages/api/nros-c/include/nros/nros_config_generated_nuttx.h`, a
hand-maintained twin reached only on NuttX (`nros_config_generated.h`'s single
non-`#error` arm), so the include RESOLVED, the macros were absent, and the
guard fired on every generated message header. The nightly `nuttx` cell went
red on 2026-09-05 and stayed red.

That file has a documented history of exactly this drift — its own comments
record #167, #464 and 0954, each "a per-build size moved and this
hand-maintained twin did not". This was the fourth, and the first that is a
hard compile error rather than a silent under-size.

THE RULE, WIDER THAN THE SITE THAT BROKE (issue-0196)

Fixing only the NuttX header would leave producer number five to repeat it. A
producer is identified by what makes it one: it defines
`NROS_EXECUTOR_STORAGE_SIZE`, the macro the executor's opaque storage is sized
from. Every such file must also define `NROS_CODEGEN_VERSION` and
`NROS_CODEGEN_VERSION_MIN`.

AND THE LITERAL ONES MUST AGREE WITH THE RUNTIME

Three producers substitute the values from `crate::codegen_version`, so they
cannot drift. The NuttX fallback writes LITERALS, because it is not generated
by anything. Those are checked against
`packages/core/nros-core/src/codegen_version.rs` for EXACT equality — exact,
not an upper bound like every size macro in that file, because the range
belongs to the runtime and a fallback that widened it would accept a tree the
runtime rejects.

Run:  python3 scripts/check-config-header-producers.py [--self-test]
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

MARKER = "NROS_EXECUTOR_STORAGE_SIZE"
RUST_CONSTS = os.path.join("packages", "core", "nros-core", "src", "codegen_version.rs")


def runtime_range(text):
    """(NROS_CODEGEN_VERSION, NROS_CODEGEN_VERSION_MIN) from the Rust SSoT."""
    out = {}
    for name in ("NROS_CODEGEN_VERSION_MIN", "NROS_CODEGEN_VERSION"):
        m = re.search(rf"^pub const {name}\s*:\s*u32\s*=\s*(\d+)\s*;", text, re.M)
        if m:
            out[name] = int(m.group(1))
    return out


def defined_macro(text, name):
    """The token a `#define <name> <token>` gives, or None.

    Returns the RAW token so a template placeholder (`@CODEGEN_VERSION@`) and a
    Rust format hole (`{codegen_version}`) are both visible as "defined but not
    a literal" — those producers substitute from the SSoT and cannot drift.
    """
    m = re.search(rf"^#define\s+{name}\s+(\S+)", text, re.M)
    return m.group(1) if m else None


def producers(root):
    """Files that define the storage-size macro: the config-header producers."""
    out = subprocess.run(
        ["git", "grep", "-l", MARKER, "--", "*.h", "*.template", "*.rs", "*.jinja"],
        cwd=root, capture_output=True, text=True,
    ).stdout.split()
    # `codegen_version.rs` and the gates themselves would match on a mention.
    return [f for f in out if not f.startswith("scripts/")]


def self_test():
    t = "#define NROS_CODEGEN_VERSION 3\n#define NROS_CODEGEN_VERSION_MIN 2\n"
    assert defined_macro(t, "NROS_CODEGEN_VERSION") == "3"
    assert defined_macro(t, "NROS_CODEGEN_VERSION_MIN") == "2"
    assert defined_macro(t, "NROS_NOPE") is None
    # A placeholder counts as DEFINED and is not compared as a literal.
    assert defined_macro("#define NROS_CODEGEN_VERSION @CODEGEN_VERSION@\n",
                         "NROS_CODEGEN_VERSION") == "@CODEGEN_VERSION@"
    assert defined_macro("#define NROS_CODEGEN_VERSION {codegen_version}\n",
                         "NROS_CODEGEN_VERSION") == "{codegen_version}"
    # The Rust SSoT parser reads both constants and is not fooled by the longer
    # name matching the shorter pattern.
    r = runtime_range("pub const NROS_CODEGEN_VERSION: u32 = 7;\n"
                      "pub const NROS_CODEGEN_VERSION_MIN: u32 = 4;\n")
    assert r == {"NROS_CODEGEN_VERSION": 7, "NROS_CODEGEN_VERSION_MIN": 4}, r
    sys.stdout.write("check-config-header-producers self-test: OK\n")


def main():
    if "--self-test" in sys.argv:
        self_test()
        return 0
    self_test()

    try:
        with open(os.path.join(ROOT, RUST_CONSTS), encoding="utf8") as fh:
            rng = runtime_range(fh.read())
    except OSError:
        rng = {}
    if len(rng) != 2:
        sys.stderr.write(
            f"error: could not read both constants from {RUST_CONSTS}.\n"
            "This gate would then compare against nothing and pass vacuously.\n"
        )
        return 1

    files = producers(ROOT)
    if not files:
        sys.stderr.write(
            f"error: no file defines {MARKER}. Either the marker moved or the\n"
            "walk is broken; a gate over an empty set is not a pass.\n"
        )
        return 1

    problems = []
    literal_checked = 0
    for rel in files:
        try:
            with open(os.path.join(ROOT, rel), encoding="utf8") as fh:
                text = fh.read()
        except OSError:
            continue
        for name in ("NROS_CODEGEN_VERSION", "NROS_CODEGEN_VERSION_MIN"):
            tok = defined_macro(text, name)
            if tok is None:
                problems.append(
                    f"  {rel}: defines {MARKER} but not {name}.\n"
                    f"      Every generated header `#include`s the config header and\n"
                    f"      `#error`s when this macro is absent (RFC-0090), so this\n"
                    f"      producer breaks the build of every message it reaches."
                )
                continue
            if re.fullmatch(r"\d+", tok):
                literal_checked += 1
                if int(tok) != rng[name]:
                    problems.append(
                        f"  {rel}: {name} is {tok}, the runtime says {rng[name]}.\n"
                        f"      EXACT, not an upper bound like the size macros: the range\n"
                        f"      belongs to the runtime, and widening it here accepts a\n"
                        f"      tree the runtime rejects."
                    )

    if problems:
        sys.stderr.write(
            "check-config-header-producers: %d problem(s)\n\n" % len(problems)
        )
        for p in problems:
            sys.stderr.write(p + "\n\n")
        sys.stderr.write(
            "  A producer is any file defining %s. Phase-429 W1 taught three of\n"
            "  them and missed the NuttX fallback, and the nightly `nuttx` cell was\n"
            "  red for two days on the resulting #error.\n" % MARKER
        )
        return 1

    print(
        f"check-config-header-producers: OK — {len(files)} producer(s) of "
        f"nros_config_generated.h, each states the codegen version range; "
        f"{literal_checked} literal value(s) match the runtime "
        f"({rng['NROS_CODEGEN_VERSION_MIN']}..={rng['NROS_CODEGEN_VERSION']})."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
