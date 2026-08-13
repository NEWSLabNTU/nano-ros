---
id: 548
title: "The XRCE C shim links against the retired `nros_platform_clock_{ms,us}`, so every Zephyr XRCE leaf fails at link — tier 2's next blocker"
status: resolved
resolved_in: issue-0548
type: bug
area: rmw-xrce
related: [phase-352, rfc-0073, issue-0528, issue-0532]
---

## Symptom

`just build-test-fixtures lane=tier2`, from a clean `build/sizes-probe`, now gets
to leaf 12 and dies at link:

```
/usr/bin/ld: …/zephyr.elf.loc_cpusw.o: in function `xrce_session_drive_io':
  packages/rmw/xrce/nros-rmw-xrce/src/session.c:508: undefined reference to `nros_platform_clock_ms'
  packages/rmw/xrce/nros-rmw-xrce/src/session.c:510: undefined reference to `nros_platform_clock_ms'
  packages/rmw/xrce/nros-rmw-xrce/src/session.c:519: undefined reference to `nros_platform_clock_ms'
/usr/bin/ld: …: in function `uxr_millis':
  packages/rmw/xrce/nros-rmw-xrce/src/platform_aliases.c:27: undefined reference to `nros_platform_clock_ms'
/usr/bin/ld: …: in function `uxr_nanos':
  packages/rmw/xrce/nros-rmw-xrce/src/platform_aliases.c:33: undefined reference to `nros_platform_clock_us'
collect2: error: ld returned 1 exit status
```

Target `build-rs-action-client-xrce` (Zephyr native_sim). The zephyr module is an
order-only prerequisite of every other platform, so this takes the tier-2 fixture
build down exactly as issue 0528 did.

## Cause

RFC-0073 / phase-352 (`bde6638ed`) replaced `nros_platform_clock_{ms,us}` with
`nros_platform_clock_ns` plus **static inline** wrappers in
`packages/platform/nros-platform-api/include/nros/platform.h`:

```c
static inline uint64_t nros_platform_clock_us(void) { return nros_platform_clock_ns() / 1000u; }
static inline uint64_t nros_platform_clock_ms(void) { return nros_platform_clock_ns() / 1000000u; }
```

No port defines those symbols any more — the wrappers are the definition, and a
caller gets them by including the header.

The XRCE C shim DOES include it (`platform_aliases.c:24` — `#include
"nros/platform.h"`), so on this path the include must be resolving to a stale
copy that still declares them `extern`: the Zephyr build has its own include
plumbing, and a header reached through a module/export path is not the one in
`packages/platform/`.

That is the same failure family as the stale committed `nros_generated.h` fixed
in `5dc2fa869` earlier the same day — the clock rename landed without every
header CONSUMER following it, and each consumer fails differently (that one a
`static`-follows-`extern` compile error, this one an undefined reference).

## Where to look

* which `nros/platform.h` the Zephyr XRCE build actually resolves — the zephyr
  module's include dirs, and any copied/exported header under
  `zephyr-workspace/` or the module's `zephyr/` dir;
* whether the shim should call `nros_platform_clock_ns()` directly instead. Both
  call sites immediately convert (`uxr_millis`, `uxr_nanos` — the latter
  multiplies microseconds back up by 1000, so it currently loses precision the ns
  clock has). Migrating the caller is probably better than making the wrapper
  reachable: RFC-0073's point was that ms/us are lossy views.

## Relationship to issue 0528

Found by running the lane for 0528's acceptance. 0528's own symptom is GONE —
zero `EXECUTOR_OPAQUE_U64S too small` in a from-scratch tier-2 build with the
probe dir wiped, where it previously took out six leaves — and the build now
reaches leaf 12. This is the next blocker in the same lane, not a recurrence.

## Acceptance

* `just build-test-fixtures lane=tier2` gets past `build-rs-action-client-xrce`.
* No first-party C source references `nros_platform_clock_{ms,us}` unless it can
  see the inline definition; a `git grep` for the retired names is the cheap
  check, and it should probably become a gate given this is the second consumer
  the rename missed.


## RESOLVED 2026-08-13 — both acceptance criteria met

**1. The lane gets past the leaf.** `build-rs-action-client-xrce` is
`zephyr-fixture-12` in the full sweep's target list, and the module completed:

```
just build-test-fixtures            (lane=all)
  == zephyr == OK
  clock undefined refs            0
  zephyr fixtures built           69
```

**2. The reference sweep is clean.** All 15 first-party C/C++ files naming
`nros_platform_clock_{ms,us}`: twelve `#include <nros/platform.h>` and so see
the `static inline` wrappers; the other three are
`nros-platform-api/include/nros/platform.h` (which IS the definition) and two
files that mention the names only in prose explaining they are retired
(`nros-board-threadx-qemu-riscv64/startup.c`,
`zephyr/nros_platform_zephyr_shims.c`).

The call sites in this shim were migrated to `nros_platform_clock_ns()`
directly, as this issue recommended — `uxr_nanos` had been multiplying
microseconds back up by 1000 and losing the resolution the ns clock has.

**The gate this issue asked for is issue 0555**, with a measured caveat worth
recording here: replaying the actual pre-fix sources through it, #547's
`internal.hpp` is caught (3 hits) and THIS issue's `platform_aliases.c` is NOT.
That file included the header and declared nothing; what failed was the include
RESOLVING to a stale copy on Zephyr's include path, which is a property of the
build's `-I` order and not of any source text. 0555 covers the hand-declaration,
missing-include and second-tracked-copy shapes; the stale-untracked-copy shape
stays a build-side problem (issue 0196's rule).

Note the evidence predates the 2026-08-13 afternoon pull (`1283130ac`).
