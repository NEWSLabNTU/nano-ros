#!/usr/bin/env python3
"""issue 0490 — every static `cargo:rerun-if-changed=<path>` in a build script
must name a path that EXISTS.

# Why this is worth a gate

Cargo treats a missing `rerun-if-changed` input as permanently dirty:

    dirty: FsStatusOutdated(StaleItem(MissingFile { path: "…" }))

so the build script re-runs on every invocation, and every crate above it
recompiles with it. Nothing fails. The build is simply never fresh, forever.

`packages/rmw/cffi/build.rs` sat like that from phase-321 W2.e (`12c365774`,
which moved the crate from `packages/core/nros-rmw-cffi` to `packages/rmw/cffi`
and carried its `../nros-rmw-abi/…` relative path along) until phase-340 P2 read
cargo's own fingerprint log while checking a staleness probe. `nros-rmw-cffi` is
under every nano-ros image, so the effect was that EVERY Rust fixture in the
repo recompiled its whole dependency chain on every staleness probe and every
incremental build — which also means every `check-fixtures-stale` run reported
those fixtures as "STALE and have now been rebuilt", the warning that teaches
readers to ignore the warning.

# Scope

Only STATIC paths in a file named exactly `build.rs`. Interpolated paths
(`{var}`, `$…`) cannot be checked without running the script. Helper modules
under `src/` that a build script calls into (e.g.
`nros-board-common/src/threadx_qemu_riscv64_build.rs`) are excluded: their
relative paths resolve against the CONSUMER crate, not the helper's own
directory, so checking them here would report four false positives and teach
people to add exemptions.

Run: python3 scripts/check-build-rs-rerun-paths.py
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RERUN = re.compile(r"cargo:rerun-if-changed=([^\"\\\n]+)")


def offenders(build_rs_paths):
    """[(relpath-to-build.rs, declared path)] for declared paths that do not exist.

    Split out so the self-test can drive it over a synthetic file.
    """
    out = []
    for rel in build_rs_paths:
        abs_path = os.path.join(ROOT, rel)
        crate_dir = os.path.dirname(abs_path)
        try:
            text = open(abs_path, encoding="utf-8").read()
        except (OSError, UnicodeDecodeError):
            continue
        for m in RERUN.finditer(text):
            declared = m.group(1).strip()
            if not declared or "{" in declared or "$" in declared:
                continue
            if not os.path.exists(os.path.join(crate_dir, declared)):
                out.append((rel, declared))
    return out


def tracked_build_scripts():
    listed = subprocess.run(
        ["git", "ls-files", "*build.rs"], capture_output=True, text=True, cwd=ROOT
    ).stdout.split()
    return [p for p in listed if os.path.basename(p) == "build.rs"]


def self_test():
    """Both directions, on a synthetic build.rs — a checker that stopped
    checking passes silently, which is the very shape this gate exists for."""
    import tempfile

    with tempfile.TemporaryDirectory(dir=os.path.join(ROOT, "tmp")) as d:
        crate = os.path.join(d, "crate")
        os.makedirs(os.path.join(crate, "include"))
        open(os.path.join(crate, "include", "real.h"), "w").close()
        good = os.path.join(crate, "build.rs")
        with open(good, "w") as fh:
            fh.write('fn main(){println!("cargo:rerun-if-changed=include/real.h");}\n')
        rel_good = os.path.relpath(good, ROOT)
        if offenders([rel_good]):
            sys.stderr.write("self-test: an existing path was reported as missing\n")
            sys.exit(2)
        with open(good, "w") as fh:
            fh.write('fn main(){println!("cargo:rerun-if-changed=include/gone.h");}\n')
        if not offenders([rel_good]):
            sys.stderr.write("self-test: a missing path was NOT reported\n")
            sys.exit(2)
        # An interpolated path must be ignored rather than guessed at.
        with open(good, "w") as fh:
            fh.write('fn main(){println!("cargo:rerun-if-changed={dir}/x.h");}\n')
        if offenders([rel_good]):
            sys.stderr.write("self-test: an interpolated path was reported\n")
            sys.exit(2)


def main():
    self_test()
    scripts = tracked_build_scripts()
    bad = offenders(scripts)
    if bad:
        sys.stderr.write(
            "check-build-rs-rerun-paths: `cargo:rerun-if-changed` names a path "
            "that does not exist.\n\n"
        )
        for rel, declared in bad:
            sys.stderr.write(f"  {rel}: {declared}\n")
        sys.stderr.write(
            "\n  Cargo treats a missing rerun-if-changed input as PERMANENTLY\n"
            "  dirty, so the build script and every crate above it recompile on\n"
            "  every invocation — silently, because the build still succeeds.\n"
            "  Fix the path, or drop the line if the input is gone. (issue 0490)\n"
        )
        sys.exit(1)
    print(f"check-build-rs-rerun-paths: OK ({len(scripts)} build script(s))")


if __name__ == "__main__":
    main()
