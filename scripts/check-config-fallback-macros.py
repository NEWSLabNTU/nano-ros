#!/usr/bin/env python3
"""A committed FALLBACK config header must carry every macro a generated
artifact reads — issue 1115.

THE PREMISE THAT WAS FALSE
--------------------------
`rosidl-codegen/packs/_codegen_version.jinja` stamps every generated C and C++
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

3. NON-VACUITY. Zero fallbacks, zero packs, or zero required macros is a
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

REPO = Path(__file__).resolve().parent.parent

# The dispatching stubs, and the include-dir roots their fallbacks live in.
STUB_GLOBS = (
    "packages/api/nros-c/include/nros/nros_config_generated.h",
    "packages/api/nros-cpp/include/nros/nros_cpp_config_generated.h",
)
FALLBACK_GLOB = "packages/api/nros-*/include/nros/*_config_generated_*.h"
# The CRATE root, not `packs/`. A second template directory beside it — the
# version-surface gate already scans `{packs,templates}`, and only the first
# exists today — would otherwise be a silent blind spot while `packs/` kept
# yielding the current macros, which is issue 0196's shape.
PACKS_DIR = REPO / "packages/cli/rosidl-codegen"
CODEGEN_VERSION_RS = REPO / "packages/core/nros-core/src/codegen_version.rs"

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


def fallback_value(text: str, name: str) -> str | None:
    m = re.search(rf"^[ \t]*#[ \t]*define[ \t]+{name}[ \t]+(\S+)[ \t]*$", text, re.M)
    return m.group(1) if m else None


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


def tracked(pattern: str) -> list[Path]:
    out = subprocess.run(
        ["git", "-C", str(REPO), "ls-files", "--", pattern],
        capture_output=True,
        text=True,
        check=False,
    )
    return [REPO / line for line in out.stdout.split("\n") if line.strip()]


def self_test() -> None:
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

    assert analyse(good, stubs, {}, rs), "no packs must FAIL, not pass vacuously"
    assert analyse({}, stubs, packs, rs), "no fallbacks must FAIL"


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
    if problems:
        print(
            "check-config-fallback-macros: a committed fallback config header "
            "is not a\nsuperset of what generated code reads (issue 1115):\n",
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
