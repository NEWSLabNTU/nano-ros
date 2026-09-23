---
id: 1467
title: "A generated C interface library links the C++ umbrella whenever that target
  exists, so a pure-Rust leaf links `libnros_cpp.a` beside its own Rust staticlib and
  every ThreadX-RV64 rust leaf dies on ~19 duplicate symbols"
status: open
type: bug
area: cmake, codegen, threadx, ci
severity: high
found: 2026-09-23
related: [1355, 0425, 0734]
---

## What happens

Nightly run **35830623546** (schedule, 07:13), job **107082715068**
(`threadx_riscv64`), step `Build (threadx_riscv64)`. All twelve leaves — six
examples (`listener`, `talker`, `service-server`, `service-client`,
`action-server`, `action-client`) × two RMWs (`cyclonedds`, `zenoh`) — fail at
**link**, with 228 `rust-lld: error: duplicate symbol` lines in total and the
same shape every time:

```
rust-lld: error: duplicate symbol: nros_rmw_cffi_lookup
>>> nros_rmw_cffi-….rcgu.o:(nros_rmw_cffi_lookup) in archive librv_virt_threadx_listener.a
>>> nros_cpp-….rcgu.o:(.text.nros_rmw_cffi_lookup+0x0) in archive nano_ros/packages/api/nros-cpp/libnros_cpp.a
```

```
rust-lld: error: duplicate symbol: __NROS_SIZE_EXECUTOR_SIZE
>>> nros-….rcgu.o:(__NROS_SIZE_EXECUTOR_SIZE) in archive librv_virt_threadx_action_client.a
>>> nros_cpp-….rcgu.o:(.rodata.__NROS_SIZE_EXECUTOR_SIZE+0x0) in archive nano_ros/packages/api/nros-cpp/libnros_cpp.a
```

The colliding symbols come from `nros`, `nros_platform` and `nros_rmw_cffi` —
the `#[no_mangle]` C ABI surface and the `__NROS_SIZE_*` size constants. They
are defined once inside the leaf's own Rust staticlib
(`librv_virt_threadx_<example>.a`) and once inside `libnros_cpp.a`, because the
C++ umbrella bundles the same Rust crates. Two archives, one ABI — and `lld`
stops at twenty errors per leaf.

## Where it comes from

`cmake/NanoRosGenerateInterfaces.cmake`, the `LANGUAGE C` branch (around line
763):

```cmake
    # issue 0425 — prefer `NanoRosCpp` when it exists. …
    if(TARGET NanoRos::NanoRosCpp)
      target_link_libraries(${_lib_target} ${_link_type} NanoRos::NanoRosCpp)
    elseif(TARGET NanoRos::NanoRos)
```

Issue 0425 reasoned over two umbrellas: a mixed workspace was dragging
`libnros_c.a` onto a C++ executable that already linked `libnros_cpp.a`, and
since the C++ one BUNDLES nros-c, preferring it keeps one Rust staticlib per
binary. That reasoning is still right for the case it names.

**A pure-Rust leaf is a third umbrella it does not model.** The leaf's app crate
is compiled as a staticlib and already carries the whole Rust surface, so the
target that keeps a C/C++ binary down to one archive puts a *second* archive on
a Rust binary. The condition is `if(TARGET NanoRos::NanoRosCpp)` — a property of
what the build tree DEFINES, not of what this executable's other archives
already contain — and every leaf here does
`add_subdirectory("${NANO_ROS_ROOT}" nano_ros)`, so that target always exists.

The path into the link is the generated message library: each leaf calls
`nros_generate_interfaces(std_msgs … LANGUAGE C)` and then
`nros_threadx_rv64_rust_app(… LINK std_msgs__nano_ros_c)`, and
`std_msgs__nano_ros_c` carries `NanoRos::NanoRosCpp` as a PUBLIC/INTERFACE link
dependency.

## Why it matters

This is what `threadx_riscv64` fails on now, and it is the only thing between
that lane and a verdict on its own cells. It is also not confined to ThreadX:
the rule is in the shared C-interface generator, so any pure-Rust entry that
links a generated C message library in a tree where the C++ umbrella target
exists is exposed to it. ThreadX-RV64 is simply where it is measured, because
that lane builds its rust leaves through cmake (phase-369) rather than through
cargo alone.

## What this is NOT

- **Not issue 1355.** That issue is the riscv64 board building ThreadX's
  `linux/gnu` port and dying in `tx_port.h` on `<semaphore.h>`. In this run the
  ThreadX kernel and the NetX Duo stack build clean for riscv64
  (`[233/754] Linking C static library nano_ros/libthreadx_kernel.a`), no port
  skip is printed, and the failure is 400 ninja edges later at link. 1355's
  named cause no longer fires; its acceptance ("the lane reaching a verdict")
  is now blocked by this.
- **Not a stale build directory.** These are fresh container builds, and the
  collision is between two archives both produced by this configure.
- **Not issue 0734** (`rmw-zenoh` linked twice) — that is one backend archive
  reaching the line twice; here two different umbrellas each bundle the same
  Rust crates, and it reproduces on `cyclonedds` as well as `zenoh`.

## How it became visible

The lane has been failing 12/12 for weeks, and the tally was all it printed.
PR #1192's `nros_run_quiet` (`scripts/build/quiet-run.sh`) replays a failed
leaf's captured output, which is what produced the `rust-lld` lines above. The
failure is not new; the attribution is.

## What would close it

1. Make the umbrella choice a property of the CONSUMER, not of the build tree.
   A generated C interface library linked into a Rust-staticlib entry must not
   pull in an umbrella that bundles the same Rust crates the entry already
   carries — the interface library needs the C ABI *declarations* and the
   ordering, not a second copy of the runtime.
2. Fix the class, not the ThreadX site: the same `if(TARGET
   NanoRos::NanoRosCpp)` preference appears for the `_ffi_lib` ordering hook
   (around line 681) and for the CPP branch. Sweep all of them and decide once.
3. Acceptance is `threadx_riscv64` linking its twelve rust leaves and reaching
   its cells — green or red on the cells, not on the link line.
