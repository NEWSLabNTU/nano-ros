---
id: 1783
title: "After a codegen-version bump, an incremental C/C++ build compiles message libraries against a stale `include/nros/nros_config_generated.h` mirror and fails, until it is re-run (or wiped)"
status: open
type: bug
area: [build, cmake]
severity: medium
found: 2026-10-10
related: [issue-0834, issue-0088, issue-1360, phase-483]
---

## What was measured

`main` moved the emitted codegen version from 9 to 10 (phase-483 W1,
`33a72e1116`). On the next incremental build of existing fixture build
directories, the generated message C sources failed with their own guard:

```
nano_ros_c/unique_identifier_msgs/msg/unique_identifier_msgs_msg_uuid.h:24:2: error: #error "nros: this generated tree was emitted at a codegen version the runtime does not accept (see NROS_EMITTED_CODEGEN_VERSION above against NROS_CODEGEN_VERSION_MIN..NROS_CODEGEN_VERSION in nros_config_generated.h) ..."
```

It hit two families on 2026-10-10:

- `just threadx_linux build-fixture-extras`: 16 `#error`s across the
  threadx-linux C/C++ role examples. It failed again on a second run, and the
  only fix that worked was deleting their `build-{zenoh,cyclonedds}` dirs.
- `just freertos build-fixtures-posix`: the `workspaces/c` and
  `workspaces/cpp` `freertos_posix` images. Run 1 failed. Run 2 fixed `c` and
  failed `cpp`. Run 3 passed. So it does converge, one stale mirror per run.

The newly emitted message headers are version 10, and so are the runtime's
own `nros-{c,cpp}/nros_config_generated.h`. The stale file is the MIRROR
under `include/nros/`. From `examples/workspaces/c/build/freertos-cyclonedds-freertos-posix/cmake`
after run 1 (HH:MM:SS, version, path):

```
04:32:43 10 nano_ros/packages/api/nros-c/nros_config_generated.h
04:32:44 10 nano_ros/packages/api/nros-c/include/nros/nros_config_generated.h
04:32:44 10 nano_ros/packages/api/nros-cpp/nros_config_generated.h
03:25:37  9 nano_ros/packages/api/nros-cpp/include/nros/nros_config_generated.h   <- stale
```

The threadx-linux C talker showed the same thing on the `nros-c` side:
`include/nros/` was 02:15 / v9, beside a v10 `nros-c/nros_config_generated.h`
written at 04:02.

`ninja -t query` on a message object shows the edge it has:
`_nros_cfg_stamp/<lib>/nros_config_generated.stamp`, which depends on
`libnros_c.a` / `libnros_cpp.a`, with `nros_c_config_header` only
order-only (`||`). So nothing forces a message TU to wait for, or re-read, a
mirror that a later step refreshes. The `nros-cpp` mirror is not even an
order-only input of a C message library that includes it.

## Why it matters

Every codegen-version bump will hit every existing incremental C/C++ build
this way. The error names a generated header and says to "regenerate it",
but the generated tree is already current. The stale file is the runtime's
mirror. That is issue 0834's shape (a mirror that one run does not repair),
reached by a version bump rather than a race.

## Direction

The mirrored header has to be a real (not order-only) input of everything
that includes it, for BOTH crates' mirrors, or the message TUs have to
include the canonical per-build header rather than the mirror. Acceptance:
bump `codegen_version.rs`, build an existing fixture dir once, and it
succeeds.
