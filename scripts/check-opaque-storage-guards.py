#!/usr/bin/env python3
"""Issue 0472 — every `*_OPAQUE_U64S` macro must have a compile-time size guard.

A C or C++ caller allocates `uint64_t _opaque[<MACRO>]` and the runtime writes a
Rust value into it. The macro comes from PROBING a compiled rlib
(`nros-build-helpers::{c,cpp}`); the value written is `size_of::<T>()`. Two
derivations of one fact, and when the probe's is smaller the write runs past the
buffer — in C, at a distance from the cause, with no diagnostic.

Before this gate, exactly TWO of the macros carried an assertion
(`EXECUTOR_OPAQUE_U64S`, `CPP_EXECUTOR_OPAQUE_U64S`). The executor's had already
earned its keep: issue 0464 records it catching a committed NuttX constant that
had rotted ~11 % low. The rest could only fail as corruption.

They were unguarded because each was added one at a time, without the guard —
CLAUDE.md's "fix the CLASS, not the reported site". Adding the nine that were
missing fixes today; this gate is what stops the next macro joining them.

THE RULE: every `<NAME>_OPAQUE_U64S` emitted into a generated header must be
named by a guard site, i.e. asserted against the size of the type it stores.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Files that EMIT the macros — templates and the inline header text in the
# build helpers. A macro reaching a header from anywhere else is invisible here,
# which is the issue-0196 shape; keep this list matched to the emitters.
EMITTERS = [
    "packages/api/nros-c/templates/nros_config_generated.h.template",
    "packages/api/nros-c/templates/nros_config_generated_exact.h.template",
    "packages/tooling/nros-build-helpers/src/cpp.rs",
]

# Files that may carry a guard. A guard NAMES its macro in the assertion
# message, which is also what makes the compile error legible.
GUARD_SITES = [
    "packages/api/nros-c/src/opaque_sizes.rs",
    "packages/api/nros-c/src/executor.rs",
    "packages/api/nros-cpp/src/lib.rs",
]

DEFINE = re.compile(r"^\s*#define\s+([A-Z_][A-Z0-9_]*_OPAQUE_U64S)\b", re.M)

# A C++ macro whose value and type are the SAME fact already guarded on the C
# side — `nros-build-helpers::cpp` emits these from the same probe of the same
# `nros::sizes::RAW_*_SIZE`, for the same Rust types. Guarding the fact twice
# would not catch anything the C-side guard misses.
#
# A DIVERGENCE between the two crates' probes (different feature sets producing
# different sizes for one name) is a real hazard, and a different one: it is
# issue 0360's variant-symbol territory, where a header/archive mismatch becomes
# an undefined reference naming what it wanted. Not this gate's job, and noted
# so the exemption is not read as "these don't matter".
COVERED_BY_C_SIDE = {
    "NROS_CPP_RAW_SUBSCRIPTION_OPAQUE_U64S": "SUBSCRIPTION_OPAQUE_U64S",
    "NROS_CPP_RAW_SERVICE_SERVER_OPAQUE_U64S": "SERVICE_SERVER_OPAQUE_U64S",
    "NROS_CPP_RAW_SERVICE_CLIENT_OPAQUE_U64S": "SERVICE_CLIENT_OPAQUE_U64S",
    "NROS_CPP_RAW_ACTION_SERVER_OPAQUE_U64S": "ACTION_SERVER_OPAQUE_U64S",
    "NROS_CPP_RAW_ACTION_CLIENT_OPAQUE_U64S": "ACTION_CLIENT_OPAQUE_U64S",
}


def read(rel):
    try:
        with open(os.path.join(ROOT, rel), encoding="utf-8", errors="replace") as fh:
            return fh.read()
    except OSError:
        return ""


def emitted_macros():
    found = {}
    for rel in EMITTERS:
        for name in DEFINE.findall(read(rel)):
            found.setdefault(name, rel)
    return found


# A guard is a `guard_opaque!(stated, Type, "MACRO")` invocation, or a
# hand-written `assert!` naming the macro (the executor's, and nros-cpp's).
GUARD_MACRO = re.compile(
    r'guard_opaque!\s*\(.*?"([A-Z_][A-Z0-9_]*_OPAQUE_U64S)"', re.S
)
ASSERT_BLOCK = re.compile(r"assert!\s*\((.*?)\);", re.S)


def guarded_macros(text=None):
    """Macro names a guard site actually GUARDS.

    Deliberately not "names that appear in the file". Every macro is also
    DEFINED in `opaque_sizes.rs` as a `pub const`, so a plain grep reports the
    whole set as guarded and the gate passes vacuously — which is what the first
    version of this script did, and its tripwire caught. The distinction is the
    entire value of the check: a guard is a construct that FAILS the build, not
    a string that occurs nearby.
    """
    blob = text if text is not None else "\n".join(read(r) for r in GUARD_SITES)
    names = set(GUARD_MACRO.findall(blob))
    for body in ASSERT_BLOCK.findall(blob):
        names.update(re.findall(r"[A-Z_][A-Z0-9_]*_OPAQUE_U64S", body))
    return names


def unguarded(emitted, guarded):
    missing = []
    for name, rel in sorted(emitted.items()):
        if name in guarded:
            continue
        if name in COVERED_BY_C_SIDE and COVERED_BY_C_SIDE[name] in guarded:
            continue
        missing.append((name, rel))
    return missing


def self_test():
    """Both directions — a gate that stopped checking passes silently, which is
    the exact failure this gate exists to prevent."""
    emitted = {"FAKE_THING_OPAQUE_U64S": "probe"}
    if not unguarded(emitted, set()):
        sys.stderr.write("self-test: an unguarded macro was NOT reported\n")
        sys.exit(2)
    if unguarded(emitted, {"FAKE_THING_OPAQUE_U64S"}):
        sys.stderr.write("self-test: a guarded macro WAS reported\n")
        sys.exit(2)
    # The alias arm: a C++ macro counts as covered only when its C-side
    # counterpart is actually guarded, not merely because it is in the map.
    cpp = {"NROS_CPP_RAW_ACTION_SERVER_OPAQUE_U64S": "probe"}
    if unguarded(cpp, {"ACTION_SERVER_OPAQUE_U64S"}):
        sys.stderr.write("self-test: an alias-covered macro WAS reported\n")
        sys.exit(2)
    if not unguarded(cpp, set()):
        sys.stderr.write("self-test: an alias with NO C-side guard was not reported\n")
        sys.exit(2)


def main():
    self_test()
    emitted = emitted_macros()
    if not emitted:
        sys.stderr.write(
            "[FAIL] no `*_OPAQUE_U64S` macros found in the emitters — either the\n"
            "       generation moved or EMITTERS is stale. Either way this gate\n"
            "       would pass vacuously.\n"
        )
        return 1
    missing = unguarded(emitted, guarded_macros())
    if missing:
        sys.stderr.write(
            "[FAIL] these opaque-storage macros have no compile-time size guard,\n"
            "       so a probe that under-states the size is a SHORT BUFFER at run\n"
            "       time rather than a build error (issue 0472):\n"
        )
        for name, rel in missing:
            sys.stderr.write(f"         {name}   (emitted from {rel})\n")
        sys.stderr.write(
            "\n       Add a `guard_opaque!(<stated>, <Type>, \"<MACRO>\")` in\n"
            "       packages/api/nros-c/src/opaque_sizes.rs, asserting the type fits\n"
            "       the width the header states. The stated value must be the\n"
            "       probe-derived one the header uses, not a re-derivation.\n"
        )
        return 1
    print(
        f"check-opaque-storage-guards: OK "
        f"({len(emitted)} macro(s) emitted, all guarded)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
