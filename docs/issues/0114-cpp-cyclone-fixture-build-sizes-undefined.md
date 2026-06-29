---
id: 114
title: "`just native build-fixture-extras` (standalone) fails compiling native C++ Cyclone fixtures — generated size constants undefined"
status: open
type: bug
area: build
related: [phase-267]
---

## Summary (root cause UNCONFIRMED)

Running `just native build-fixture-extras` standalone (to build the native
C/C++ + Cyclone test fixtures) failed compiling the native **C++ Cyclone**
fixtures. The terminal error is a cascade from undefined generated size
constants:

```
nros_generated.h:2022: ‘SERVICE_SERVER_OPAQUE_U64S’ undeclared here
nros_generated.h:2127: ‘SERVICE_CLIENT_OPAQUE_U64S’ undeclared here
nros_generated.h:2236: ‘NROS_LIFECYCLE_CTX_OPAQUE_U64S’ undeclared here
subscription.hpp:473: ‘class nros::Subscription<std_msgs::msg::Int32>’ has no member named ‘storage_’
```

The `storage_` "no member" error is a CASCADE, not a real API regression:
`Subscription::storage_` IS declared (`alignas(8) uint8_t
storage_[NROS_SUBSCRIBER_SIZE]`, `subscription.hpp`), but the array bound
`NROS_SUBSCRIBER_SIZE` comes from the GENERATED config header
(`nros_config_generated.h`, template `@PROBE_SUBSCRIBER@`). When that constant is
undefined the member declaration is dropped, so every use reports "no member".

## Why root cause is unconfirmed

Both copies of the generated header were PRESENT on the failing tree
(`packages/core/nros-c/include/nros/nros_config_generated.h` tracked, AND
`target/nros-c-generated/nros/nros_config_generated.h`), each defining
`NROS_SUBSCRIBER_SIZE` + the `*_OPAQUE_U64S` constants — yet the C++ Cyclone
fixture build still saw them undefined. So the likely culprit is the **cmake
include path** for the native-cyclonedds-cmake fixtures not resolving the
generated config header (or `build-fixture-extras` assuming a predecessor step
that the standalone invocation skips), rather than a missing header per se.

Two candidate explanations, neither confirmed:
1. **Build-ordering / standalone invocation:** `build-fixture-extras` is normally
   reached via the full `just native build-all` (after `build`); run alone it may
   miss a generation/probe step that the cyclone-cpp include path depends on.
2. **A real include-path gap** in the `native-cyclonedds-cmake` fixture cells
   (the generated config dir not added to the cpp Cyclone target's includes).

## Repro / next step

Reproduce via the canonical full chain — `just build-test-fixtures` (or
`just native build-all`) — and see whether the C++ Cyclone fixtures compile. If
they do, this is invocation-ordering (downgrade to a docs note on
`build-fixture-extras` prerequisites); if they still fail, it is a real
include-path gap in the native-cyclonedds-cmake fixture cells. Did NOT block the
phase-267 W-B test wave (the declarative bridge test gates on the cyclone
listener fixture and skips cleanly; the C bridge path was validated separately).
