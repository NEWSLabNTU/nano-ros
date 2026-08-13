---
id: 548
title: "The XRCE C shim links against the retired `nros_platform_clock_{ms,us}`, so every Zephyr XRCE leaf fails at link — tier 2's next blocker"
status: open
type: bug
area: rmw-xrce
related: [phase-350, rfc-0073, issue-0528, issue-0532]
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

RFC-0073 / phase-350 (`bde6638ed`) replaced `nros_platform_clock_{ms,us}` with
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
