---
id: 1901
title: "`sizes-header-mirrors` reports every metadata-probe build dir that shares a keyed Corrosion cargo dir as STALE after ANY one of them builds; the second writer is the sibling probe, and the dir's own next build repairs it"
status: resolved
type: bug
area: [build, testing]
severity: medium
found: 2026-10-15
resolved: 2026-10-15
related: [1911, 1792, 1783, 0805, 0978, 0268, 1593, 1832]
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

## First CI sightings: it now stops both tier lanes (2026-10-14)

Run-matrix **38637410239** (workflow_dispatch, main `5ab72ab02`). Both fixture
jobs ran on `newslab-118-server` in the persistent fixture tree (phase-489 W6)
and failed before any cell, in the `check fast` that opens `just build`:

- **116104004118** `tier 2 (1-wise matrix)`, step `just build tier2`
- **116114632835** `tier 1 (cells)`, step `just build tier1`

```
check-fast (parallel): 1 of 450 gate(s) FAILED
===== FAIL (sizes-header-mirrors, rc=1, 14091ms) =====
STALE MIRROR: …/nros-tests/fixtures/image_config_ws/build/nros-metadata/metadata-probe-cmake/build/nano_ros/packages/api/nros-c/include/nros/nros_config_generated.h
         vs: …/metadata-probe-cmake/build/cargo/nros-fixture-tree_15c46/nros-c-generated/nros/nros_config_generated.h
             source: #define EXECUTOR_OPAQUE_U64S 11353
             mirror: #define EXECUTOR_OPAQUE_U64S 11352
```

That is the shape described above, now in CI. The executor grew by 8 bytes on
main (2248 → 2256 value size), the shared cargo dir was rebuilt for one probe,
and every sibling probe's mirror reads stale. The persistent tree is what makes
it bite in CI: a fresh checkout has no sibling mirror to go stale. Until this
is fixed, every tier-1/tier-2 dispatch on a self-hosted runner whose tree
predates an `nros-c` size change stops at this gate.

## Resolution

**The main fix landed as [issue 1911](1911-sizes-mirror-gate-judges-unbuilt-dirs.md)**
(commit `843ced628c`), which hit the same defect in CI. It took this issue's
first fix direction: `mirror-generated-header.sh` records each run in a
marker under `<build>/CMakeFiles/nros-mirror-ran/`, and
`check-sizes-header-mirrors.py` counts a differing pair as drift only when
that run is at least as new as the source. Otherwise the pair is "pending
its build dir's next build". That answers "What is wrong" items 1 and 2 for
the gate verdict. 1911's self-tests cover both generators.

Two things 1911 left open. This change closes both, in
`scripts/check-sizes-header-mirrors.py`.

**1. `--fix` copied into pending dirs.** The skip read
`if not fix and not ran_after(...)`, so `--fix` re-mirrored every differing
pair, pending ones included. That is the copy this issue warned about: the
new header goes in beside the dir's older archive. Measured on
`examples/workspaces/{realtime-c,c}`, whose probes share
`build/corrosion-cargo/metadata-probe/<key>`. Setup: add a temporary
`#define` to both header writers and a temporary `#[no_mangle]` static to
`nros-c`, build both probes, revert all three, rebuild **only** realtime-c.

- The `origin/main` gate's `--fix` then printed `re-mirrored 2 stale
  pair(s)` and reported `OK — 6 pairs`. c's mirror had lost the `#define`,
  but c's `libnros_c.a` still exported the reverted static. The gate called
  an inconsistent dir clean.
- With this change, `--fix` re-mirrors drift only. For a pending pair it
  prints `NOT re-mirrored: N pending pair(s) in <build dir>`, says why, and
  gives the remedy (`cmake --build <build dir>`: its build re-mirrors before
  any consumer compiles, issue 1783).
- c's own rebuild then cleared both the archive symbol and the mirror
  define. The gate read `OK — 6 pairs`.

**2. A marker-less dir read as pending forever.** A dir last built before
1911 has no marker, so every differing pair there read as pending until a
rebuild. That included a dir whose own build ran after the source moved
and left the mirror behind (the 0268 shape). Measured on the c probe:

- Rebuild c, so its archive is newer than the source.
- Put the old mirror back and move the markers aside, giving the exact
  state of a pre-1911 dir with 0268 drift.
- The `origin/main` gate said `OK … 1 mirror(s) … pending`, rc 0. This
  change says `STALE MIRROR`, rc 1, and `--fix` re-mirrors it.

The fail-open is real. On this host it is also currently empty. Across the
three checkouts, 328 pairs were not source-gone. All 328 were marker-less,
because the main checkout and the p1903 worktree were built before 1911
landed. None differed.

**How a marker-less dir is judged now.** It goes by its own uplifted
archive, the `.a` beside the mirror command's leaf-copy argument. Corrosion
copies that file in with `copy_if_different`, and only on that dir's build.
A sibling rebuilding the shared cargo store never moves it.

- **Archive newer than the source:** the dir built after the source moved,
  so a differing mirror is drift.
- **Archive older:** pending.
- **Marker present:** the marker alone decides. 1911's case (a sibling
  moved the source, or a partial build copied the archive without the
  mirror) stays pending on both paths.

**What the archive cannot see.** A source change that leaves the archive
bytes alone (a header-only edit, measured: the `#define` alone did not move
c's `libnros_c.a`) does not move the archive. A marker-less dir in that state
still reads as pending. That errs toward pending, never toward a false
STALE, and the dir's next build writes a marker. A size change, the case
this gate exists for, changes the archive.

**Self-test.** `_pending_cases` runs on both generators (ninja and
Makefile). It checks:

- a pending pair, with no marker and no archive;
- `--fix` leaving a pending pair untouched;
- a marker-less dir whose archive predates the source (pending) or
  postdates it (drift, which `--fix` repairs);
- a marker older than the source, beside a newer archive (pending);
- a marker newer than the source (drift).

Mutation proofs, each applied and grep-confirmed, then restored
byte-identical. The make-only column neutralises the ninja call, so the
Makefile case has to catch the mutation alone:

| mutation | self-test rc | make-only rc |
| --- | --- | --- |
| `--fix` re-mirrors pending again | 1 | 1 |
| archive bridge removed | 1 | 1 |
| archive overrides an older marker | 1 | 1 |
| marker-less always counts as built | 1 | 1 |

Baseline and restored: rc 0.

**Still unmeasured:** the first two items in "Unmeasured" above. Nothing
here depends on them.
