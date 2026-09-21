#!/usr/bin/env python3
"""A schema refusal must be able to name the reader that answered.

Issue 1389. Two rules, one class, both buildless.

RULE 1 — DECLARATION. A cmake module that declares a schema constant
`NROS_<x>_SCHEMA_SUPPORTED` must also set `NROS_<x>_SCHEMA_SUPPORTED_FROM` to
`${CMAKE_CURRENT_LIST_FILE}`, in the same file.

RULE 2 — REFUSAL. Any diagnostic that COMPARES a `..._SCHEMA_VERSION` against
its `..._SCHEMA_SUPPORTED` must reach `nros_schema_mismatch_diagnosis()`, which
is where the provenance and the direction of the mismatch are spelled once.

WHY

tier 2 died in cmake configure of `build-cortex-m-c-talker-zenoh`, against the
persistent workspace under `$NROS_STORE/workspaces/zephyr/3.7/`::

    ... states entity-inventory schema version 6; this reader understands 3.
      Rebuild the `nros` CLI so the producer and the reader come from one tree

Producer and reader both read 6 in the checkout that drove that configure, and
no `3` existed anywhere in it. The advice was wrong in a way the message could
not detect: a newer CLI cannot have written an older fragment, so rebuilding it
moves neither number. The `3` came from a SECOND nano-ros checkout — the
persistent workspace's west MANIFEST PROJECT was still the runner's
provisioning tree, so Zephyr's module lane loaded ITS copy of the module too.

MEASURED, because the mechanism reads like it cannot happen:

  * `include_guard(GLOBAL)` keys on the RESOLVED PATH, so two copies of one
    module at two paths are two guards and BOTH bodies run.
  * `set(<x> <v> CACHE INTERNAL ...)` implies FORCE, so the second copy
    overwrites the first — and redefines its `function()`s besides. Whichever
    copy runs LAST is the reader that answers.
  * The tempting alternative is DISPROVEN. The constant is `CACHE INTERNAL`
    and a persistent build dir keeps its `CMakeCache.txt`, so a stale cached
    reader looks like the obvious cause. It is not one: because INTERNAL
    implies FORCE, bumping the module literal and re-configuring the SAME build
    dir answers the new number, cache entry and all.

So the gate is NOT "a module constant may not be `CACHE INTERNAL`". That storage
is this tree's deliberate idiom at ~100 sites (`_NROS_ENTRY_DIR`: these files
are reachable from inside a function frame, where a file-scope plain `set()`
dies with the frame while `include_guard` makes every later include a no-op),
and removing it would reintroduce 287-W6 everywhere to fix a staleness that does
not exist. What was missing is EVIDENCE, and that is what these two rules keep.

REACH

Every tracked cmake file outside `third-party/` — not only the two modules that
declare a constant today. A rule enforced over the file that prompted it is the
2026-07-28 audit's shape (CLAUDE.md, issue 0196); the next schema reader is the
one that will need this.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

HELPER = "nros_schema_mismatch_diagnosis"

# `set(NROS_FOO_SCHEMA_SUPPORTED <n> CACHE ...)` — the declaration. The `_FROM`
# partner is excluded here so it is never mistaken for a declaration of itself.
DECL = re.compile(
    r"^[^#\n]*\bset\s*\(\s*(NROS_[A-Z0-9_]*_SCHEMA_SUPPORTED)\b(?!_FROM)",
    re.MULTILINE,
)

# The comparison that precedes a refusal: a `_SCHEMA_VERSION` tested against a
# `_SCHEMA_SUPPORTED`. Matched across the line break cmake style puts between
# them, and only where BOTH names appear — a file that merely mentions one is
# not making a version decision.
CMP = re.compile(
    r"\bNROS_([A-Z0-9_]*?)_SCHEMA_VERSION\b[^)]*?\bNROS_[A-Z0-9_]*_SCHEMA_SUPPORTED\b",
    re.DOTALL,
)


def strip_comments(text: str) -> str:
    """Drop whole-line cmake comments.

    Only whole-line ones: a trailing `# ...` cannot appear inside the quoted
    message bodies this gate reads, and stripping mid-line would corrupt them.
    """
    return "\n".join("" if ln.lstrip().startswith("#") else ln for ln in text.splitlines())


def decl_offenders(files, read):
    """Rule 1: a declared constant with no `_FROM` partner in the same file."""
    bad = []
    for rel in files:
        text = strip_comments(read(rel))
        for var in sorted(set(DECL.findall(text))):
            partner = re.compile(
                r"\bset\s*\(\s*" + re.escape(var) + r"_FROM\b[^)]*"
                r"\$\{CMAKE_CURRENT_LIST_FILE\}",
                re.DOTALL,
            )
            if not partner.search(text):
                bad.append((rel, var))
    return bad


def refusal_offenders(files, read):
    """Rule 2: a version comparison in a file that never calls the helper."""
    bad = []
    for rel in files:
        text = strip_comments(read(rel))
        families = sorted(set(CMP.findall(text)))
        if not families:
            continue
        # A call, not a mention: the helper name followed by an opening paren.
        if re.search(r"\b" + re.escape(HELPER) + r"\s*\(", text):
            continue
        # The module that DEFINES the helper is not a call site.
        if re.search(r"\bfunction\s*\(\s*" + re.escape(HELPER) + r"\b", text):
            continue
        bad.append((rel, families))
    return bad


def tracked_cmake():
    out = subprocess.run(
        ["git", "ls-files", "*.cmake", "*CMakeLists.txt", "*.cmake.in"],
        cwd=REPO,
        capture_output=True,
        text=True,
        check=True,
    ).stdout.split()
    return [p for p in out if not p.startswith("third-party/")]


def selftest(verbose: bool = False) -> int:
    ok = fail = 0

    def chk(label, cond):
        nonlocal ok, fail
        if cond:
            ok += 1
            if verbose:
                print(f"  ok   {label}")
        else:
            fail += 1
            print(f"  FAIL {label}", file=sys.stderr)

    # ---- Rule 1 ----
    paired = (
        'set(NROS_FOO_SCHEMA_SUPPORTED 6 CACHE INTERNAL "x")\n'
        'set(NROS_FOO_SCHEMA_SUPPORTED_FROM "${CMAKE_CURRENT_LIST_FILE}"\n'
        '    CACHE INTERNAL "y")\n'
    )
    lone = 'set(NROS_FOO_SCHEMA_SUPPORTED 6 CACHE INTERNAL "x")\n'
    dfiles = {
        "paired.cmake": paired,
        "lone.cmake": lone,
        # A partner that records something OTHER than the file it lives in
        # keeps the defect while looking fixed.
        "wrong.cmake": lone + 'set(NROS_FOO_SCHEMA_SUPPORTED_FROM "guess" CACHE INTERNAL "y")\n',
        # Naming the partner in a comment is not setting it.
        "comment.cmake": "# NROS_FOO_SCHEMA_SUPPORTED_FROM ${CMAKE_CURRENT_LIST_FILE}\n" + lone,
        # A file that merely READS a constant declares nothing.
        "reader.cmake": 'message(STATUS "${NROS_FOO_SCHEMA_SUPPORTED}")\n',
        "unrelated.cmake": "add_library(x INTERFACE)\n",
        # The partner alone must not be read as a declaration of ITSELF, which
        # would ask for a `..._FROM_FROM` and never be satisfiable.
        "partneronly.cmake":
            'set(NROS_FOO_SCHEMA_SUPPORTED_FROM "${CMAKE_CURRENT_LIST_FILE}"'
            ' CACHE INTERNAL "y")\n',
    }
    drun = lambda names: decl_offenders(names, dfiles.get)  # noqa: E731

    chk("a declared constant with no _FROM partner FAILS",
        drun(["lone.cmake"]) == [("lone.cmake", "NROS_FOO_SCHEMA_SUPPORTED")])
    chk("the pair passes", drun(["paired.cmake"]) == [])
    chk("a _FROM that is not CMAKE_CURRENT_LIST_FILE does not count",
        drun(["wrong.cmake"]) == [("wrong.cmake", "NROS_FOO_SCHEMA_SUPPORTED")])
    chk("the partner named in a comment does not count",
        drun(["comment.cmake"]) == [("comment.cmake", "NROS_FOO_SCHEMA_SUPPORTED")])
    chk("reading a constant is not declaring one", drun(["reader.cmake"]) == [])
    chk("a file with no schema constant is out of scope",
        drun(["unrelated.cmake"]) == [])
    chk("the _FROM line is not itself a declaration needing a partner",
        drun(["partneronly.cmake"]) == [])

    # ---- Rule 2 ----
    cmp_block = (
        "if(NOT NROS_FOO_SCHEMA_VERSION EQUAL\n"
        "   NROS_FOO_SCHEMA_SUPPORTED)\n"
        '    message(FATAL_ERROR "nros: mismatch")\n'
        "endif()\n"
    )
    rfiles = {
        "blind.cmake": cmp_block,
        "asks.cmake": f"{HELPER}(_d SUPPORTED_VAR NROS_FOO_SCHEMA_SUPPORTED)\n" + cmp_block,
        # The defining module is not a call site.
        "core.cmake": f"function({HELPER} _out)\nendfunction()\n" + cmp_block,
        # A file that names only one of the two is not deciding a version.
        "onlyone.cmake": 'message(STATUS "${NROS_FOO_SCHEMA_SUPPORTED}")\n',
        "unrelated.cmake": "add_library(x INTERFACE)\n",
    }
    rrun = lambda names: refusal_offenders(names, rfiles.get)  # noqa: E731

    chk("comparing the two versions without the helper FAILS",
        rrun(["blind.cmake"]) == [("blind.cmake", ["FOO"])])
    chk("calling the helper passes", rrun(["asks.cmake"]) == [])
    chk("DEFINING the helper is not a call site", rrun(["core.cmake"]) == [])
    chk("naming one constant alone is not a comparison", rrun(["onlyone.cmake"]) == [])
    chk("a file with no comparison is out of scope", rrun(["unrelated.cmake"]) == [])
    # Mutation: the helper NAME without a call must not satisfy rule 2.
    chk("the helper's name in prose does not satisfy the rule",
        refusal_offenders(
            ["m.cmake"],
            {"m.cmake": f'message(STATUS "see {HELPER}")\n' + cmp_block}.get,
        ) == [("m.cmake", ["FOO"])])
    # And the two rules must not substitute for each other.
    chk("declaring the _FROM partner does not satisfy the refusal rule",
        refusal_offenders(["x.cmake"], {"x.cmake": paired + cmp_block}.get)
        == [("x.cmake", ["FOO"])])
    chk("calling the helper does not satisfy the declaration rule",
        decl_offenders(
            ["y.cmake"],
            {"y.cmake": lone + f"{HELPER}(_d)\n"}.get,
        ) == [("y.cmake", "NROS_FOO_SCHEMA_SUPPORTED")])

    chk("the whole tracked set is enumerable", len(tracked_cmake()) > 0)

    if verbose:
        print(f"\n{ok} passed, {fail} failed")
    if fail:
        print("check-schema-reader-provenance self-test: FAILED", file=sys.stderr)
        raise SystemExit(1)
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()
    if args.selftest:
        return selftest(verbose=True)
    # On the NORMAL path, every time — a negative control nobody runs decays
    # into a comment.
    selftest()

    files = tracked_cmake()
    read = lambda rel: (REPO / rel).read_text(errors="replace")  # noqa: E731

    bad_decl = decl_offenders(files, read)
    bad_refusal = refusal_offenders(files, read)

    if bad_decl:
        print("check-schema-reader-provenance: FAILED", file=sys.stderr)
        for rel, var in bad_decl:
            print(
                f"  {rel}: declares {var} but sets no {var}_FROM"
                ' = "${CMAKE_CURRENT_LIST_FILE}"',
                file=sys.stderr,
            )
        print(
            "\nA refusal that quotes two numbers cannot say WHICH READER answered, and\n"
            "two copies of one module at two paths both run — `include_guard(GLOBAL)`\n"
            "keys on the resolved path — with the last one winning. Record the file\n"
            "beside the number:\n"
            '  set(<VAR>_FROM "${CMAKE_CURRENT_LIST_FILE}" CACHE INTERNAL "...")\n'
            "`CACHE INTERNAL` for its partner's reason, not against it: this storage is\n"
            "correct here and re-forced on every configure (issue 1389 measured it).",
            file=sys.stderr,
        )

    if bad_refusal:
        if not bad_decl:
            print("check-schema-reader-provenance: FAILED", file=sys.stderr)
        for rel, families in bad_refusal:
            names = ", ".join(f"NROS_{f}_SCHEMA_VERSION" for f in families)
            print(
                f"  {rel}: compares {names} against its _SCHEMA_SUPPORTED but "
                f"never calls {HELPER}()",
                file=sys.stderr,
            )
        print(
            f"\nBuild the second half of the message with {HELPER}(), which states the\n"
            "reader's own file AND the direction of the mismatch. The direction is a\n"
            "diagnosis on its own: an artifact NEWER than the reader means the reader is\n"
            "behind, so `rebuild the nros CLI` is definitionally wrong advice — it is\n"
            "the advice tier 2 got for three runs while two checkouts sat in one\n"
            "configure (issue 1389).",
            file=sys.stderr,
        )

    if bad_decl or bad_refusal:
        return 1

    print(
        f"check-schema-reader-provenance: OK "
        f"({len(files)} cmake files; every schema constant records its file, "
        f"every version comparison reaches {HELPER}())"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
