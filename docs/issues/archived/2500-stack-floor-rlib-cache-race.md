---
id: 2500
title: "Two links of one configure race on the stack-floor cache: the rlib extraction and the llc object both write shared paths"
status: resolved
type: bug
area: [memory, tooling, build]
severity: medium
found: 2026-11-13
related: [issue-2400, phase-550]
---

**Home: issue [2400](../2400-stack-floor-from-measured-frames.md)'s stack-floor walk (`scripts/nros-stack-floor.py`)**

## Symptom

A fixture build of `examples/workspaces/cpp` for the
`freertos-cyclonedds-mps3-an536-freertos` coordinate failed at link time in
the stack-floor step:

```
File "scripts/nros-stack-floor.py", line 402, in rlib_bitcode
    m.unlink()
FileNotFoundError: [Errno 2] No such file or directory:
  '.../cmake/nros/stack-floor/llc/rlib/3ed0e82b…/<member>.o'
gmake[5]: *** [...: mps3_an536_slirp_entry] Error 1
```

The log printed `nano-ros: stack floor of slirp_bringup@mps3-an536-freertos`
twice, once for `mps3_an536_slirp_entry` and once for
`mps3_an536_slirp_bulk_entry`: the two links ran in parallel in one cmake
build directory. A second session's tier-2 run hit the same failure, and
found a second site of the same class (below).

## Cause

Two links of one configure walk the same inputs and share one cache
(`<build>/nros/stack-floor/llc/`). The walk wrote that cache in two places,
and both assumed only one writer.

1. `rlib_bitcode()` extracted a sysroot rlib in place, in a directory keyed
   on its content (`rlib/<sha256>/`): `ar x`, dump each member's `.llvmbc`,
   delete each `.o`, then write `.complete`. Two processes found no
   `.complete`, extracted into the SAME directory, and one deleted a member
   the other had just written. Its `m.unlink()` then raised.
2. `llc_object()` lowered bitcode through a FIXED temporary name,
   `<hash>.o.tmp`, before renaming it to `<hash>.o`. Two processes lowering
   the same bitcode wrote into one file, and one's rename could take the
   other's half-written output, or leave it nothing to rename.

The race is intermittent. Its cause is permanent: any image with two entries
sharing a configure can hit it.

## Fix

Each writer works in a place of its own and publishes in one atomic step:

1. The rlib is extracted into a `tempfile.mkdtemp` directory beside the cache
   entry, marked `.complete`, then renamed into place. A process that loses
   the rename uses the winner's copy. An unfinished directory left by the old
   in-place code is replaced.
2. `llc` writes to a `tempfile.mkstemp` name in the cache directory, which is
   then `replace()`d onto `<hash>.o`. The temporary file is removed on any
   failure.
3. The per-image result table, which both entries of one image write, goes
   through a `mkstemp` name and one `replace()` as well, so a reader never
   sees half of it.

Measured: the fixture lane (`just build-test-fixtures lane=tier2`) failed its
freertos stage on site 1. With the rlib fix applied, that stage passed. The
script's self-test passes (67 checks) with both fixes.
