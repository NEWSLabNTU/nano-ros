---
id: 2115
title: "nros-cpp did not compile without `alloc`: `nros`'s capability helpers went dead, and nros-cpp reached `alloc`-only nros-node API"
status: resolved
type: bug
area: [cpp, build, memory]
severity: medium
found: 2026-10-27
related: [1812, 1529, 1177, 0586, 0816]
resolved: 2026-10-27
resolved_in: "#2784"
---

## What

A heap-free C++ image builds nros-cpp with no `alloc` and the parameter
store carved from the executor backing (`param-store`, `NROS_PARAM_STORE=1`,
issue 1812). That build failed:

```
$ NROS_PARAM_STORE=1 cargo check -p nros-cpp --features param-store,platform-posix
error: unreachable pattern
    --> packages/api/nros/src/node_runtime.rs
error: function `capability_reason` is never used
error: function `capability_transport_reason` is never used
```

The first failure hid four more, in nros-cpp itself:

- `T::BackendDynamic(_)` in `transport_error_to_cpp_ret` and
  `transport_error_class`. The variant only exists with `nros-rmw/alloc`.
- `nros_cpp_apply_ros_args` used `apply_ros_args_with_params`,
  `ApplyParamsError`, `nros::env::read_file` and `alloc::string::String`
  under `param-store` alone. All four are `alloc`-only.
- `nros_cpp_level_open` called `BandWaiter::platform()`, which `Box::leak`s
  a platform wake object and only exists with `alloc + rmw-cffi`.

On an aarch64 host, nros-c also failed `clippy::unnecessary_cast` at its
default features. `until_nul` cast `c_char` to `u8`, and `c_char` is `u8` on
ARM.

## Cause

Every nros-cpp row in the compile-smoke lane carries `alloc`, which is in
`C_API_SHIPPED_FEATURES`. No lane compiled nros-cpp without it, so each of
these landed separately and nothing reported them.

The `BackendDynamic` arm was un-gated on purpose. A comment above it said no
buildable configuration lacked the variant: "If a genuinely alloc-free build is
ever made to work, this line fails to compile". That build works now. The
first mapper had already gained the fallback that makes a gated arm safe under
feature unification: a `#[cfg(not(feature = "alloc"))]` wildcard.

## Fix

- `nros`: `capability_reason` and `capability_transport_reason` are gated on
  `alloc` as well. Their only callers are `ExecutorNodeRuntime`'s capability
  hooks, and that runtime is `alloc`-only. With `nros/alloc`,
  `nros-rmw/alloc` is on, so the transport mapper's wildcard is always
  reachable where it is compiled.
- nros-cpp, both `TransportError` mappers: the `BackendDynamic` arm is
  `#[cfg(feature = "alloc")]`. A `#[cfg(not(feature = "alloc"))]`
  `#[allow(unreachable_patterns)]` wildcard covers the variant when another
  crate turns on `nros-rmw/alloc` alone. This is the shape issue 0586's
  `check-cpp-ffi-error-mapping` already accepts.
- nros-cpp `nros_cpp_apply_ros_args`: the `-p` / `--params-file` branch needs
  `param-store` AND `alloc`, because nros-node holds argv overrides on the
  heap. A heap-free image takes the remap-only parse, which refuses `-p` by
  name, as an image without a store always has.
- nros-cpp `nros_cpp_level_open`: without `alloc` a level has no waiter and
  polls. The executor does the same in that build. A platform wake object is
  sized at run time (`nros_platform_wake_size`), so it is allocated, and
  `node_wake` is `alloc`-only.
- nros-c `until_nul`: `u8::from_ne_bytes(c.to_ne_bytes())`, which is the same
  no-op on both `c_char` signs and is never linted.

## Gate

`just check compile-smoke` gains a row: nros-cpp with no `alloc`,
`panic-halt,rmw-cffi,platform-posix,param-store`, `NROS_PARAM_STORE=1`, in its
own target dir. The row was verified against the old sources: it fails there
(exit 101) and passes with the fix.

## Measured

- **thumbv7m staticlib, no `alloc`:** `libnros_cpp.a` for `thumbv7m-none-eabi`
  (`panic-halt,rmw-cffi,param-store`, `NROS_PARAM_STORE=1`) has no undefined
  reference to `__rust_alloc*`, `__rg_*`, `__rdl_*`, `malloc`, `calloc`,
  `realloc`, `free`, `strdup` or `nros_platform_alloc` / `_dealloc` /
  `_realloc` in any member.
- **The same, on the host with `platform-posix`:** the only references are the
  POSIX C port's own (`platform.o`, `timer.o`, `net.o`), plus
  `PlatformGlobalAllocator::alloc`. That impl is compiled but never registered
  (no `__rg_alloc` is defined), so nothing in nros-cpp allocates.
- **Clippy (`-D warnings`) is clean on these shapes:**
  - the default features;
  - `rmw-cffi,platform-posix` with no `alloc`;
  - the heap-free shape above;
  - the shipped shape plus `param-services`;
  - the unification case: nros-cpp with no `alloc` while `nros-rmw/alloc`
    is on.

## Not covered

A LINKED heap-free C++ image is not checked. The heap-free link gate (`just
ci l3`, `check-no-alloc-image.py`) checks the Rust `heap-free-poc-mps2` image
only. Every C++ RMW backend in the tree allocates in its vendored C code: for
example, zenoh-pico's `z_malloc` sites, which that script's own header
discusses. So a heap-free C++ image would need a backend that does not
allocate, and none exists yet.
