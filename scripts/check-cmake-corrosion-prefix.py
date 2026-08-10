#!/usr/bin/env python3
"""issue 0493 — every builder that CONFIGURES a cmake tree must route through
the one `CMAKE_PREFIX_PATH` derivation (`scripts/build/cmake-prefix.sh`), or say
in writing why it does not.

# Why this is worth a gate

The prefix that makes `find_package(Corrosion)` resolve the SDK install was
wired in exactly ONE of three builders:

    scripts/build/compile-check-fixtures.sh    3 refs to ~/.nros/sdk/corrosion
    scripts/build/workspace-fixtures-build.sh  0
    scripts/build/fixtures-build.sh            0

So on ONE host with ONE install, compile-check trees resolved the SDK Corrosion
and workspace/fixture trees fell through to FetchContent. Those are different
CORROSION VERSIONS, and the version decides the cargo target-dir topology:
`< 0.6.0` names the dir with a constant (two workspace roots share one `deps/`
-> duplicate `#[no_mangle]` -> cannot link), `>= 0.6.0` hashes the workspace
manifest path. That is why issue 0493 and phase-340/344 measured contradictory
topologies for days and both were right — and nothing in either build reported
which Corrosion it had used.

This is the repo's recurring defect shape: one caller wires a rule and its
siblings do not (the sizes-header mirror chain, #282's Zephyr guard, #328's
freshness resolver, `fixtures-build.sh`'s `--lang` proxy). The unification is
only half a fix if a FOURTH builder can be added tomorrow without the wiring.

# The rule

A line that runs a cmake CONFIGURE (`cmake … -S …` or `cmake -B …`; a bare
`cmake --build` is not one) must be in a file that either

  * sources the helper, directly or through `cmake-incremental.sh` /
    `fixture-matrix.sh`, which source it and export at file scope; or
  * carries the marker `nros-cmake-prefix-exempt: <reason>` on the configure
    line or within the 6 lines above it.

ONE spelling of the exemption, on purpose: per-caller variants of a rule are how
this class recurs. The exemptions are real — a tree with no Rust in it (the
FreeRTOS/ThreadX C smoke ports, the PX4 posix archive), a third-party build
(CycloneDDS, the XRCE agent), Corrosion's own build, a user-facing copy-out
template that must not depend on repo scripts, and this gate's own synthetic
projects.

Run: python3 scripts/check-cmake-corrosion-prefix.py
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# A cmake CONFIGURE invocation. `cmake --build <dir>` and `cmake --install` are
# not configures and carry no prefix path.
# NOTE the whitespace is only LOOKED at, never consumed: `cmake\s+` followed by
# a `\s-[SB]` lookahead cannot match `cmake -B build`, because the one space is
# already eaten. That miss made the first run of this gate silently pass two
# `cmake -B build` sites.
CONFIGURE = re.compile(r"(?:^|[;&|(]|\s)cmake\b(?![^\n]*--build)(?=[^\n]*\s-[SB]\b)")
MARKER = "nros-cmake-prefix-exempt:"
# Sourcing any of these puts the derivation on this file's environment. It must
# be an actual `source` / `.` LINE, not a mention: these files name each other in
# prose comments constantly, and matching prose would let someone delete the
# `source` line, keep the comment, and get a green — the exact false-negative
# this gate exists to prevent.
WIRED = re.compile(
    # `[^#]*` rather than `\S*`: the real spelling is
    # `. "$(dirname "${BASH_SOURCE[0]}")/cmake-prefix.sh"` — spaces and all —
    # while a trailing `# … cmake-prefix.sh` comment must not count as wiring.
    r"^\s*(?:source|\.)\s+[^#]*(?:cmake-prefix|cmake-incremental|fixture-matrix)\.sh"
)
SEARCH_GLOBS = ("*.sh", "*.just", "justfile", "just/*.just")


def is_comment(line):
    return line.lstrip().startswith("#")


def _block_bounds(lines, i):
    """The recipe body containing line `i`, as [start, end).

    Justfile scope is per RECIPE, not per file: each `#!/usr/bin/env bash`
    recipe runs its own shell, so a `source scripts/build/…` in some other
    recipe does not wire this one. `just/threadx-linux.just` is exactly that —
    one recipe sources `fixture-matrix.sh`, and a `cmake -B build` sits in a
    different recipe 110 lines away. A recipe body is indented; any column-0
    line bounds it.
    """
    start = i
    while start > 0 and (lines[start - 1][:1] in (" ", "\t") or not lines[start - 1].strip()):
        start -= 1
    end = i + 1
    while end < len(lines) and (lines[end][:1] in (" ", "\t") or not lines[end].strip()):
        end += 1
    return start, end


def offenders(files, read=None):
    """[(relpath, lineno, text)] for configure lines with neither wiring nor marker.

    `read` is injectable so the self-test can drive synthetic content.
    """
    reader = read or (lambda rel: open(os.path.join(ROOT, rel), encoding="utf-8").read())
    out = []
    for rel in files:
        try:
            text = reader(rel)
        except (OSError, UnicodeDecodeError):
            continue
        lines = text.splitlines()
        per_recipe = rel.endswith(".just") or os.path.basename(rel) == "justfile"
        file_wired = any(WIRED.search(ln) for ln in lines)
        for i, line in enumerate(lines):
            if is_comment(line) or not CONFIGURE.search(line):
                continue
            if per_recipe:
                start, end = _block_bounds(lines, i)
                wired = any(WIRED.search(ln) for ln in lines[start:end])
            else:
                wired = file_wired
            if wired:
                continue
            context = lines[max(0, i - 6) : i + 1]
            if any(MARKER in c for c in context):
                continue
            out.append((rel, i + 1, line.strip()))
    return out


def tracked_files():
    listed = subprocess.run(
        ["git", "ls-files", *SEARCH_GLOBS], capture_output=True, text=True, cwd=ROOT
    ).stdout.split()
    # Vendored / nested-repo trees are not ours to wire.
    return [
        p
        for p in listed
        if not p.startswith("third-party/")
        and not p.startswith("packages/cli/third-party/")
        and "/third-party/" not in p
    ]


def self_test():
    """Both directions on synthetic content — a checker that stopped checking
    passes silently, which is the very shape this gate exists for."""
    cases = {
        # unwired configure -> MUST be reported
        "bad.sh": "set -e\ncmake -S . -B build\n",
        # same configure, file sources the helper -> must NOT be reported
        "wired.sh": 'source "$r/scripts/build/cmake-prefix.sh"\ncmake -S . -B build\n',
        # same configure, reached through cmake-incremental.sh -> not reported
        "indirect.sh": '. "$d/cmake-incremental.sh"\ncmake -S x -B y\n',
        # explicit exemption in the 6 lines above -> not reported
        "exempt.sh": f"# {MARKER} third-party tree, no Rust\ncmake -S . -B build\n",
        # `cmake --build` is not a configure -> not reported
        "buildonly.sh": "cmake --build build -j4\n",
        # `-B` with no `-S` IS a configure -> reported
        "bonly.sh": "cd x\ncmake -B build >/dev/null\n",
        # a configure inside a COMMENT is documentation -> not reported
        "prose.sh": "# run: cmake -S . -B build\n",
        # a PROSE mention of the helper is not wiring -> still reported
        "mention.sh": "# see scripts/build/cmake-prefix.sh for why\ncmake -S . -B b\n",
    }
    got = {rel for rel, _, _ in offenders(list(cases), read=cases.get)}
    expected = {"bad.sh", "bonly.sh", "mention.sh"}
    if got != expected:
        sys.stderr.write(
            f"self-test: classifier reported {sorted(got)}, expected {sorted(expected)}\n"
        )
        sys.exit(2)
    # The marker must not reach backwards from BELOW the configure line.
    late = {"late.sh": f"cmake -S . -B build\n# {MARKER} too late\n"}
    if not offenders(list(late), read=late.get):
        sys.stderr.write("self-test: a marker placed AFTER the configure was honoured\n")
        sys.exit(2)
    # Justfile wiring is per RECIPE. One recipe sourcing the helper must not
    # vouch for a configure in a different recipe.
    other_recipe = {
        "x.just": (
            "wired-recipe:\n"
            "    source scripts/build/cmake-prefix.sh\n"
            "    cmake -S a -B b\n"
            "\n"
            "other-recipe:\n"
            "    cmake -S c -B d\n"
        )
    }
    hits = offenders(list(other_recipe), read=other_recipe.get)
    if [ln for _, ln, _ in hits] != [6]:
        sys.stderr.write(
            f"self-test: justfile recipe scoping wrong — reported lines {[l for _, l, _ in hits]}, "
            "expected [6]\n"
        )
        sys.exit(2)


def main():
    self_test()
    files = tracked_files()
    bad = offenders(files)
    if bad:
        sys.stderr.write(
            "check-cmake-corrosion-prefix: a cmake CONFIGURE runs without the "
            "shared CMAKE_PREFIX_PATH derivation.\n\n"
        )
        for rel, lineno, text in bad:
            sys.stderr.write(f"  {rel}:{lineno}: {text}\n")
        sys.stderr.write(
            "\n  Source scripts/build/cmake-prefix.sh (or cmake-incremental.sh,\n"
            "  which sources it) and call `nros_cmake_export_prefix_path` at file\n"
            "  scope — or mark the line `# " + MARKER + " <reason>` when the tree\n"
            "  genuinely has no Rust/Corrosion in it.\n\n"
            "  Why: the SDK-Corrosion prefix decides which Corrosion VERSION a\n"
            "  configure resolves, and that decides the cargo target-dir topology\n"
            "  (< 0.6.0 shares one deps/ across workspace roots -> duplicate\n"
            "  #[no_mangle] -> cannot link). One builder having the wiring and its\n"
            "  siblings not is what made one host produce both. (issue 0493)\n"
        )
        sys.exit(1)
    print(f"check-cmake-corrosion-prefix: OK ({len(files)} file(s) scanned)")


if __name__ == "__main__":
    main()
