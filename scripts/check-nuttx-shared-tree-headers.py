#!/usr/bin/env python3
"""Issue 0525 — no build input may take NuttX headers from the SHARED tree.

NuttX is built IN PLACE: `configure.sh` writes `.config` and the generated
`include/nuttx/config.h` INTO `third-party/nuttx/nuttx`, and one checkout serves
both in-tree arches. So the tree holds exactly one arch's configuration, and
which one is a property of BUILD ORDER — `lane=tier2` builds nuttx-riscv after
nuttx. Anything that derives a compile input from `$NUTTX_DIR/include` therefore
silently takes the other arch's values.

That is issue 0511, which cost real time out of proportion to its size: the ARM
Rust image was linked with the RISC-V memory map (`MEMORY { ROM ... LENGTH =
CONFIG_FLASH_SIZE }`, and RISC-V has `CONFIG_FLASH_SIZE=0`), so ROM had zero
bytes and every byte placed in it "overflowed". It read as a 400-500 KB size
regression, survived clean rebuilds — the stale `.config` lives in the submodule,
not in any target dir — and cost a bisect that had to be retracted, because no
revision had ever fit.

phase-339 W2 had already moved the kernel LIBS and the linker SCRIPT onto
per-arch export snapshots. The headers were left behind, so the arch selection
covered two of three input classes. The fix routed every reader through
`nros_build_paths::nuttx_include_root`. This gate is what stops the next one
from being written — and it earned itself on the first run, catching a FIFTH
site the hand sweep had missed: `nuttx-sys`'s bindgen script, which is a
standalone crate outside the board helpers and so outside the grep that found
the other four.

THE RULE: a compile input (an `-isystem`/`-I` for a C compile or a cpp pass)
must resolve headers through `include_root`, never by joining `include` onto the
NuttX tree. Existence PROBES are fine — they ask whether the tree is provisioned
at all, and both arches answer the same.
"""

import os
import re
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Rust: `nuttx_dir.join("include")` / `nuttx.join("include")` and friends.
RUST_PAT = re.compile(r'\b\w*(?:nuttx\w*|NUTTX\w*)\s*\.join\(\s*"include"')
# Shell / CMake / just: `$NUTTX_DIR/include`, `${NUTTX_DIR}/include`.
SHELL_PAT = re.compile(r'\$\{?NUTTX_DIR\}?/include')

# A line is a PROBE, not a compile input, when it only asks whether the path
# exists. Both arches answer identically, so a probe cannot pick the wrong one.
PROBE_PAT = re.compile(
    r"""\.exists\(\)|\.is_dir\(\)|                 # Rust
        \[\s*!?\s*-[dfe]\s|                        # sh test
        \bif\s*\(\s*(?:NOT\s+)?EXISTS\b""",        # cmake
    re.X,
)

# Exempt sites, each with the reason it cannot take the wrong arch. An exemption
# is a claim about arch-invariance, not a preference.
ALLOWED = {
    # The accessor itself: this IS the resolution, and its fallback to the live
    # tree is what keeps a pre-phase-339 checkout working. It lives in
    # `nros-build-paths` so the board helpers and `nuttx-sys`'s bindgen script
    # share ONE spelling — `nuttx-sys` is a standalone crate that cannot reach
    # `nros-board-common`, and a second copy is the drift that caused 0511.
    "packages/tooling/nros-build-paths/src/lib.rs":
        "defines nuttx_include_root; the shared path is its documented fallback",
    # This gate itself. Its matches are its own prose (the paragraph explaining
    # what the pattern means) and the synthetic build.rs / shell fixtures its
    # tripwires feed to `offenders()` — a gate that greps for a string cannot
    # avoid containing that string, so it matched itself and was RED on main
    # from the commit that added it.
    "scripts/check-nuttx-shared-tree-headers.py":
        "the gate's own documentation and tripwire fixtures contain the pattern",
}

SUFFIXES = (".rs", ".sh", ".py", ".cmake", ".just", ".txt")


def tracked_files():
    out = subprocess.run(
        ["git", "-C", ROOT, "ls-files", "--", "packages", "scripts", "cmake", "just", "justfile"],
        capture_output=True, text=True, check=True,
    ).stdout.split()
    return [f for f in out if f.endswith(SUFFIXES) or f == "justfile"]


def offenders(files):
    hits = []
    for rel in files:
        if rel in ALLOWED:
            continue
        path = os.path.join(ROOT, rel)
        try:
            with open(path, encoding="utf-8", errors="replace") as fh:
                lines = fh.readlines()
        except OSError:
            continue
        for n, line in enumerate(lines, 1):
            if line.lstrip().startswith(("#", "//", "*", "!")):
                continue          # prose about the rule is not a violation
            if not (RUST_PAT.search(line) or SHELL_PAT.search(line)):
                continue
            if PROBE_PAT.search(line):
                continue          # existence probe: arch-invariant
            hits.append((rel, n, line.strip()[:100]))
    return hits


def self_test():
    """Both directions. A checker that stopped checking passes silently, which
    is the failure shape this gate exists for."""
    tmp_root = os.path.join(ROOT, "tmp")
    os.makedirs(tmp_root, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=tmp_root) as d:
        probe = os.path.join(d, "build.rs")
        rel = os.path.relpath(probe, ROOT)

        def write(body):
            with open(probe, "w") as fh:
                fh.write(body)

        # A compile input off the shared tree IS reported.
        write('fn main(){ cc.include(nuttx_dir.join("include")); }\n')
        if not offenders([rel]):
            sys.stderr.write("self-test: a shared-tree include was NOT reported\n")
            sys.exit(2)
        # …in its shell spelling too.
        write('cc -isystem$NUTTX_DIR/include foo.c\n')
        if not offenders([rel]):
            sys.stderr.write("self-test: the shell spelling was NOT reported\n")
            sys.exit(2)
        # An existence PROBE is not reported.
        write('fn main(){ if nuttx_dir.join("include").exists() { return; } }\n')
        if offenders([rel]):
            sys.stderr.write("self-test: an existence probe WAS reported\n")
            sys.exit(2)
        # The sanctioned accessor is not reported.
        write('fn main(){ cc.include(nros_build_paths::nuttx_include_root(&nuttx_dir)); }\n')
        if offenders([rel]):
            sys.stderr.write("self-test: include_root WAS reported\n")
            sys.exit(2)


def main():
    self_test()
    files = tracked_files()
    if not files:
        sys.stderr.write("[FAIL] no tracked sources scanned — this gate would pass vacuously.\n")
        return 1
    hits = offenders(files)
    if hits:
        sys.stderr.write(
            "[FAIL] these take NuttX headers from the SHARED tree, whose "
            "`nuttx/config.h`\n       belongs to whichever arch was configured "
            "LAST (issue 0525):\n"
        )
        for rel, n, text in hits:
            sys.stderr.write(f"         {rel}:{n}: {text}\n")
        sys.stderr.write(
            "\n       Resolve through `nros_build_paths::nuttx_include_root`,\n"
            "       which prefers THIS arch's export snapshot and falls back to the live\n"
            "       tree. If the site genuinely cannot take the wrong arch, add it to\n"
            "       ALLOWED in this file WITH that reason.\n"
        )
        return 1
    print(f"check-nuttx-shared-tree-headers: OK ({len(files)} tracked source(s))")
    return 0


if __name__ == "__main__":
    sys.exit(main())
