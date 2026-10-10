---
id: 1783
title: "After a codegen-version bump, an incremental C/C++ build compiles message libraries against a stale `include/nros/nros_config_generated.h` mirror and fails, until it is re-run (or wiped)"
status: resolved
type: bug
area: [build, cmake]
severity: medium
found: 2026-10-10
resolved: 2026-10-11
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

## Resolution (2026-10-11)

### Root cause: the mirror was keyed on a proxy that does not move

Reproduced in a fresh worktree. Build `examples/threadx-linux/c/*` (zenoh)
and both `freertos_posix` workspace images at codegen version 10. Then bump
`NROS_CODEGEN_VERSION{,_MIN}` to 11, restamp the six committed
`packages/interfaces` crates, rebuild the CLI, and build once. The result was
16 `#error`s on threadx-linux and 2 on `workspaces/c`. Afterwards, in
`examples/threadx-linux/c/talker/build-zenoh`:

```
cargo/<ws>/nros-c-generated/nros/nros_config_generated.h   01:50:33  v11   <- cargo rewrote it
cargo/<ws>/.../release/libnros_c.a                          01:50:36  6828024 B
nano_ros/packages/api/nros-c/libnros_c.a                    01:44:56  6828024 B  <- NOT updated
nano_ros/packages/api/nros-c/include/nros/nros_config_generated.h   01:44:56  v10
nano_ros/packages/api/nros-cpp/include/nros/nros_config_generated.h 01:45:15  v10
```

Issue 0268 made `$<TARGET_FILE:nros_{c,cpp}-static>` the mirror's input, and
0740's per-consumer stamp copied that. The premise was that a header that
changes comes with an archive that changes. For a codegen-version bump it does
not: the constant is a `#define` that nothing in the archive reads, so cargo
relinked a BYTE-IDENTICAL `libnros_c.a`. Corrosion's `copy_if_different` then
kept the build-dir copy's mtime, and ninja restat'd the proxy as unchanged.
Neither the mirror nor the stamp ran. The mirror's real source is a file cargo
writes as a side effect, and no CMake rule produces it, so no proxy can stand in
for it.

There was a second, independent defect, which is the "the nros-cpp mirror is
not an input at all" observation. A generated C message library links
`NanoRosCpp` whenever the build has it (issue 0425). That target prepends the
nros-**cpp** mirror dir, so the TU's `<nros/nros_config_generated.h>`
resolves there. Yet `nros_generate_interfaces` stamped only the nros-**c**
mirror. The compile line shows this: include dir 8 is
`…/nros-cpp/include`, and dir 11 is `…/nros-c/include`.

### Fix: one module, and both mirror and stamp re-run every build

`cmake/NanoRosConfigHeaderMirror.cmake` is now the one home for this class:

- `nros_config_header_mirror()` is the producer, used by both
  `packages/api/nros-{c,cpp}/CMakeLists.txt`. It creates one custom command
  per header. Each command depends on an always-out-of-date SYMBOLIC node
  (`_nros_config_header_rerun_node`) and is ordered after the cargo build.
  It does not take the archive as an input. The mirror script is
  write-if-changed, and ninja custom commands are `restat`, so consumers
  recompile only when the header content moves. Each command has exactly one
  output, because the Makefile generator `touch_nocreate`s every output after
  the first. That was measured on the old two-output nros-cpp rule
  (cmake 3.22), and it would have re-stamped the header on every build.
- `_nros_config_header_stamp()` moved here from `NanoRosNodeRegister.cmake`.
  It re-runs the same way and `copy_if_different`s. Stamps are now keyed per
  crate (`nros-c-…` / `nros-cpp-…`) because both crates mirror a
  `nros_config_generated.h`.
- `nros_config_header_files()` and `nros_config_header_object_depends()` are
  now the ONLY consumer spelling. The five sites that each read the two GLOBAL
  properties by hand go through them: the generated message library, both
  `nano_ros_entry` arms, and both ThreadX carrier arms. The message library
  therefore stamps both crates' mirrors.

### Before / after: `ninja -t query` on one message object

Before:

```
CMakeFiles/std_msgs__nano_ros_c.dir/nano_ros_c/std_msgs/msg/std_msgs_msg_string.c.o:
  input: C_COMPILER__std_msgs__nano_ros_c_Release
    …/std_msgs_msg_string.c
    | _nros_cfg_stamp/std_msgs__nano_ros_c/nros_config_generated.stamp
    || cmake_object_order_depends_target_std_msgs__nano_ros_c
_nros_cfg_stamp/std_msgs__nano_ros_c/nros_config_generated.stamp:
  input: CUSTOM_COMMAND
    nano_ros/packages/api/nros-c/libnros_c.a        <- the proxy
    nano_ros/packages/api/nros-cpp/libnros_cpp.a
    || … nros_c_config_header || … nros_cpp_config_header
```

After:

```
CMakeFiles/std_msgs__nano_ros_c.dir/nano_ros_c/std_msgs/msg/std_msgs_msg_string.c.o:
  input: C_COMPILER__std_msgs__nano_ros_c_Release
    …/std_msgs_msg_string.c
    | _nros_cfg_stamp/std_msgs__nano_ros_c/nros-c-nros_config_generated.stamp
    | _nros_cfg_stamp/std_msgs__nano_ros_c/nros-cpp-nros_config_generated.stamp
    | _nros_cfg_stamp/std_msgs__nano_ros_c/nros-cpp-nros_cpp_config_generated.stamp
    || cmake_object_order_depends_target_std_msgs__nano_ros_c
_nros_cfg_stamp/std_msgs__nano_ros_c/nros-cpp-nros_config_generated.stamp:
  input: CUSTOM_COMMAND
    _nros_cfg_stamp/std_msgs__nano_ros_c/rerun      <- always out of date
    || … nros_c_config_header || … nros_cpp_config_header …
nano_ros/packages/api/nros-cpp/include/nros/nros_config_generated.h:
  input: CUSTOM_COMMAND
    nano_ros/packages/api/nros-cpp/CMakeFiles/nros_cpp_config_header.rerun
    || … cargo-build_nros_cpp
```

### Acceptance (measured)

The test was not run on the build dirs that had already failed. Every dir was
first rebuilt at v10 with the fix. Then the bump was applied and each image was
built exactly ONCE:

| image | result | mirrors after |
| --- | --- | --- |
| `examples/threadx-linux/c/*` (zenoh, all 6 roles) | rc=0, 0 `#error` | nros-c v11, nros-cpp v11 |
| `workspaces/c` freertos_posix | rc=0, 0 `#error` | v11 / v11 |
| `workspaces/cpp` freertos_posix | rc=0, 0 `#error` | v11 / v11 |

No-op cost, measured: a second `ninja` in the threadx talker compiled and
linked 0 objects. A second `cmake --build` of `workspaces/c` under the Makefile
generator also compiled and linked 0. The mirror and stamp commands run, but
they write nothing.

### Gate

`check-config-header-single-writer` already owned "which bytes are the sizes
header" (issues 0985 and 1746). It now also enforces rules E1–E6:

- E1: the build-time mirror rule exists only in the helper.
- E2: no custom command keys on the staticlib proxy, including when it is bound
  through `set()` or `foreach()`. Both pre-fix sites were spelled that way.
- E3: the mirror and the stamp depend on the rerun node and have one output
  each.
- E4: no consumer reads a mirror property or calls the stamp directly.
- E5: the helpers exist.
- E6: each image/library creator still calls the consumer helper as often as
  it creates such a target (`nros_generate_interfaces` 1, `nano_ros_entry` 2,
  `nano_ros_node_register` 2). E1–E4 police how a consumer spells its edge and
  are silent on one that has NO edge: with only E1–E5, commenting out the
  message library's call left the gate green (rc=0). A creator that
  disappears fails too, so the table cannot drift toward OK.

Mutation proof:

- Whole fix reverted: rc=1, 24 findings.
- Stamp re-keyed on `$<TARGET_FILE:nros_c-static>`: rc=1 (E2, E3).
- Message library stamping only the nros-c mirror: rc=1 (E4 ×2).
- Message library's `nros_config_header_object_depends` call commented out:
  rc=0 before E6, rc=1 after (E6). Same for one of `nano_ros_entry`'s calls.
- Fixed tree: rc=0.

Not changed, noted: `scripts/check-sizes-header-mirrors.sh` still globs
`packages/core/nros-*`, which is where the crates were before they moved to
`packages/api/`. Its comparison therefore finds no pairs, and it records the
run as NOT VERIFIED rather than as a pass. Re-pointing it also means comparing
against the shared cargo copy rather than the leaf copy (issue 0978), so it is
left for its own change.
