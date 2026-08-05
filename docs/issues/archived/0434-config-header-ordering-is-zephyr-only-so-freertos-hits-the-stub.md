---
id: 434
title: The per-build config-header ordering is scoped to Zephyr, so a FreeRTOS C++ TU compiles against the in-tree #error stub
status: resolved  # fixed 2026-08-05
type: bug
area: build
related: [issue-0088, issue-0090, issue-0326, issue-0268, phase-337, rfc-0069]
---

## Problem

FreeRTOS C++ example fixtures fail to compile:

```
packages/api/nros-c/include/nros/nros_config_generated.h:37:2: error:
  #error "nros_config_generated.h must be supplied per-build by the build system"
packages/api/nros-c/include/nros/nros_generated.h:949:20: error:
  'SESSION_OPAQUE_U64S' was not declared in this scope
packages/api/nros-c/include/nros/nros_generated.h:1040:20: error:
  'EXECUTOR_OPAQUE_U64S' was not declared in this scope
```

The failing TU is `cpp_action_client`'s `src/main.cpp`, reaching the C header
through `parameter.hpp` → `parameter.h` → `types.h`. Reproduced on a freshly
wiped build dir, so it is not a stale cache.

## Cause — NOT the ordering I first wrote

The first diagnosis here blamed the Zephyr-guarded `add_dependencies` /
`OBJECT_DEPENDS` block in `NanoRosVerbs.cmake`, and proposed lifting it out of
the guard. **That was wrong, and lifting it changed nothing** — the paths in that
block (`nros-rust/nros-c-generated/…`) are the ZEPHYR build layout, which is why
the guard is there. Reverted.

Two facts settled it:

1. **It is deterministic, not a race.** Two consecutive builds with the generated
   headers already on disk failed identically. Ordering cannot explain that.
2. **The include LIST order is wrong.** For the failing TU
   (`cpp_action_client/src/main.cpp`):

   ```
    9: -I …/packages/api/nros-c/include                       ← SOURCE, has the stub
   10: -I …/build-zenoh/nano_ros/packages/api/nros-cpp/include ← has the real header
   13: -I …/build-zenoh/nano_ros/packages/api/nros-c/include   ← empty
   14: -I …/packages/api/nros-c/include                        ← the legitimate INTERFACE dir
   ```

   Position 9 wins, so `<nros/nros_config_generated.h>` resolves to the stub.

Position 9 comes from `cmake/board/nano-ros-board-mps2-an385-freertos.cmake`,
where phase-337 W5.b added the SOURCE include dir to `FREERTOS_STARTUP_INCLUDES`
so `freertos_c_entry.c` could read `NROS_APP_CONFIG` from `<nros/app_config.h>`,
with the comment:

> "The per-app generated header shadows this one on the include path when the
> carrier emits it; both spell the same type."

It does not shadow it — it is shadowed BY it. The assumption was about ordering
and never checked.

## Fix

Remove that entry. The dir is still reachable: `nros-c` exports it as an
INTERFACE include, so it appears later in the same list (position 14) and
`app_config.h` still resolves — adding it early was redundant as well as
harmful.

Verified: freertos `cpp` fixtures build (RC=0, 0 stub errors) and `c` fixtures
still build (RC=0, no `app_config.h` error). All three freertos action Runtime
cells — C, C++ and Rust — pass.

## Consequence

`freertos cpp` action/service/pubsub example fixtures cannot build, so those
Runtime cells never run. It blocked RFC-0069's last acceptance item for the
`freertos c`/`freertos cpp` action cells (the raw↔raw pairs the action payload
envelope change alters).

It also aborts `just freertos build-fixtures` partway, which is why the C++
examples show binaries dated days earlier while the C ones are current — the
recipe never reached them.

## Fix direction

Lift the cargo `add_dependencies` and the `OBJECT_DEPENDS` out of the Zephyr
guard, leaving only `target_include_directories(app …)` inside it. Apply the
same at the two `NanoRosNodeRegister.cmake` sites so one rule covers every
platform rather than three copies each with its own scope.
