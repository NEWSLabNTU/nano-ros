#!/usr/bin/env bash
#
# A cargo profile defined in BOTH `Cargo.toml` and `.cargo/config.toml` must
# agree.
#
# WHY BOTH FILES EXIST
#
# They are not redundant, which is exactly why this drifts unnoticed:
#
#   * `Cargo.toml [profile.*]` applies to the ROOT WORKSPACE build.
#   * `.cargo/config.toml [profile.*]` applies to any cargo invocation made
#     from this directory tree — including the ~48 LEAF crates that live
#     outside the root workspace (board crates, drivers, bench/test bins).
#
# So a leaf built with `--profile nros-relwithdebinfo` reads the config copy and
# never sees the manifest one. Editing one and not the other silently gives
# half the tree different optimization settings, with no error anywhere: both
# files stay valid, both builds succeed, and the difference shows up only as a
# performance or size number nobody can explain.
#
# That is the same shape as every other mirror in this repo (the sizes-header
# family, the FFI struct mirrors, and — found the same day as this gate —
# `nros_orchestration_ir::TierRtosSpec` vs `ros_launch_manifest_sched::
# TierPlatformSpec`, where the narrower copy silently decided what users were
# allowed to write). Mirrors are fine; ungated mirrors are not.
#
# THE RULE
#
# For every profile name present in BOTH files, the key/value bodies must be
# identical. A profile present in only one file is fine and intentional:
# `Cargo.toml` carries `release`/`dev` overrides the leaves do not need.
#
# phase-336 added a THIRD copy — `packages/tooling/nros-cargo-profile`, the
# table cmake/bash/just read through `nros profile`. Its own tests check both
# files against it, so this gate (buildless, in `check-fast`) and those tests
# (behind a build) cover the same triangle from different sides.

set -uo pipefail
cd "$(dirname "$0")/.."

python3 - <<'PY'
import sys
try:
    import tomllib  # Python 3.11+
except ModuleNotFoundError:
    import tomli as tomllib

def profiles(path):
    with open(path, "rb") as fh:
        return (tomllib.load(fh).get("profile") or {})

manifest = profiles("Cargo.toml")
config = profiles(".cargo/config.toml")

shared = sorted(set(manifest) & set(config))
if not shared:
    print("cargo profile mirror OK — no profile defined in both files.")
    sys.exit(0)

status = 0
for name in shared:
    a, b = manifest[name], config[name]
    if a == b:
        continue
    status = 1
    print(f"[FAIL] [profile.{name}] differs between Cargo.toml and .cargo/config.toml:",
          file=sys.stderr)
    for key in sorted(set(a) | set(b)):
        va, vb = a.get(key, "<absent>"), b.get(key, "<absent>")
        if va != vb:
            print(f"       {key}: Cargo.toml={va!r} vs .cargo/config.toml={vb!r}",
                  file=sys.stderr)

if status:
    print("", file=sys.stderr)
    print("  Both copies are load-bearing: the manifest one applies to the root", file=sys.stderr)
    print("  workspace, the config one to every leaf crate outside it. A build", file=sys.stderr)
    print("  using the copy you did not edit succeeds with different settings", file=sys.stderr)
    print("  and reports nothing. Make them identical.", file=sys.stderr)
    sys.exit(1)

print(f"cargo profile mirror OK — {len(shared)} shared profile(s) identical: {', '.join(shared)}.")
PY
