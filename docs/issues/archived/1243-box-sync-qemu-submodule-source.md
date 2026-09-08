---
id: 1243
title: "The box mirror dropped 2914 tracked files, and the first fix silenced coverage instead of restoring it"
status: resolved
area: tooling, ci
severity: medium
related: [1229, 0925, 1055, 0870, phase-440, RFC-0095]
---

# 2914 tracked paths, four causes, and one wrong turn

`check-box-sync-covers-tracked-source` reported **2914** tracked paths that
`ros2-box-sync.sh` would drop. Issue 1229 had widened the sweep from one index
to every repository the mirror copies — the right fix for a real blindness — and
this was the cost, unmeasured: on a provisioned host the gate went red about
trees a bare CI worktree never sees, and the reds hid each other.

Four independent causes, each the class named in `ros2-box-sync.sh`'s own header
(**a build-output NAME that is also a source directory**), each found only after
the one in front of it was cleared:

| tree | rule | tracked files lost |
| --- | --- | --- |
| `third-party/qemu/qemu` | `target/` | 1936 |
| `external/` (kani, colcon-\*, a stray px4) | `build-*/`, `target/` | ~970 |
| `esp-idf-workspace/**/protobuf-c` | `build-*/` | 2 |
| `third-party/px4/…/pymavlink` | `node_modules/` | 2 |

## The wrong turn, recorded because the reasoning was plausible

The first fix partitioned the sweep: a repository inside a directory the
SUPERPROJECT gitignores is provisioning, not this repo's source, so do not sweep
it. It measured well — 978 paths out of the verdict, and the gate's answer no
longer depended on what the operator had installed.

**It was wrong.** `zephyr-workspace` is gitignored too, so the partition would
have stopped the gate asserting the Zephyr rescue that issue #758 added for a
REAL breakage ("`build-rust-talker-zenoh` could never build ANY Zephyr target").
The mirror copies gitignored trees — rsync does not read `.gitignore` — so
"gitignored" says nothing about whether the box needs a file. The partition
silenced coverage rather than restoring it, and would have done so invisibly.

Caught by mutating an EXISTING rescue and finding the gate no longer noticed.
The measurement that settled it: rules-only sweeps **200,571 paths across 107
trees**; with the partition, **110,159 across 69**. Same green, half the tree
unexamined.

## What actually fixed it

Rules only. No change to the gate.

* **QEMU submodules excluded from the mirror**, both of them. Its source uses
  `target/`, `build/` and `build-*/` as SOURCE names and it vendors whole other
  projects that do the same — edk2 → openssl → krb5. Rescuing it by `--include`
  is open-ended: measured 1936 → 1032 → 958 after two rules with no end. It
  needs no rescue, because **nothing builds it here**: `[tool.qemu]` provisions
  a released tarball (11.0.0-nros6), the submodule is absent from
  `build_sources`, and its one consumer
  (`.github/actions/setup-qemu-patched`) runs `git submodule update --init`
  ITSELF on a CI runner. `third-party/esp32/qemu` is the same and weaker still —
  nothing under `just/`, `scripts/`, `cmake/` or `.github/` names that path at
  all. `third-party/qemu/patches/` stays: small, tracked, and what makes the pin
  legible.
* **`external/` excluded** — vendored tool sources plus a 23 M stray duplicate
  of the 68 G tracked px4 submodule. RFC-0095 D3 retires the directory into the
  store; this is that decision arriving early.
* **Two narrow rescues**, in the shape the file already uses for zephyr and px4:
  `/esp-idf-workspace/**/build-cmake/***` (protobuf-c keeps tracked CMake files
  there) and `/third-party/px4/PX4-Autopilot/**/node_modules/***` (pymavlink
  vendors `jspack` and `long` as tracked gitlinks).

## Verified

`check-box-sync-covers-tracked-source`: **OK — 200,571 tracked path(s) across
107 source tree(s)**, up from 69 examined under the abandoned partition.

Each rescue mutation-tested, so none is decoration:

| rule dropped | paths lost |
| --- | --- |
| zephyr `target/` (pre-existing, #758) | 5 |
| esp-idf `build-cmake/` | 2 |
| pymavlink `node_modules/` | 2 |

## What this does not fix

The gate still costs more the more a host provisions, because provisioned source
and build output share directories and no name-based rule separates them. That
is RFC-0095 D1/D2 (phase-440 W3/W4): with provisioned trees out of the
repository, nothing provisioned sits under the sync root and this class stops
recurring. Until then each new provisioning root is another rule someone has to
remember.
