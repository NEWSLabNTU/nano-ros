#!/usr/bin/env python3
"""issue 0834 — a per-build generated header must never lose its `.stamp` twin.

`nros-c` / `nros-cpp` write a per-build sizes header into the cargo target dir
together with a `<name>.h.stamp` sidecar:

    <target>/nros-cpp-generated/nros/nros_cpp_config_generated.h
    <target>/nros-cpp-generated/nros/nros_cpp_config_generated.h.stamp

The pair is written by ONE function, header first, stamp second
(`write_header_to_target_dir`). So a stamp with no header beside it is a state
the writers cannot produce, and it is the state issue 0834 found on two zephyr
XRCE leaves after a `lane=all` sweep.

## Why a gate rather than only a fix

The DISAPPEARANCE was never rooted — the failing build dirs were `rm -rf`'d,
which is what recovered them, and the evidence went with them. What IS
established (reproduced 2026-08-29):

* the build script legitimately declines to write when its probe yields 0
  (`cpp.rs`: "no RMW backend means no executor sizes to ship"), and on that path
  it writes NOTHING and touches nothing;
* so once the header is absent, only a build that probes non-zero restores it,
  and a no-backend build over the same target dir "succeeds" while leaving it
  absent — deleting the header and re-running `cargo build -p nros-cpp` does not
  bring it back.

That makes the state persistent, and the consumer-side symptom is a stub
`#error` (C++) or `SESSION_OPAQUE_U64S undeclared` (C) from a file nobody was
looking at. This gate names the directory instead, in milliseconds, and covers
the nros-C side where the failure is not self-describing.

Runs its own negative control on every invocation, per AGENTS.md.
"""

import os
import subprocess
import sys

ROOT = subprocess.run(
    ["git", "rev-parse", "--show-toplevel"],
    capture_output=True, text=True, check=True,
).stdout.strip()

# The two generated-header trees. Both have the same layout and the same hazard;
# 0088 records the C side being latent for a whole phase because its symptom is
# an undeclared macro rather than an `#error`.
GEN_DIRS = ("nros-c-generated", "nros-cpp-generated")

# Directories that cannot CONTAIN a `nros-*-generated` and are expensive to
# walk. The generated dirs sit directly inside a cargo target dir, so nothing
# below cargo's own bookkeeping or a CMake/Zephyr build tree can hold one —
# and those are where essentially all the files are. Pruning them took the
# whole-repo scan from 8.2 s to well under a second, which is the difference
# between a fast-lane gate and one nobody runs.
SKIP = {
    ".git", "node_modules", "__pycache__", ".claude", "external", "third-party",
    # cargo bookkeeping
    "deps", ".fingerprint", "incremental",
    # CMake / Zephyr / west build interiors
    "CMakeFiles", "zephyr", "modules", "_deps",
}


def orphans_in(dirpath):
    """Stamps with no header beside them, in one `<gen>/nros/` directory."""
    try:
        names = set(os.listdir(dirpath))
    except OSError:
        return []
    out = []
    for name in sorted(names):
        if not name.endswith(".h.stamp"):
            continue
        if name[: -len(".stamp")] not in names:
            out.append(os.path.join(dirpath, name))
    return out


def scan(root):
    found = []
    for dirpath, dirnames, _ in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in SKIP]
        base = os.path.basename(dirpath)
        if base in GEN_DIRS:
            found.extend(orphans_in(os.path.join(dirpath, "nros")))
            # Nothing below a generated dir holds another one.
            dirnames[:] = []
    return found


def self_test():
    """Prove the check can fail, and that a healthy pair does not trip it."""
    import tempfile

    bad = []
    with tempfile.TemporaryDirectory() as d:
        gen = os.path.join(d, "build-x", "nros-cpp-generated", "nros")
        os.makedirs(gen)
        header = os.path.join(gen, "nros_cpp_config_generated.h")
        stamp = header + ".stamp"

        # 1. Healthy pair — silent.
        open(header, "w").close()
        open(stamp, "w").close()
        if scan(d):
            bad.append("flagged a HEALTHY header+stamp pair")

        # 2. The 0834 state — stamp outlives its header.
        os.remove(header)
        if len(scan(d)) != 1:
            bad.append("MISSED a stamp whose header is absent")

        # 3. Header alone is fine: a build may legitimately not have stamped
        #    yet, and this gate is about the stamp outliving the header, not
        #    the reverse.
        open(header, "w").close()
        os.remove(stamp)
        if scan(d):
            bad.append("flagged a header with no stamp (not this defect)")

    if bad:
        print("check-orphan-generated-stamp SELF-TEST FAILED:", file=sys.stderr)
        for b in bad:
            print(f"  {b}", file=sys.stderr)
        return 1
    print("check-orphan-generated-stamp self-test: OK (3 case(s))")
    return 0


def main():
    if self_test() != 0:
        return 2
    if "--self-test" in sys.argv:
        return 0

    found = scan(ROOT)
    if found:
        print(
            "check-orphan-generated-stamp: FAILED — generated header(s) lost, "
            "stamp left behind (issue 0834):",
            file=sys.stderr,
        )
        for f in found:
            print(f"  {os.path.relpath(f, ROOT)}", file=sys.stderr)
        print(
            "\n  A stamp is written immediately AFTER its header by one function, "
            "so this pair cannot be produced by the writers.\n"
            "  The header will not come back on its own: a build whose probe "
            "yields 0 declines to write it and reports success.\n"
            "\n  Recover by wiping the build dir that owns it, then rebuild:\n"
            "      rm -rf <the build-* directory above>\n",
            file=sys.stderr,
        )
        return 1

    print("check-orphan-generated-stamp: OK (no generated header lost its stamp)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
