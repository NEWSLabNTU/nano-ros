---
id: 1023
title: "`nros_sertype.cpp` includes `<memory>` and `<string>`, so cyclonedds cannot compile for a freestanding target"
status: open
area: rmw, build
severity: high
found: 2026-09-04
related: [0970, 0968, 0942, 0112, phase-393]
---

# The sertype the CDR-blob path needs is written in hosted C++

## What happened

`just build-test-fixtures lane=tier2`, on a tree at `a1c6d0d22`:

```
== threadx_riscv64 == FAILED (rc=2)
FAILED: nros_rmw_cyclonedds.dir/src/nros_sertype.cpp.obj
packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/nros_sertype.cpp:23:10:
    fatal error: memory: No such file or directory
```

The other seven modules of that lane built: zephyr, native, qemu, freertos,
nuttx, threadx_linux — this is one module, and it is a COMPILE failure, not a
runtime one.

## It is a declared coordinate, not an unsupported one

`examples/fixtures.toml:3146` pairs threadx_riscv64 with cyclonedds for
`test_threadx_riscv64_cyclonedds_two_qemu_pubsub`. So this is a cell the tree
says it supports, and it cannot be built.

## Provenance: issue 0970, and the file fixed ONE include of this class already

`nros_sertype.cpp` is new in `b4858f941` ("feat(#0970): the Cyclone backend
registers its own sertype"). Before it, the Cyclone path used Cyclone's
generated sertype and this TU did not exist, so the coordinate built.

The file already carries a note about exactly this class, for a different
include:

```c
// Issue 0942 — `<cstdio>` is only required to declare these in `std`, and a
// freestanding libstdc++ does the reverse, so a board toolchain can fail on
// `std::snprintf` where every hosted target passed. Unqualified, from
// `<stdio.h>`, as `descriptors.cpp` does.
#include <stdio.h>
```

So the class was known HERE, and fixed at the reported site only — which is
CLAUDE.md's "fix the CLASS, not the reported site" verbatim. The remaining two
hosted includes are three lines below that comment.

## What is actually used, so the fix is not an include deletion

* `<memory>` — `std::unique_ptr<NrosSerdata>` at three sites
  (`serdata_from_ser`, and its two siblings), each `new (std::nothrow)` with
  early returns on failure.
* `<string>` — `NrosSertype::type_name`, used four ways: `==` between two
  sertypes (`:335`), a char loop for the hash (`:343`), assignment from
  `desc->m_typename` (`:391`), and `.c_str()` into `ddsi_sertype_init_flags`
  (`:392`).
* `<new>` and `<cstring>` are fine: `<new>` is a freestanding header, and
  `<cstring>` resolved on this toolchain (the compiler stopped at `<memory>`,
  which is three lines later).

## The fix, scoped — and the part that needs a decision

`std::unique_ptr` is mechanical: a ten-line local RAII holder, or explicit
`delete` on each early return. Prefer the holder — the manual version needs
every return path audited and this is a serdata allocation path.

`std::string` is NOT mechanical, and the reason is lifetime.
`st->type_name = desc->m_typename` currently COPIES. Replacing it with a
`const char*` stores a pointer into the caller's descriptor, and whether that
outlives the sertype is not established here — `create_nros_sertype(desc)`'s
callers would each have to be checked. The safe shape is `ddsrt_strdup` freed in
the sertype's free op, which also keeps the "ddsrt heap, not libc heap" rule the
file already states for its buffers (Phase 177.26.RX.2: on ThreadX and FreeRTOS
the libc heap is separate from the ddsrt heap, so `new[]` can return null for
every message on a board where `ddsrt_malloc` works).

**Not attempted here.** A memory-lifetime change in a serdata path is not
something to land unverified, and the coordinate that would verify it is the one
that cannot build.

## Why nobody noticed

Issue 0968's thesis, showing up as a BUILD break rather than a test one:
`post-submit`'s tier-2 job has never run (interlocked on an unset
`vars.NROS_SELF_HOSTED_READY`), and `host-tests` has been red on issue 0967. So
no fixture-backed lane has built this coordinate since 0970 landed.

Found while doing 0968's step 1 — "rebuild tier-2 fixtures, nothing here is
currently reproducible" — which is the first tier-2 fixture build in some time.

## Not blocking 0968's own work

None of 0968's twelve runtime failures are on threadx_riscv64: they are esp32
(5), threadx_linux (3), zephyr xrce-cpp (3) and one qemu-rtic. Those modules
built. This issue is a separate finding from the same run.
