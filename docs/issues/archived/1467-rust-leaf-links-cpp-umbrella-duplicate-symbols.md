---
id: 1467
title: "A generated C interface library links the C++ umbrella whenever that target
  exists, so a pure-Rust leaf links `libnros_cpp.a` beside its own Rust staticlib and
  every ThreadX-RV64 rust leaf dies on ~19 duplicate symbols"
status: resolved
type: bug
area: cmake, codegen, threadx, ci
severity: high
found: 2026-09-23
resolved: 2026-09-27
related: [1355, 0425, 0734, 0666, 1280]
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

## Resolution — 2026-09-27

The umbrella decision now belongs to the CONSUMER, and it has one home:
`cmake/NanoRosRuntimeUmbrella.cmake`.

### The mechanism

Two rules met at nine sites, each spelling both:

1. **WHICH umbrella** — 0425's answer, unchanged. `NanoRos::NanoRosCpp` BUNDLES
   nros-c since Phase 241.D3-rev, so where it exists it satisfies both ABIs and
   a mixed workspace keeps exactly ONE Rust staticlib.
2. **WHETHER the consumer wants one** — this issue. Unanswerable from the
   declaring site, because `if(TARGET ...)` asks what the tree DEFINES.

`nros_link_runtime_umbrella(<target> <scope> [CANDIDATES ...])` resolves (1) and,
for **PUBLIC/INTERFACE only**, wraps the pick in

```cmake
$<$<NOT:$<BOOL:$<TARGET_PROPERTY:NROS_CARRIES_RUST_RUNTIME>>>:NanoRos::NanoRosCpp>
```

`$<TARGET_PROPERTY:prop>` with no target name is evaluated against the **head
target** — the binary being linked. That is the only mechanism that can answer
this from inside a library codegen'd *before* the executable exists, and which in
a workspace may be consumed by binaries with different answers.
`nros_threadx_rv64_rust_app` calls `nros_declare_rust_runtime_carrier(${target})`.

Measured on **cmake 3.22.1** (this tree's `cmake_minimum_required`), with a
purpose-built probe covering the three hops the real graph has — an ALIAS of an
INTERFACE target, a PUBLIC static-library hop, and an IMPORTED `_ffi_lib` hop:

* the guarded consumer's `LINK_LIBRARIES` loses the umbrella;
* the unguarded consumer keeps it **and keeps the Phase 150.B ORDER**
  (`libmsglib.a libumbrella_archive.a` — umbrella after the message archive);
* the genex survives `install(EXPORT)`, exported verbatim.

### PRIVATE links stay literal, deliberately

A PRIVATE link on a binary is NOT guarded and keeps the umbrella's literal name
in `LINK_LIBRARIES`. Two reasons, both load-bearing:

* the consumer IS the target, so the choice is already the consumer's; and
* `cmake/board/nano-ros-board-{qemu-armv7a,rv-virt}-nuttx.cmake` compare that
  executable's `LINK_LIBRARIES` entries as **strings**
  (`if(_lib STREQUAL "NanoRos::NanoRosCpp")`) to skip the umbrella while ferrying
  its include dirs into the cargo cross-build. A generator expression is not a
  name those comparisons can match, and the failure would surface as the Phase
  155.B.5 `nros_config_generated.h` `#error` minutes later — not as a bad link.

So the rule is: **guard what PROPAGATES; leave what the target chose for itself.**

### Acceptance — MEASURED, same leaf, same configure command

`examples/rv-virt-threadx/rust/listener`, `-DNROS_RMW=zenoh`,
`-DNANO_ROS_PLATFORM=threadx_riscv64`, riscv-none-elf-gcc 14.2.0, reading
`LINK_LIBRARIES` for `riscv64_threadx_rust_listener` out of the generated
`build.ninja`.

BEFORE (this branch with `cmake/` reverted to `origin/main`):

```
  LINK_LIBRARIES = libstd_msgs__nano_ros_c.a  librv_virt_threadx_listener.a  nano_ros/packages/api/nros-cpp/libnros_cpp.a  nano_ros/libthreadx_glue.a  nano_ros/libvirtio_net_netx.a  -lc  nano_ros/nros_platform_threadx/libnros_platform_threadx.a  nano_ros/libnetxduo.a  nano_ros/libthreadx_kernel.a  …
```

AFTER:

```
  LINK_LIBRARIES = libstd_msgs__nano_ros_c.a  librv_virt_threadx_listener.a  nano_ros/libthreadx_glue.a  nano_ros/libvirtio_net_netx.a  -lc  nano_ros/nros_platform_threadx/libnros_platform_threadx.a  nano_ros/libnetxduo.a  nano_ros/libthreadx_kernel.a  …
```

`libnros_cpp.a` is gone from the link line and from every other link line in that
manifest; it is still BUILT (the root `add_subdirectory` builds it regardless of
who links it), which is why the remaining textual hits are its own
`CUSTOM_COMMAND` copy rules.

### And the direction that must NOT break — same configure, same library

A before/after pair proves the umbrella left. It does not prove it still ARRIVES
where it is wanted, and a fix that silently dropped it everywhere would read
identically. So both consumers were put in ONE configure of the same leaf, sharing
the one generated `std_msgs__nano_ros_c`: the Rust carrier, plus a plain
`add_executable` + `target_link_libraries(... PRIVATE std_msgs__nano_ros_c)` —
which is exactly how a C/C++ leaf reaches that library.

```
build riscv64_threadx_rust_listener: C_EXECUTABLE_LINKER__riscv64_threadx_rust_listener_Release …
  LINK_LIBRARIES = libstd_msgs__nano_ros_c.a  librv_virt_threadx_listener.a  nano_ros/libthreadx_glue.a  nano_ros/libvirtio_net_netx.a  -lc  nano_ros/nros_platform_threadx/libnros_platform_threadx.a  nano_ros/libnetxduo.a  nano_ros/libthreadx_kernel.a  …

build probe_c_consumer: C_EXECUTABLE_LINKER__probe_c_consumer_Release …
  LINK_LIBRARIES = libstd_msgs__nano_ros_c.a  nano_ros/packages/api/nros-cpp/libnros_cpp.a  nano_ros/libthreadx_glue.a  nano_ros/libvirtio_net_netx.a  -lc  nano_ros/nros_platform_threadx/libnros_platform_threadx.a  nano_ros/libnetxduo.a  nano_ros/libthreadx_kernel.a  …
```

Two consumers of one library, opposite answers, and `probe_c_consumer` keeps the
umbrella **after** the message archive — the Phase 150.B order the `_ffi_lib`
ordering hook exists to record. The probe target was appended to the leaf's
`CMakeLists.txt` for the measurement and reverted; it is not in the tree.

**What was NOT run here, stated plainly:** no leaf was LINKED, and no C/C++ leaf
was configured in its own right. The measurement is the generated link line, which
is the claim this issue makes — the 228 `duplicate symbol` errors are `lld` reading
exactly these two archives — and the C-direction evidence is a synthetic consumer
of the real library rather than `examples/rv-virt-threadx/c/listener`. That leaf's
configure was attempted and abandoned after ~25 min: it goes through
`cmake/bootstrap.cmake`, whose `git -C <repo-root> submodule status` ran against the
PARENT checkout (an inherited absolute root, issue 1280's class) on a box with five
concurrent agents and `State: D` disk-sleep throughout — and a configure that
resolves the parent's tree would have measured the wrong `cmake/` anyway. Acceptance
for the lane — `threadx_riscv64` linking its twelve leaves and reaching its cells —
is a CI measurement and remains open on the nightly.

### Sweep

Every site where the umbrella is a PROPAGATED requirement:

```
grep -rn 'NanoRos::NanoRos\b\|NanoRos::NanoRosCpp\|nros_cpp::nros_cpp\|nros_c::nros_c' \
    cmake/ CMakeLists.txt nano_rosConfig.cmake \
  | grep -E 'target_link_libraries|INTERFACE_LINK_LIBRARIES'
```

Nine, all converted: the codegen **C** branch (the reported one), the codegen
**CPP** branch, the `_ffi_lib` ordering hook, `nano_ros_node_register`'s component
lib, `nano_ros_auto_add_library`, `nros_components_register_node`,
`ament_auto_add_library`, and the `rclcpp` / `rclcpp_components` /
`diagnostic_updater` compat forwards.

`integrations/nuttx/CMakeLists.txt` surfaces the umbrella on NuttX's `apps`
INTERFACE target and is deliberately left literal: `CRATE_TYPES staticlib` appears
**exactly once** in the tree (the ThreadX RV64 board), so no NuttX binary can be a
Rust-runtime carrier, and the literal name is what the NuttX board walkers match.
`examples/**` and the test fixtures keep their literal PRIVATE links — that line is
the published API a user writes.

### Gate

`check-runtime-umbrella-link-sites` (fast line; membership is derived from the
recipe name in `just/check/cmake.just`). It refuses a PUBLIC/INTERFACE literal
umbrella link, an `INTERFACE_LINK_LIBRARIES` write naming one, and a
`target_link_libraries` whose scope is a VARIABLE — which is the spelling the
reported site actually carried (`${_link_type}`), so a scope the gate cannot read
is a failure rather than a default. PRIVATE links are RULED per file with a reason,
checked in both directions, so a tenth site is a decision and not a copy.

Nine negative controls, run on the NORMAL path rather than behind the flag —
`check-gate-selftests` requires that and CAUGHT this gate in the flag-only shape on
its first `check-fast` run, which is the rule working on the gate that was written
to make another rule work. Plus the strongest control available: run against
`origin/main`'s cmake tree it reports **16 violations**, naming
`cmake/NanoRosGenerateInterfaces.cmake:768` — the site this issue diagnosed.
Against this branch, **0**.

### For 1355

1355's remaining acceptance ("the lane reaching a verdict") was blocked by this.
Its named cause — the riscv64 board building ThreadX's `linux/gnu` port — did not
recur in this configure: `nros_resolve_threadx_port` selected the bare-metal port
and the kernel and NetX Duo targets generate cleanly.
