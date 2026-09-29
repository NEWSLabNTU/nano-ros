#!/usr/bin/env python3
"""A committed FALLBACK config header must carry every macro a generated
artifact reads — issue 1115.

THE PREMISE THAT WAS FALSE
--------------------------
`rosidl-codegen/packs/shared/_codegen_version.jinja` stamps every generated C and C++
header with `NROS_EMITTED_CODEGEN_VERSION` and compares it, with the
preprocessor, against `NROS_CODEGEN_VERSION_MIN..NROS_CODEGEN_VERSION`. Its own
rationale says of those two numbers:

    "Both come from the generated config header, which `nros-build-helpers`
     writes from `nros_core::codegen_version` — so neither side is authored and
     neither can drift from the Rust constant."

On NuttX neither half of that holds. `<nros/nros_config_generated.h>` is a
DISPATCHING STUB: under `NROS_PLATFORM_NUTTX` it includes the committed
snapshot `nros_config_generated_nuttx.h`, and the per-build header the sentence
describes is on no include path at all (measured on a NuttX leaf: the string
`nros-c-generated` appears nowhere in its `build.ninja`, and the preprocessor's
own `-M` output names the in-tree snapshot). So on that platform the config
header IS authored, and it CAN drift.

It did. `828a06616` added the two macros to the template and the consuming
`#error` to every generated header, correctly paired — and the two committed
NuttX snapshots, which stand in for that template's output, gained neither. From
a CLEAN CLONE every NuttX C and C++ image then failed to compile on the first
arm, "the generated config header did not define NROS_CODEGEN_VERSION". Nothing
merge-gating builds NuttX, so it was invisible to CI rather than unreachable by
it — the issue's first diagnosis called it host-local build residue, which a
`rm -rf` would have "fixed" by rebuilding nothing that was wrong.

WHAT THIS GATE CHECKS
---------------------
1. NAME COVERAGE. Every macro that a codegen pack READS (a `#if`/`#ifdef`/
   `#elif` in an emitted artifact) and does not itself define must be defined by
   every committed fallback config header. This is the class: it is not about
   the two macros of issue 1115 but about the next pair someone adds to a
   template and the artifacts without adding it here.

2. EXACT VALUES for the codegen-version pair. A snapshot of a SIZE may be a safe
   upper bound — that is what those files are for, and their own `_Static_assert`
   blocks keep the bounds self-consistent. A version range has no such slack: a
   fallback that is merely "high enough" would accept a tree the runtime rejects.
   So `NROS_CODEGEN_VERSION` / `NROS_CODEGEN_VERSION_MIN` must equal the
   constants in `nros-core/src/codegen_version.rs`.

3. ONE DEFINITION PER MACRO. A fallback that defines a macro twice compiles
   against the LAST definition, while every reader — this gate's value check
   included, until it counted — finds the FIRST. The NuttX C snapshot carried
   the version pair twice (two same-day fixes for one break each added it, in
   different hunks), so a bump that edited the first copy alone passed this
   gate and every NuttX image compiled against the second. A macro defined
   more than once OUTSIDE a conditional arm is refused, whatever its value; a
   definition inside `#if`/`#elif`/`#else` arms (the file's
   `NROS__NUTTX_FALLBACK_ASSERT` selector) is one definition per arm and is
   not counted. The include guard's own `#ifndef` is not an arm. And the
   value check reads the LAST unconditional definition, the one that compiles.

5. NO BUILD REACHES A FALLBACK (issue 1569). The premise above stopped being
   true for NuttX: its FFI build now compiles against the per-build header from
   the `nros-c`/`nros-cpp` `links` channels, and the committed snapshots were
   renamed `*_buildless.h`. They sized every NuttX image from a hand-kept
   number that fell below the build four times (#167, #464, #954, 1568); a
   snapshot cannot even be made exact, since one file stood for two
   architectures. So a stub may dispatch to a fallback ONLY under
   `NROS_CONFIG_BUILDLESS` -- never under a platform macro -- and that define is
   for the header-only checks under `scripts/` that compile with no build at
   all. A non-comment line naming it anywhere else (a cmake file, a build
   script, a Kconfig fragment, a codegen template) is refused: that would be a
   build sizing an image from the snapshot again, the exact regression. Rules
   1-3 still hold for the snapshots, because the buildless checks include
   generated-code-shaped TUs and need every macro to exist; their VALUES bind
   nothing that runs, so no bound over a build is asserted.

4. NON-VACUITY. Zero fallbacks, zero packs, or zero required macros is a
   FAILURE, not an OK — a scan that found nothing prints the same word as a scan
   that found nothing wrong (`check-reconfigure-stale`'s lesson). A fallback is
   also only counted when some stub actually dispatches to it, so a file nobody
   can reach cannot pad the count.

Buildless and offline; reads tracked files only.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
import per_item  # noqa: E402  phase-472 W6 — every definition, not the first

REPO = Path(__file__).resolve().parent.parent

# The dispatching stubs, and the include-dir roots their fallbacks live in.
STUB_GLOBS = (
    "packages/api/nros-c/include/nros/nros_config_generated.h",
    "packages/api/nros-cpp/include/nros/nros_cpp_config_generated.h",
)
STUB_NAMES = tuple(p.rsplit("/", 1)[-1] for p in STUB_GLOBS)
FALLBACK_GLOB = "packages/api/nros-*/include/nros/*_config_generated_*.h"
# The CRATE root, not `packs/`. A second template directory beside it — the
# version-surface gate already scans `{packs,templates}`, and only the first
# exists today — would otherwise be a silent blind spot while `packs/` kept
# yielding the current macros, which is issue 0196's shape.
PACKS_DIR = REPO / "packages/cli/rosidl-codegen"
CODEGEN_VERSION_RS = REPO / "packages/core/nros-core/src/codegen_version.rs"

# issue 1569 -- the ONE define that may select a committed fallback, and the
# only places allowed to state it outside a comment: the buildless checks
# (`scripts/`), the stubs that test it, the snapshots, and prose.
BUILDLESS = "NROS_CONFIG_BUILDLESS"
BUILDLESS_ALLOWED_PREFIXES = ("scripts/", "docs/", "book/")
BUILDLESS_ALLOWED_SUFFIXES = (".md",)

# The pair that must match EXACTLY rather than merely be present.
EXACT = ("NROS_CODEGEN_VERSION", "NROS_CODEGEN_VERSION_MIN")

_DEFINE = re.compile(r"^[ \t]*#[ \t]*define[ \t]+([A-Za-z_][A-Za-z0-9_]*)", re.M)
_COND = re.compile(r"^[ \t]*#[ \t]*(?:if|ifdef|ifndef|elif)\b(.*)$", re.M)
_IDENT = re.compile(r"\b(NROS_[A-Z0-9_]+)\b")
# A jinja comment block `{#- … -#}`: prose about the mechanism, not emitted text.
_JINJA_COMMENT = re.compile(r"\{#.*?#\}", re.S)


def defines_in(text: str) -> set[str]:
    return set(_DEFINE.findall(text))


def macros_read_by(text: str) -> set[str]:
    """`NROS_*` names a preprocessor CONDITIONAL in this text depends on."""
    out: set[str] = set()
    for cond in _COND.findall(text):
        out.update(_IDENT.findall(cond))
    return out


def rust_codegen_versions(text: str) -> dict[str, int]:
    out: dict[str, int] = {}
    for name in EXACT:
        m = re.search(rf"pub const {name}:\s*u32\s*=\s*(\d+)\s*;", text)
        if m:
            out[name] = int(m.group(1))
    return out


_DIRECTIVE = re.compile(r"^[ \t]*#[ \t]*([a-z]+)\b[ \t]*(.*)$")


def unconditional_defines(text: str) -> list[tuple[int, str, str | None]]:
    """(line, name, value-or-None) for every `#define` not inside a conditional
    arm — `per_item.c_defines` (phase-472 W6), which treats the outermost
    `#ifndef` as the include guard. Function-like macros report value None."""
    return [(d.line, d.name, d.value) for d in per_item.c_defines(text) if not d.conditional]


def fallback_value(text: str, name: str) -> str | None:
    """The value the COMPILER sees: the last unconditional definition."""
    vals = [v for _, n, v in unconditional_defines(text) if n == name]
    return vals[-1] if vals else None


def analyse(
    fallbacks: dict[str, str],
    stubs: dict[str, str],
    pack_texts: dict[str, str],
    codegen_rs: str,
) -> list[str]:
    """The whole rule, over CONTENT — so the self-test drives the same code."""
    problems: list[str] = []

    # --- required macro names, from the packs -------------------------------
    required: set[str] = set()
    for text in pack_texts.values():
        body = _JINJA_COMMENT.sub("", text)
        required |= macros_read_by(body) - defines_in(body)
    if not required:
        problems.append(
            "  the codegen packs read NO config-header macro. Either the packs "
            "moved or\n      the scan is broken — this gate cannot pass "
            "vacuously."
        )

    # --- reachable fallbacks -------------------------------------------------
    reachable: dict[str, str] = {}
    for path, text in fallbacks.items():
        name = path.rsplit("/", 1)[-1]
        if any(name in stub for stub in stubs.values()):
            reachable[path] = text
        else:
            problems.append(
                f"  {path} is a fallback config header that NO stub includes.\n"
                f"      Either wire it up or delete it — an unreachable "
                f"fallback silently\n      stops covering the platform it names."
            )
    if not reachable:
        problems.append(
            "  found NO reachable fallback config header. The glob or the "
            "layout moved;\n      a gate that examines nothing must not report "
            "OK."
        )

    # --- (1) name coverage ---------------------------------------------------
    for path, text in sorted(reachable.items()):
        missing = sorted(required - defines_in(text))
        if missing:
            problems.append(
                "  {} does not define {} macro(s) that generated code READS:\n{}"
                "      Generated artifacts reach this file on their platform, "
                "so the\n      `#error` they carry fires for every image. Add "
                "them here in the SAME\n      commit as the template.".format(
                    path,
                    len(missing),
                    "".join(f"        {m}\n" for m in missing),
                )
            )

    # --- (3) one definition per macro ----------------------------------------
    for path, text in sorted(reachable.items()):
        dups = per_item.duplicate_defines(per_item.c_defines(text))
        for name, defs in sorted(dups.items()):
            if len(defs) > 1:
                where = ", ".join(f"line {d.line} = {d.value}" for d in defs)
                problems.append(
                    f"  {path} defines {name} {len(defs)} times ({where}).\n"
                    f"      The compiler keeps the LAST; a reviewer and a gate find "
                    f"the FIRST,\n      so a bump that edits one copy compiles "
                    f"against the other. Define it once."
                )

    # --- (2) exact values for the version pair -------------------------------
    expected = rust_codegen_versions(codegen_rs)
    for name in EXACT:
        if name not in expected:
            problems.append(
                f"  could not read {name} from "
                f"{CODEGEN_VERSION_RS.relative_to(REPO)}.\n"
                f"      Without it the value half of this gate is inert."
            )
    for path, text in sorted(reachable.items()):
        for name, want in expected.items():
            got = fallback_value(text, name)
            if got is None:
                continue  # already reported by (1) when the packs require it
            if got != str(want):
                problems.append(
                    f"  {path} defines {name} = {got}, but "
                    f"nros_core::codegen_version says {want}.\n"
                    f"      This pair is an EXACT mirror, not an upper bound: a "
                    f"snapshot that is\n      merely high enough accepts a "
                    f"generated tree the runtime rejects."
                )
    return problems


_FALLBACK_INCLUDE = re.compile(r'^[ \t]*#[ \t]*include[ \t]*[<"]([^>"]+)[>"]')
# A comment line in any of the languages a build input is written in. A `#`
# that starts a preprocessor DIRECTIVE is code, not a cmake/shell comment.
_COMMENT_LEAD = re.compile(
    r"^[ \t]*(#(?![ \t]*(define|undef|if|elif|ifdef|ifndef|include)\b)|//|/\*|\*|--|;)"
)


def stub_dispatch_problems(stubs: dict[str, str], fallback_names: set[str]) -> list[str]:
    """issue 1569 -- each `#include` of a fallback must sit directly under an
    `#if defined(NROS_CONFIG_BUILDLESS)`, and a stub must name no platform
    macro in a conditional (a platform arm is how NuttX was sized from a
    snapshot)."""
    problems: list[str] = []
    for path, text in sorted(stubs.items()):
        last_cond = ""
        for lineno, line in enumerate(text.split("\n"), 1):
            m = _DIRECTIVE.match(line)
            if m and m.group(1) in ("if", "ifdef", "ifndef", "elif"):
                last_cond = m.group(2)
                if re.search(r"\bNROS_PLATFORM_[A-Z0-9_]+", m.group(2)):
                    problems.append(
                        f"  {path}:{lineno} conditions on a platform macro "
                        f"({m.group(2).strip()}).\n      A stub that picks a "
                        f"committed header per PLATFORM sizes that platform's "
                        f"images\n      from a hand-kept number -- issue 1569. "
                        f"Supply the per-build header instead."
                    )
            inc = _FALLBACK_INCLUDE.match(line)
            if inc and inc.group(1).rsplit("/", 1)[-1] in fallback_names:
                if BUILDLESS not in last_cond:
                    problems.append(
                        f"  {path}:{lineno} includes the fallback "
                        f"{inc.group(1)} under `{last_cond.strip() or '(no condition)'}`."
                        f"\n      Only `defined({BUILDLESS})` may select it (issue 1569)."
                    )
    return problems


def buildless_define_problems(texts: dict[str, str]) -> list[str]:
    """issue 1569 -- `NROS_CONFIG_BUILDLESS` stated outside a comment in a file
    that is not a buildless check, a stub, a snapshot or prose."""
    problems: list[str] = []
    for path, text in sorted(texts.items()):
        if path.startswith(BUILDLESS_ALLOWED_PREFIXES) or path.endswith(
            BUILDLESS_ALLOWED_SUFFIXES
        ):
            continue
        name = path.rsplit("/", 1)[-1]
        if name in STUB_NAMES or "_config_generated_" in name:
            continue
        for lineno, line in enumerate(text.split("\n"), 1):
            if BUILDLESS in line and not _COMMENT_LEAD.match(line):
                problems.append(
                    f"  {path}:{lineno} states {BUILDLESS}:\n        "
                    f"{line.strip()[:120]}\n      That define selects the "
                    f"committed buildless snapshot; a BUILD that sets it sizes\n"
                    f"      its image from a hand-kept number (issue 1569). "
                    f"Only the header-only\n      checks under scripts/ may."
                )
    return problems


def tracked(pattern: str) -> list[Path]:
    out = subprocess.run(
        ["git", "-C", str(REPO), "ls-files", "--", pattern],
        capture_output=True,
        text=True,
        check=False,
    )
    return [REPO / line for line in out.stdout.split("\n") if line.strip()]


def self_test() -> None:
    per_item.self_test()  # the shared helper's own controls (phase-472 W6)
    """Both directions, on synthetic input — `check-gate-selftests` requires
    this on the NORMAL path, because a control nobody runs is a comment."""
    stubs = {"stub.h": '#if defined(NROS_PLATFORM_X)\n#include "cfg_x.h"\n#endif\n'}
    packs = {
        "p.jinja": (
            "{#- prose naming NROS_NEVER_READ -#}\n"
            "#define NROS_EMITTED_CODEGEN_VERSION 3\n"
            "#ifndef NROS_CODEGEN_VERSION\n#error missing\n"
            "#elif NROS_EMITTED_CODEGEN_VERSION < NROS_CODEGEN_VERSION_MIN\n"
            "#error range\n#endif\n"
        )
    }
    rs = (
        "pub const NROS_CODEGEN_VERSION: u32 = 3;\n"
        "pub const NROS_CODEGEN_VERSION_MIN: u32 = 2;\n"
    )
    good = {
        "cfg_x.h": "#define NROS_CODEGEN_VERSION 3\n#define NROS_CODEGEN_VERSION_MIN 2\n"
    }
    assert analyse(good, stubs, packs, rs) == [], "the compliant shape must pass"

    # A pack the gate must not read prose from: `NROS_NEVER_READ` sits in a
    # jinja comment, and `NROS_EMITTED_CODEGEN_VERSION` is defined by the pack.
    assert "NROS_NEVER_READ" not in "".join(analyse(good, stubs, packs, rs))

    missing = {"cfg_x.h": "#define NROS_CODEGEN_VERSION 3\n"}
    assert analyse(missing, stubs, packs, rs), "a missing macro must FAIL"

    wrong = {
        "cfg_x.h": "#define NROS_CODEGEN_VERSION 2\n#define NROS_CODEGEN_VERSION_MIN 2\n"
    }
    out = "".join(analyse(wrong, stubs, packs, rs))
    assert "EXACT mirror" in out, "a stale version value must FAIL"

    orphan = dict(good)
    orphan["cfg_y.h"] = good["cfg_x.h"]
    assert "NO stub includes" in "".join(analyse(orphan, stubs, packs, rs))

    # A duplicate FAILS even when the values agree, and when they do not, the
    # value check judges the LAST copy — the one the compiler uses.
    dup = {"cfg_x.h": good["cfg_x.h"] + "#define NROS_CODEGEN_VERSION 3\n"}
    assert "2 times" in "".join(analyse(dup, stubs, packs, rs)), "a duplicate must FAIL"
    half = {
        "cfg_x.h": "#define NROS_CODEGEN_VERSION 3\n#define NROS_CODEGEN_VERSION_MIN 2\n"
        "#define NROS_CODEGEN_VERSION 2\n"
    }
    assert "= 2, but" in "".join(analyse(half, stubs, packs, rs)), "the LAST copy is judged"
    # Arms of one conditional are one definition each; the include guard is not an arm.
    arms = {
        "cfg_x.h": "#ifndef CFG_X_H\n#define CFG_X_H\n" + good["cfg_x.h"]
        + "#if A\n#define SEL(c) 1\n#elif B\n#define SEL(c) 2\n#else\n#define SEL(c)\n#endif\n"
        "#endif\n"
    }
    assert analyse(arms, stubs, packs, rs) == [], analyse(arms, stubs, packs, rs)
    guarded_dup = {"cfg_x.h": "#ifndef CFG_X_H\n#define CFG_X_H\n" + dup["cfg_x.h"] + "#endif\n"}
    assert "2 times" in "".join(analyse(guarded_dup, stubs, packs, rs)), "guard is not an arm"

    assert analyse(good, stubs, {}, rs), "no packs must FAIL, not pass vacuously"
    assert analyse({}, stubs, packs, rs), "no fallbacks must FAIL"

    # issue 1569 -- the dispatch rule, both directions.
    ok_stub = {
        "s.h": '#if defined(NROS_CONFIG_BUILDLESS)\n#include "nros/cfg_x.h"\n'
        "#else\n#error x\n#endif\n"
    }
    assert stub_dispatch_problems(ok_stub, {"cfg_x.h"}) == []
    nuttx_stub = {"s.h": '#if defined(NROS_PLATFORM_NUTTX)\n#include "nros/cfg_x.h"\n#endif\n'}
    out = "".join(stub_dispatch_problems(nuttx_stub, {"cfg_x.h"}))
    assert "platform macro" in out and "Only" in out, out
    # ... and the define rule: a comment is prose, a cmake or build.rs line is a build.
    assert buildless_define_problems({"cmake/x.cmake": "# NROS_CONFIG_BUILDLESS: checks\n"}) == []
    assert buildless_define_problems({"scripts/c.py": '"-DNROS_CONFIG_BUILDLESS",\n'}) == []
    bad = buildless_define_problems(
        {"cmake/x.cmake": "target_compile_definitions(t PRIVATE NROS_CONFIG_BUILDLESS)\n"}
    )
    assert bad and "cmake/x.cmake:1" in bad[0], bad
    assert buildless_define_problems({"a/build.rs": '    b.define("NROS_CONFIG_BUILDLESS", None);\n'})
    assert buildless_define_problems({"a/x.h": "#define NROS_CONFIG_BUILDLESS 1\n"})


def main() -> int:
    self_test()

    stubs = {}
    for rel in STUB_GLOBS:
        p = REPO / rel
        if p.is_file():
            stubs[rel] = p.read_text(encoding="utf-8")
    if not stubs:
        print(
            "check-config-fallback-macros: found NO dispatching stub header — "
            "the layout moved.",
            file=sys.stderr,
        )
        return 1

    fallbacks = {
        str(p.relative_to(REPO)): p.read_text(encoding="utf-8")
        for p in tracked(FALLBACK_GLOB)
        if p.is_file()
    }
    pack_texts = {
        str(p.relative_to(REPO)): p.read_text(encoding="utf-8")
        # `tracked()`, not `rglob`: the packs are committed, so the index
        # answers this and a walk only stats its way to the same list.
        # `check-no-tracked-file-find` refuses the walk for the reason it
        # states — 7m36s to 0.8s for the same 232 paths — and the helper is
        # already used two lines up for the fallback headers.
        for p in sorted(tracked(f"{PACKS_DIR.relative_to(REPO)}/**/*.jinja"))
    }
    codegen_rs = (
        CODEGEN_VERSION_RS.read_text(encoding="utf-8")
        if CODEGEN_VERSION_RS.is_file()
        else ""
    )

    problems = analyse(fallbacks, stubs, pack_texts, codegen_rs)
    problems += stub_dispatch_problems(stubs, {str(p).rsplit("/", 1)[-1] for p in fallbacks})
    # issue 1569 -- every tracked text file that names the define. `grep -l`
    # over the index is one process, not a walk.
    named = subprocess.run(
        ["git", "-C", str(REPO), "grep", "-l", "-I", "--", BUILDLESS],
        capture_output=True,
        text=True,
        check=False,
    ).stdout.split()
    if not any(n.startswith("scripts/") for n in named):
        problems.append(
            f"  no buildless check under scripts/ names {BUILDLESS}. Those checks "
            f"must define it\n      to reach the snapshot; a scan that found none "
            f"examined nothing."
        )
    problems += buildless_define_problems(
        {rel: (REPO / rel).read_text(encoding="utf-8", errors="replace") for rel in named}
    )
    if problems:
        print(
            "check-config-fallback-macros: a committed fallback config header "
            "breaks its\ncontract -- a superset of what generated code reads (issue 1115), reachable\n"
            "by no build (issue 1569):\n",
            file=sys.stderr,
        )
        print("\n".join(problems), file=sys.stderr)
        return 1

    print(
        f"check-config-fallback-macros: OK — {len(fallbacks)} fallback "
        f"header(s) against {len(pack_texts)} codegen pack(s)."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
