---
id: 52
title: check-cpp freestanding C++ probe — missing nros-platform-api include + main.hpp uses hosted std::printf under -ffreestanding
status: resolved
type: tech-debt
area: build
related: [phase-241, issue-0051]
resolved_in: Phase 241 W11 follow-up
---

## Why

`just check` → `check-cpp` (the per-header freestanding C++14 syntax probe) has two
pre-existing failures, exposed only after issue #51 unblocked the lane ahead of it:

1. **Missing include path.** `heap_sequence.hpp` (`377854191`, Phase 229.5,
   2026-06-10) does `#include <nros/platform.h>`, which lives in
   `packages/core/nros-platform-api/include/nros/`. The probe's `-I` set lists
   `nros-cpp/include`, `nros-c/include`, and the two generated variant dirs, but
   NOT `nros-platform-api/include` → `fatal error: nros/platform.h: No such file
   or directory`.

2. **`main.hpp` is hosted, probed freestanding.** `main.hpp` (the `EntryNodeRuntime`
   readiness/sample banners, `a7ce7e7da`, Phase 238.E) calls `::std::printf`. It
   `#include <cstdio>`, but `-ffreestanding` does not require a hosted `<cstdio>` to
   declare `std::printf` (only the global `printf`), so g++ rejects `::std::printf`
   → `'printf' is not a member of 'std'`. `main.hpp` is the host/native (and NuttX)
   entry runtime — it legitimately uses hosted I/O for the e2e readiness banners;
   the actual embedded targets that link it have `printf` (the cross cells are
   green). It is simply not a freestanding-target header.

Both predate the RFC-0042 D3 work (phase-241) — surfaced running `just check` for
D3, same broad-`just check` class as `f78a16989`.

## Fix

In the `check-cpp` recipe (`justfile`):

1. Add `-Ipackages/core/nros-platform-api/include` to the syntax-probe `-I` set.
2. Probe `main.hpp` in HOSTED mode (drop `-ffreestanding` for that one header)
   rather than excluding it — keeps full syntax coverage while honouring that it is
   a hosted entry runtime. (Mirrors the existing `rclcpp_compat.hpp` carve-out,
   but coverage-preserving.)

With both, every `nros-cpp/include/nros/*.hpp` passes the probe.

Out of scope (separate, harder): the `f78a16989`-noted nros-c `platform/posix.h`
C11 `_Atomic`/`atomic_load_explicit` under the g++ umbrella-header check — it does
NOT surface in this probe (the C++ headers don't pull posix.h here).

## Status

RESOLVED. The active failures are fixed in the `justfile` `check-cpp` recipe
(platform-api `-I` + hosted main.hpp probe); check-cpp passes. The "out of scope /
harder" `_Atomic` sibling is now MOOT: phase-248's platform-vtable refactor
(`df324f542` C3 / `6ddba94e0` C2) removed the `nros-c platform/posix.h` header that
used C11 `_Atomic`/`atomic_load_explicit` — no platform header carries `_Atomic`
anymore, so there is nothing left to fail the g++ umbrella-header check.
