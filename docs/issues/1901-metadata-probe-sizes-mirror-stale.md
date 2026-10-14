---
id: 1901
title: "`sizes-header-mirrors` reports every metadata-probe build dir that shares a keyed Corrosion cargo dir as STALE after ANY one of them builds; the second writer is the sibling probe, and the dir's own next build repairs it"
status: open
type: bug
area: [build, testing]
severity: medium
found: 2026-10-15
related: [1792, 1783, 0805, 0978, 0268, 1593, 1832]
---

## What was seen

During a pre-push, `sizes-header-mirrors` failed on
`examples/workspaces/*/build/nros-metadata/metadata-probe-cmake/build/…` with
`NROS_CODEGEN_VERSION 14` in the mirror against `15` in its source. In
`examples/workspaces/c` (checkout `p500w2`, Unix Makefiles generator):

```
2026-10-14 06:36:03 nano_ros/packages/api/nros-c/include/nros/nros_config_generated.h   (mirror)
2026-10-14 06:36:03 nano_ros/packages/api/nros-c/libnros_c.a
2026-10-14 09:31:02 cargo/p500w2_06501/nros-c-generated/nros/nros_config_generated.h      (source)
```

Two explanations were possible: something ran cargo into that dir without the
cmake mirror step, or the mirror step is missing an edge.

## Cause: the source is shared, the mirror is not

phase-489 W4 shares the C/C++ probe's Corrosion cargo dir across workspaces
(`metadata_probe_cmake::configure_project` passes
`-DNROS_SHARED_CARGO_ROOT=<checkout>/build/corrosion-cargo/metadata-probe`).
Issue 0805's `nros_shared_cargo_dir` keys the directory by the cargo
**inputs**: features, rmw, board, caps, profile, target, knobs. It does not key
by workspace. Each probe build dir's `cargo` is a symlink into that keyed dir.
Measured on this tree after `just build-test-fixtures lane=linux`:

```
examples/workspaces/c          -> build/corrosion-cargo/metadata-probe/ddb2119461cd
examples/workspaces/mixed      -> build/corrosion-cargo/metadata-probe/ddb2119461cd
examples/workspaces/realtime-c -> build/corrosion-cargo/metadata-probe/ddb2119461cd
examples/workspaces/cpp        -> build/corrosion-cargo/metadata-probe/d1088a68552b
examples/workspaces/derived-tiers-cpp, realtime-cpp, realtime-cpp-subnode-portable,
  violation-cpp                -> …/d1088a68552b
features -> 64bbb5df69c4, managed -> 38265e322fb9, safety -> 7a3cd6d1891d
```

(plus `examples/templates/{pure-c,multi-package,c-and-cpp-mixed}-workspace` and
`nros-tests/fixtures/image_config_ws`, which also share `ddb2119461cd`; see
below.)

So the header under `cargo/<checkout>_<hash>/nros-c-generated/` is written by
whichever sharer last ran cargo. The mirror
(`nano_ros/packages/api/nros-{c,cpp}/include/nros/…`) is per build dir and is
refreshed only when **that** dir builds. `mirror-generated-header.sh` picks the
newest `cargo/*/<gen>/nros/<name>`, and `check-sizes-header-mirrors.py`
compares each mirror against that same `--resolve` answer (issue 1792). The
result: after any one sharer builds against a changed header, every other
sharer reads as drifted until it builds itself.

The second writer is the **sibling probe**. Nothing writes without the copy
step, and the copy step has its edge.

## Reproduction on this tree (`e8400d5a92`)

Baseline after the fixture build:

```
$ python3 scripts/check-sizes-header-mirrors.py
sizes-header mirrors OK — 299 mirror/source pair(s) across 107 build dir(s) (49 of them Makefile-generator)
```

The header content was changed without touching the CLI closure (a bump of
`nros-core/src/codegen_version.rs` re-runs the probe's configure and is refused
as a STALE CLI). A temporary `#define NROS_REPRO_1901 1` went into both writers
of `nros_config_generated.h`:
`packages/api/nros-c/templates/nros_config_generated_exact.h.template` and the
nros-c-format string in `nros-build-helpers/src/cpp.rs`. Both are needed:
`write_shared_header` leaves a comment-only change alone (issue 1461), and with
the define in one writer only, nros-cpp's verifier panics with "DIFFERENT probed
sizes". Then **only** realtime-c's probe was built, the way `run_probes` does
it:

```
$ NROS_WORKSPACE=$PWD NROS_CLI=$PWD/packages/cli/target/release/nros \
    cmake --build examples/workspaces/realtime-c/build/nros-metadata/metadata-probe-cmake/build \
    --parallel --target probe_ctrl_pkg__ctrl probe_telem_pkg__telem
rc=0
$ python3 scripts/check-sizes-header-mirrors.py
STALE MIRROR: examples/workspaces/c :: nros-c/include/nros/nros_config_generated.h
STALE MIRROR: examples/workspaces/c :: nros-cpp/include/nros/nros_config_generated.h
STALE MIRROR: examples/workspaces/mixed :: nros-c/…   (and nros-cpp/…)
STALE MIRROR: examples/templates/pure-c-workspace :: …            (×2)
STALE MIRROR: examples/templates/multi-package-workspace :: …     (×2)
STALE MIRROR: examples/templates/c-and-cpp-mixed-workspace :: …   (×2)
STALE MIRROR: packages/testing/nros-tests/fixtures/image_config_ws :: …  (×2)
detector rc=1
```

realtime-c, the dir that built, is clean. The six dirs that did not build are
all flagged. That matches the evidence: c's mirror **and** its `libnros_c.a`
both at 06:36 means c's probe had not built since. It is not an
archive-without-mirror state.

Then **c's** probe was built the same way:

```
[ 12%] nano-ros: mirroring nros_config_generated.h for in-tree consumers
c mirror: 2026-10-15 02:59:59  NROS_REPRO_1901 present
$ python3 scripts/check-sizes-header-mirrors.py   → c no longer listed; the other 5 dirs still are
```

So the mirror edge works (issue 1783's always-re-run node, Makefile generator
included), and a dir's own build repairs it. The edits were then reverted, and
realtime-c and c were rebuilt. The detector is back to `OK — 299 pairs`. No
build dir was deleted.

## What is wrong, then

1. **The verdict is wrong for an unbuilt sharer.** The detector says "every
   consumer TU in those trees compiles against the wrong storage sizes". For a
   dir that has not built since its shared source moved, nothing has compiled
   against anything yet. Its next build re-mirrors before any consumer
   compiles. The failure blocks a push (it did) and recommends `--fix`, which
   copies the new header into a dir whose archive (`libnros_c.a`, 06:36) still
   predates it. That makes the dir **less** consistent until it builds.
2. **The "source" of an unbuilt dir is not its source.** `--resolve` answers
   "what would the next build copy", while the gate asks "is this dir
   consistent with what it compiled". Those agree only for a dir that owns its
   cargo dir, which phase-489 W4 ended for every probe build dir.

**Unmeasured:**
- whether anything consumes a probe build dir's mirror without building that
  dir first (a gate, a census, an IDE `compile_commands.json`);
- whether the same false verdict hits the fixture build dirs that share a
  Corrosion cargo dir through `NROS_SHARED_CARGO_ROOT` (issue 0805's original
  sharers). The mechanism is the same.

## Fix direction

Let the detector tell "not built since the shared source moved" apart from
"built and still stale". Two candidates:

- **mtime:** a pair whose source is newer than the dir's own last mirror run
  (or its uplifted archive) is reported as a NOTE ("not rebuilt since a sharer
  moved the shared header"), never as drift. A mirror whose dir built after the
  source moved and still differs is the 0268 failure, and stays a FAIL.
- **content:** have `mirror-generated-header.sh` record the source's hash
  beside the mirror when it copies. The gate compares mirror to recorded hash
  (did the copy happen?), and recorded hash to current source only to emit the
  note.

`--fix` should refuse, or rebuild, a dir in the note state rather than copy
into it.

Gate: extend `self_test()` in `check-sizes-header-mirrors.py` with this case. Two
build dirs share one `cargo/` target; build A; B must read "not rebuilt", not
"STALE"; then corrupt A's mirror after its build, and A must read STALE. Keep
the case on both generators, since issue 1832 says the CI lanes are Unix
Makefiles.
