---
id: 2131
title: "No C++ image was checked heap-free: the link gate covered the Rust runtime only, so a C or C++ path that allocates could land unseen"
status: resolved
type: limitation
area: [cpp, testing, memory]
severity: medium
found: 2026-10-29
related: [2115, 0816, 0843, 1493, 1812]
resolved: 2026-10-29
resolved_in: "the PR that archives this file"
---

## What

`just ci l3` links `heap-free-poc-mps2` and runs
`scripts/check-no-alloc-image.py --tier heap-free` on it. That image is Rust:
`nros` without `alloc`, the executor, component install, spin and a parameter
store. Nothing checked the C and C++ surfaces the same way.

Issue 2115 made nros-cpp COMPILE without `alloc` and gated that compile. A
compile says nothing about the link. An `alloc`-only call that is `cfg`'d
correctly compiles, and so does a C++ header that reaches `operator new`, a
C helper that calls `malloc`, or a platform feature that turns on a global
allocator. Only the linked symbol table can rule those out.

## Fix

`packages/testing/nros-tests/bins/heap-free-cpp-poc-mps2` is the C++ twin.
It runs on bare-metal MPS2-AN385 with a Rust link root, like the bare-metal
C++ examples.

- **nros-cpp and nros-c are built without `alloc`.** nros-cpp has
  `rmw-cffi,param-store,panic-halt`. The parameter store is carved from the
  executor backing (`NROS_PARAM_STORE=1` in the leaf's cargo config).
- **The platform comes in directly,** as `nros-platform-mps2-an385` with
  `cffi-export` and without its `libc-heap` shims. It does NOT come through
  `nros-c`'s `platform-mps2-an385` feature, because that feature also turns on
  `global-allocator`.
- **`FORCE_LINK_ANCHOR` keeps the whole C and C++ `#[no_mangle]` ABI in the
  link,** so the gate covers what a C++ image CAN reach, not just what this
  program names.
- **`src/poc.cpp` is ordinary freestanding nano-ros C++:**
  1. `rclcpp::init_in()`;
  2. `create_node`;
  3. `declare_parameter`, `set_parameter` and `get_parameter` on the node;
  4. `create_wall_timer`;
  5. `spin_once`;
  6. `shutdown`.
- **No RMW is linked.** Every in-tree transport allocates in its vendored C,
  which is the Rust image's reason too. The init is refused at run time,
  which the image reports as success. The calls after it stay in the image,
  because the compiler cannot know the init's outcome.
- **No backend registration.** `nros_app_register_backends()` is written
  from an empty list through `nros_cc_flags::backends::source`.

`just ci l3` builds the image right after the Rust one, in the CI profile,
and gates it with `--tier heap-free`. It is in `fixture_source_coverage`'s
bins allowlist for the Rust image's reason (issue 1493): it is a symbol-table
witness with no fixture coordinate.

## Measured

| build | symbols read | gate |
| --- | --- | --- |
| `nros-relwithdebinfo` (the L3 profile) | 1,823 | OK |
| debug | 2,615 | OK |

- **The program's calls are in the optimised image:**
  `nros_cpp_node_create`, `nros_cpp_node_{declare,set,get}_param_integer`,
  `nros_cpp_timer_create` and `nros_cpp_spin_once` are all present.
- **It boots on QEMU:** it prints `cannot select an RMW backend — no RMW
  backend is registered`, then
  `HEAP-FREE-CPP-POC: init refused (no RMW linked) -- link property holds`,
  and exits 0.
- **Negative control:** the same leaf built with
  `nros-cpp/alloc,nros-cpp/platform-mps2-an385` FAILS the gate with 9
  allocation symbols. Among them are `__rust_alloc`, the `__rdl_*` handler,
  `PlatformGlobalAllocator::realloc` and `zpico_alloc::FreeListHeap::free`.

## Not covered

A C++ image WITH a transport is still not heap-free. That is the same limit
the Rust image states: zenoh-pico (`z_malloc`), XRCE (its `malloc` wrapper)
and Cyclone (`ddsrt_malloc`) all allocate in vendored C. Such an image can
reach the `unified` tier at best, with allocation only through the platform
backend. This image does not test that tier.
