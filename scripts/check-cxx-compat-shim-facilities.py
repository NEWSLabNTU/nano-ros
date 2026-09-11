#!/usr/bin/env python3
"""Each `cxx-compat/` shim supplies the freestanding facilities our code is entitled to.

THE SIBLING GATE ANSWERS A DIFFERENT QUESTION

`check-cxx-compat-shim-coverage.py` asks whether every `std::` C-LIBRARY name a
served source spells is exported — `memchr`, `abort`, `snprintf`. It harvests
uses from sources and exports from the shim, which is right for a set that
grows one name at a time as code is written.

This gate asks the other half: whether the shim supplies what the LANGUAGE
guarantees on a freestanding implementation, whether or not any source spells it
today. Those two sets fail differently. A missing `memchr` is caught by the
first line of code that wants it. A missing placement `operator new` is caught
by the first line of code that wants it too — but the code that wants it is
written months later, against a header that looks complete, and the error
arrives on a cross build nobody runs per-push. `[new.delete.placement]` puts
those forms in the freestanding subset of `<new>`, so a freestanding target is
entitled to them and neither shim had them.

WHAT IT MEASURES

One probe translation unit, compiled against each shim exactly as that target's
build reaches it (`-nostdinc++`, freestanding, the shim first on the include
path). Not a text scan of the shim: a trait can be present and wrong, and
`decay` in particular is easy to write as `remove_cv_t<remove_reference_t<T>>`,
which is correct for scalars and wrong for exactly the function and array types
a callback signature is written with. The probe `static_assert`s the answers.

THE NEGATIVE CONTROLS RUN EVERY TIME

Two, on the normal path (phase-395): the probe is re-compiled against a COPY of
the ThreadX shim with the placement forms removed, and against a copy with
`decay` removed. Both must FAIL. A gate that compiles a probe is otherwise
indistinguishable from a gate whose probe compiles against anything.

WHY BOTH SHIMS, AND WHY THE ZEPHYR ONE CAN SKIP

`zephyr/cxx-compat` and the threadx-qemu-riscv64 board shim are one
implementation under two names — the Zephyr header says so in its own comment,
"Mirrors the threadx-qemu-riscv64 board shim" — so a facility added to one and
not the other is a difference between two boards that nothing measures. The
Zephyr arm layers over Zephyr's own minimal libcpp, which lives in the west
workspace; when that workspace is not checked out the arm is reported as a SKIP
through the `nros_check_skip` ledger rather than passing quietly, because a skip
that reads like a pass is how the gap being fixed here survived.

Usage::

    check-cxx-compat-shim-facilities.py
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

THREADX_SHIM = "packages/boards/nros-board-threadx-qemu-riscv64/cxx-compat"
ZEPHYR_SHIM = "zephyr/cxx-compat"
ZEPHYR_MINIMAL = "zephyr-workspace/zephyr/lib/cpp/minimal/include"
PLATFORM_API = "packages/platform/nros-platform-api/include"

# The probe. Every assertion here is a facility the standard puts in the
# freestanding subset, or a trait phase-442 W3's inplace callable and handle
# need in order to be written without leaning on the shim's shape (RFC-0096 D3
# says the API carries its own; this says the shim should still be honest).
PROBE = r"""
#include <new>
#include <type_traits>
#include <utility>

struct Cell {
    int v;
    explicit Cell(int x) : v(x) {}
};

// [new.delete.placement] -- freestanding-guaranteed.
alignas(Cell) static unsigned char storage[sizeof(Cell)];
static Cell* make() { return new (static_cast<void*>(storage)) Cell(7); }

static_assert(std::is_same<int, int>::value, "is_same");
static_assert(!std::is_same<int, long>::value, "is_same discriminates");
static_assert(std::is_same<std::remove_reference<int&>::type, int>::value, "remove_reference");
static_assert(std::is_same<std::remove_cv<const volatile int>::type, int>::value, "remove_cv");
static_assert(std::is_same<std::conditional<true, int, long>::type, int>::value, "conditional");

// `decay`, at the three shapes that separate it from
// `remove_cv_t<remove_reference_t<T>>`.
static_assert(std::is_same<std::decay<const int&>::type, int>::value, "decay strips cv-ref");
static_assert(std::is_same<std::decay<int[4]>::type, int*>::value, "decay array-to-pointer");
static_assert(std::is_same<std::decay<void(int)>::type, void (*)(int)>::value,
              "decay function-to-pointer");

// The two the shims already had, so a widening cannot quietly drop them.
static_assert(std::is_convertible<int, long>::value, "is_convertible");
static_assert(std::enable_if<true, int>::type(0) == 0, "enable_if");

extern "C" int nros_shim_facilities_probe() {
    int a = 1;
    int b = static_cast<int>(std::move(a)); // <utility> still provides move
    return make()->v + b;
}
"""

# (label, compiler, extra include dirs beyond the shim). The ThreadX board is
# built with `riscv64-unknown-elf-g++` (`cmake/toolchain/riscv64-threadx.cmake`);
# the host `c++` is used where the cross compiler is absent, which still answers
# the question this gate asks, because every facility here is language-level.
THREADX_CC = ["riscv64-unknown-elf-g++", "c++"]
ZEPHYR_CC = ["c++"]

FLAGS = ["-fsyntax-only", "-std=c++14", "-fno-exceptions", "-fno-rtti", "-ffreestanding",
         "-nostdinc++"]


def first_available(candidates):
    for c in candidates:
        if shutil.which(c):
            return c
    return None


def compile_probe(cc, shim, extra_includes, workdir):
    src = os.path.join(workdir, "shim_facilities_probe.cpp")
    with open(src, "w", encoding="utf8") as fh:
        fh.write(PROBE)
    cmd = [cc] + FLAGS + ["-isystem", shim]
    for inc in extra_includes:
        cmd += ["-isystem", inc]
    cmd += ["-I", os.path.join(ROOT, PLATFORM_API), src]
    proc = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
    return proc.returncode, proc.stdout + proc.stderr


def mutate(shim, workdir, name, drop):
    """A copy of `shim` with `drop(text)` applied to one header."""
    dst = os.path.join(workdir, name)
    shutil.copytree(os.path.join(ROOT, shim), dst)
    header, transform = drop
    path = os.path.join(dst, header)
    with open(path, encoding="utf8") as fh:
        text = fh.read()
    new_text = transform(text)
    if new_text == text:
        return None  # the mutation did not apply -- caller reports it
    with open(path, "w", encoding="utf8") as fh:
        fh.write(new_text)
    return dst


def drop_placement_new(text):
    """Delete the scalar placement `operator new`, whatever its formatting.

    Matched by SIGNATURE and cut to the closing brace rather than by an exact
    block of text: `clang-format` reflowed this header once and a literal
    three-line marker stopped matching, which the gate correctly reported as a
    BROKEN selftest rather than a pass. A mutation that silently stops applying
    is a control that proves nothing.
    """
    sig = re.compile(r"^inline void\s*\*\s*operator new\(\s*size_t\s*,[^)]*\)[^{]*\{", re.M)
    m = sig.search(text)
    if m is None:
        return text
    end = text.index("\n}", m.end()) + len("\n}\n")
    return text[:m.start()] + text[end:]


def drop_decay(text):
    start = text.find("template <class T>\nstruct decay {")
    if start < 0:
        return text
    end = text.find("using decay_t", start)
    if end < 0:
        return text
    end = text.find("\n", text.find(";", end)) + 1
    return text[:start] + text[end:]


def selftest(cc, workdir):
    """Both gaps this gate exists for, re-introduced into a COPY of the shim.

    Runs on the NORMAL path, every invocation (phase-395): a gate that compiles
    a probe is otherwise indistinguishable from a gate whose probe compiles
    against anything. Returns the list of failures, empty when both mutants were
    correctly rejected.
    """
    failures = []
    for label, header, transform in (
        ("placement operator new removed", "new", drop_placement_new),
        ("std::decay removed", "type_traits", drop_decay),
    ):
        mutated = mutate(THREADX_SHIM, workdir, "mutant_" + header, (header, transform))
        if mutated is None:
            failures.append("SELFTEST BROKEN: the '%s' mutation changed nothing, so the "
                            "control proves nothing. The shim's text moved; update the "
                            "mutation." % label)
            continue
        rc, _out = compile_probe(cc, mutated, [], workdir)
        if rc == 0:
            failures.append("SELFTEST FAILED: with %s the probe still COMPILED, so this gate "
                            "cannot detect that gap." % label)
    return failures


def main():
    failures = []
    notes = []

    threadx_cc = first_available(THREADX_CC)
    if threadx_cc is None:
        print("check-cxx-compat-shim-facilities: no C++ compiler found", file=sys.stderr)
        return 1

    shim_path = os.path.join(ROOT, THREADX_SHIM)
    if not os.path.isdir(shim_path):
        print("check-cxx-compat-shim-facilities: %s is MISSING -- this gate would pass on "
              "absence" % THREADX_SHIM, file=sys.stderr)
        return 1

    with tempfile.TemporaryDirectory() as workdir:
        # --- the measurement -------------------------------------------------
        rc, out = compile_probe(threadx_cc, shim_path, [], workdir)
        if rc != 0:
            failures.append("ThreadX shim (%s): the freestanding facility probe does NOT "
                            "compile.\n%s" % (threadx_cc, out.strip()))
        else:
            notes.append("ThreadX shim: probe compiles (%s)" % threadx_cc)

        zephyr_min = os.path.join(ROOT, ZEPHYR_MINIMAL)
        if os.path.isdir(zephyr_min):
            rc, out = compile_probe(first_available(ZEPHYR_CC),
                                    os.path.join(ROOT, ZEPHYR_SHIM), [zephyr_min], workdir)
            if rc != 0:
                failures.append("Zephyr shim over the minimal libcpp: the freestanding "
                                "facility probe does NOT compile.\n%s" % out.strip())
            else:
                notes.append("Zephyr shim + minimal libcpp: probe compiles")
        else:
            notes.append("Zephyr arm SKIPPED: %s absent (west workspace not checked out)"
                         % ZEPHYR_MINIMAL)
            skip_ledger = os.environ.get("NROS_CHECK_SKIP_LEDGER")
            if skip_ledger:
                with open(skip_ledger, "a", encoding="utf8") as fh:
                    fh.write("check-cxx-compat-shim-facilities: zephyr arm: %s absent\n"
                             % ZEPHYR_MINIMAL)

        # --- the negative controls, on the normal path -----------------------
        failures.extend(selftest(threadx_cc, workdir))

    for n in notes:
        print("check-cxx-compat-shim-facilities: %s" % n)
    if failures:
        print("", file=sys.stderr)
        for f in failures:
            print("FAIL: %s" % f, file=sys.stderr)
        print("\nA `cxx-compat/` shim IS the C++ library of the target it serves. A facility "
              "the standard guarantees on a freestanding implementation must be there whether "
              "or not any source spells it today -- the code that needs it is written later, "
              "against a header that looks complete.", file=sys.stderr)
        return 1

    print("check-cxx-compat-shim-facilities: OK -- freestanding <new> placement forms and the "
          "trait set both shims must carry, measured by compiling a probe, with 2 negative "
          "control(s) that ran and failed as required")
    return 0


if __name__ == "__main__":
    sys.exit(main())
