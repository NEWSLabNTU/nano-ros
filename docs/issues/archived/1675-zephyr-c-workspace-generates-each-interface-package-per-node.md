---
id: 1675
title: "A Zephyr C workspace entry generates each shared interface package once
  PER NODE PACKAGE — the guard meant to stop that keys on a target only the C++
  path creates — and since issue 1636 every C workspace image fails to link"
status: resolved
type: bug
area: [zephyr, build, codegen]
severity: high
found: 2026-10-03
related: [1636, 1673, 1645, phase-263]
---

## Symptom

Found by `just build-test-fixtures lane=all` once issue 1673 cleared the Zephyr
C + Cyclone DDS images. With that fix in place, the run built all 74 Zephyr
fixtures and exactly three still failed to link — every Zephyr **C workspace**
entry:

* `build-ws-c-entry-zenoh`
* `build-ws-c-realtime-entry-zenoh`
* `build-ws-c-realtime-entry-smp` (`qemu_cortex_a53/smp`)

```
app/libapp.a(builtin_interfaces_msg_duration.c.obj): in function
  `builtin_interfaces_msg_duration_get_type_support':
talker_pkg/nano_ros_c/builtin_interfaces/msg/builtin_interfaces_msg_duration.c:19:
  multiple definition of `builtin_interfaces_msg_duration_get_type_support';
  app/libapp.a(builtin_interfaces_msg_duration.c.obj):listener_pkg/nano_ros_c/…
```

and likewise for every `std_msgs` binding.

## Cause

`talker_pkg` and `listener_pkg` each generated their own copy of the shared
interface packages' C bindings, and both compiled into the one Zephyr `app`.

`zephyr/cmake/nros_generate_interfaces.cmake` has a guard for exactly this —
phase-263 C2c, *"idempotent across node pkgs that share an interface"* — keyed on

```cmake
if(TARGET ${target}_${_nros_iface_lang}_ffi_build)
  return()
endif()
```

**Only the C++ path ever creates that target** (line 568, `${target}_cpp_ffi_build`,
the FFI staticlib it builds). The C path builds no FFI library and creates no
`_c_ffi_build`, so for C the guard can never fire. Measured in the failing build:
no `*_c_ffi_build` target exists, while `build-ws-cpp-entry-zenoh` has both
`std_msgs_cpp_ffi_build` and `builtin_interfaces_cpp_ffi_build`. So C workspaces
have duplicated their bindings since the guard was written (June); C++ ones never
did.

**Why it surfaced now.** The global `--allow-multiple-definition` kept the first
copy of each symbol. Issue 1636 (`f513b9cdcd`, 2026-10-02) scoped that flag to
C++ FFI images; its comment records that a C image links without it, measured on
the single-package `c/talker` — one node, so one generation, so no duplicate.
Multi-node C workspaces were not measured. Same class as issue 1673: something
the global flag hid, now a link error.

## Fix

The guard keys on a language-scoped GLOBAL PROPERTY,
`_NROS_IFACE_GENERATED_<target>_<lang>`, set by the first generation, so it says
the same thing on both paths. The `TARGET … _ffi_build` check stays as a second
witness for C++. The property is per-language on purpose: a mixed workspace needs
both the C and the C++ bindings for one package.

## Verified

All three failing images build, and each was relinked by the run, 0 duplicates.
Controls build too:

```
ok fresh=yes dups=0   ws-c-entry-zenoh
ok fresh=yes dups=0   ws-c-realtime-entry-zenoh
ok          dups=0   ws-c-realtime-entry-smp   (qemu_cortex_a53/smp has no zephyr.exe;
                                                zephyr.elf relinked inside the run)
ok fresh=yes dups=0   ws-cpp-entry-zenoh        (control: the C++ guard path)
ok fresh=yes dups=0   ws-mixed-entry-zenoh      (control: C AND C++ for one package)
ok fresh=yes dups=0   c-talker-zenoh            (control: single-package C)
```

**Runtime — all three repaired images, measured:**

* `entry_e2e` — `zephyr/c/entry_pubsub` ran and passed: the C workspace image's
  talker delivered `/chatter` cross-process to the native C listener entry. The
  matrix prints only skipped and failed cells (`ran = cells − skipped − failed`,
  `entry_e2e.rs`), and this cell is in neither list. Its first attempt could not
  run because the native peer `workspace-c-native-robot2` was stale (issue 0445's
  absorbing verdict, one fixture over); the native lane rebuilt it.
* `realtime_tiers_e2e` — PASS, 18 rows run, including
  `build-ws-c-realtime-entry-zenoh`.
* `sched_dims_applied_e2e` — PASS, with
  `[zephyr c CorePinPlacement] ACCEPT` on `build-ws-c-realtime-entry-smp` and
  `[zephyr c EdfDeadline] ACCEPT` on `build-ws-c-realtime-entry-zenoh`.

**Observed in the same run, not caused here:** the Zephyr **Rust** `params`,
`lifecycle` and `qos` entry cells failed at runtime. Those images were built
before this change and do not use the C generation path it touches; they are
recorded rather than investigated.
