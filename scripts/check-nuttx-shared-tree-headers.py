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

# Issue 0551 — RUST_PAT keys on the receiver's NAME, and the rule is about its
# VALUE. `nros-zpico-build` wrote
#
#     if let Ok(dir) = env::var("NUTTX_DIR") {
#         build.include(PathBuf::from(dir).join("include"));
#
# which is the violation exactly, spelled with a binding called `dir`. It sat
# unflagged until `make olddefconfig` deleted the shared tree's generated
# `nuttx/config.h` and every NuttX cargo fixture died on `#include
# <nuttx/config.h>` — the sixth site this gate's name-grep has missed.
#
# So TAINT the bindings instead: a name bound from `NUTTX_DIR`, then a
# `.join("include")` on it.
#
# The taint is PROXIMITY-scoped, not file-scoped. `dir` is the single most
# reused binding name in these build scripts — `nros-zpico-build` alone binds it
# from FREERTOS_DIR and ZENOH_PICO_DIR in other branches of the same function —
# so a file-wide taint reports those as NuttX violations. Real bindings are
# consumed within a line or two of `if let Ok(dir) = env::var("NUTTX_DIR") {`,
# so a short window separates them cleanly. Scope analysis would need a Rust
# parser; the window is the honest approximation, and it is stated rather than
# assumed.
RUST_BIND_PAT = re.compile(
    r'(?:let|if\s+let)\s+(?:Ok\(\s*)?(?:mut\s+)?([A-Za-z_]\w*)\s*\)?\s*=\s*[^;]*'
    r'(?:env::)?var(?:_os)?\(\s*"NUTTX_DIR"'
)
JOIN_INCLUDE_PAT = re.compile(r'\.join\(\s*"include"')
TAINT_WINDOW = 6

# Manifest: `config/<platform>/nros-platform.toml` interpolates `{env:VAR}`, so
# the shared tree has a THIRD spelling that is neither Rust nor shell —
# `"{env:NUTTX_DIR}/include"`. That is where issue 0551 actually lived: the
# zenoh-pico library's `-I` list is manifest-driven, and this gate scanned
# neither `.toml` nor `config/`, so the rule had a hole exactly the shape of the
# violation. Use `{nuttx_include}`, which resolves through
# `nuttx_include_root` and cannot be spelled wrongly.
TOML_PAT = re.compile(r'\{env:NUTTX_DIR\}/include')

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
    # The cmake sibling of the same accessor (issue 0551). The cmake C/C++ lane
    # cannot call the Rust one, and the alternative — every cmake caller
    # open-coding the snapshot lookup — is the drift this gate exists to stop.
    # Its fallback is the same documented one.
    "cmake/platform/nano-ros-nuttx.cmake":
        "defines nros_nuttx_include_root; the shared path is its documented fallback",
    # This gate itself. Its matches are its own prose (the paragraph explaining
    # what the pattern means) and the synthetic build.rs / shell fixtures its
    # tripwires feed to `offenders()` — a gate that greps for a string cannot
    # avoid containing that string, so it matched itself and was RED on main
    # from the commit that added it.
    "scripts/check-nuttx-shared-tree-headers.py":
        "the gate's own documentation and tripwire fixtures contain the pattern",
}

# `.toml` and `config/` joined the scan in issue 0551 — the violation that took
# the NuttX lane down was a manifest row, in a tree this gate never opened.
SUFFIXES = (".rs", ".sh", ".py", ".cmake", ".just", ".txt", ".toml")


def tracked_files():
    out = subprocess.run(
        # Issue 0551 — `config/` and the repo-root `CMakeLists.txt` joined this
        # list because the two sites that took the NuttX lane down were in
        # trees the gate never opened: a `config/nuttx/nros-platform.toml` row
        # and root `CMakeLists.txt`'s `${NUTTX_DIR}/include`. The SHELL_PAT
        # would have matched the latter on sight; it was never handed the file.
        # A gate's SCOPE is part of the rule it enforces.
        ["git", "-C", ROOT, "ls-files", "--",
         "packages", "scripts", "cmake", "just", "justfile", "config",
         "CMakeLists.txt"],
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
        # (name, line) bound from NUTTX_DIR in this file — issue 0551.
        binds = []
        for n, line in enumerate(lines, 1):
            if line.lstrip().startswith(("#", "//", "*", "!")):
                continue
            m = RUST_BIND_PAT.search(line)
            if m:
                binds.append((m.group(1), n))
        for n, line in enumerate(lines, 1):
            if line.lstrip().startswith(("#", "//", "*", "!")):
                continue          # prose about the rule is not a violation
            tainted_join = bool(
                JOIN_INCLUDE_PAT.search(line)
                and any(
                    0 <= n - bn <= TAINT_WINDOW
                    and re.search(rf'\b{re.escape(name)}\b', line)
                    for name, bn in binds
                )
            )
            if not (
                RUST_PAT.search(line)
                or SHELL_PAT.search(line)
                or TOML_PAT.search(line)
                or tainted_join
            ):
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
        # Issue 0551 — a receiver NOT named nuttx, tainted by the binding. This
        # is the shape that shipped in nros-zpico-build and went unflagged.
        write('fn main(){ if let Ok(dir) = env::var("NUTTX_DIR") {\n'
              '    build.include(PathBuf::from(dir).join("include")); } }\n')
        if not offenders([rel]):
            sys.stderr.write("self-test: a TAINTED shared-tree include was NOT reported\n")
            sys.exit(2)
        # …and the same shape routed through the accessor is still clean, so the
        # taint rule cannot be satisfied only by renaming the variable.
        write('fn main(){ if let Ok(dir) = env::var("NUTTX_DIR") {\n'
              '    build.include(nros_build_paths::nuttx_include_root(&PathBuf::from(dir))); } }\n')
        if offenders([rel]):
            sys.stderr.write("self-test: the tainted-but-correct form WAS reported\n")
            sys.exit(2)
        # A `.join("include")` on an UNRELATED path in a file that also reads
        # NUTTX_DIR must not be swept up by the taint.
        write('fn main(){ let _ = env::var("NUTTX_DIR");\n'
              '    build.include(zephyr_dir.join("include")); }\n')
        if offenders([rel]):
            sys.stderr.write("self-test: an unrelated join WAS reported\n")
            sys.exit(2)

        # Issue 0551 — the manifest spelling, which is neither Rust nor shell.
        # This is the one that actually shipped.
        toml_probe = os.path.join(d, "nros-platform.toml")
        toml_rel = os.path.relpath(toml_probe, ROOT)
        with open(toml_probe, "w") as fh:
            fh.write('include_paths = [ "{env:NUTTX_DIR}/include" ]\n')
        if not offenders([toml_rel]):
            sys.stderr.write("self-test: the manifest spelling was NOT reported\n")
            sys.exit(2)
        with open(toml_probe, "w") as fh:
            fh.write('include_paths = [ "{nuttx_include}" ]\n')
        if offenders([toml_rel]):
            sys.stderr.write("self-test: the {nuttx_include} token WAS reported\n")
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
