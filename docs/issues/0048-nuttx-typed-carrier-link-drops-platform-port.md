---
id: 48
title: NuttX Test/e2e — typed-carrier link drops the platform port (undefined nros_platform_*)
status: open
type: bug
area: boards
related: [phase-240, phase-243]
---

The platform-ci **nuttx** cell's Test/e2e step fails at **link** (Build ✓): the
entire `nros_platform_*` ABI is undefined when linking the nuttx zenoh fixture
(the talker ELF). Surfaced by every `run_e2e` dispatch since the 240.6 NuttX-talker
migration.

```
error: linking with `arm-none-eabi-gcc` failed: exit status: 1
  nros_rmw_zenoh…ZenohSubscriber::new: undefined reference to `nros_platform_time_now_ms'
  nros_rmw_zenoh…try_recv_sequence / has_data: undefined reference to `nros_platform_time_now_ms'
  nros_rmw_cffi…assert_publisher_liveliness_trampoline: undefined reference to `nros_platform_time_now_ms'
  platform_aliases.c:(z_malloc):   undefined reference to `nros_platform_alloc'
  platform_aliases.c:(z_free):     undefined reference to `nros_platform_dealloc'
  platform_aliases.c:(z_sleep_ms): undefined reference to `nros_platform_sleep_ms'
  platform_aliases.c:(z_random_fill): undefined reference to `nros_platform_random_fill'
  platform_aliases.c:(_z_task_init/join/detach/exit): undefined reference to `nros_platform_task_*'
  … (the whole nros_platform_* surface)
```

## Root cause (diagnosis)

The `nros_platform_*` symbols are the canonical platform ABI implemented by the
linked platform **port** (`nros-platform-nuttx` + the `nros-platform-cffi` export
macros that emit the `#[unsafe(no_mangle)] extern "C"` definitions). The nuttx
zenoh fixture (RMW + zpico `platform_aliases.c`) references them; the link can't
find them ⇒ **the port is not in the link graph**.

`240.6` ("migrate NuttX talker to typed component") rewrote the talker CMakeLists
to a **TYPED carrier** — dropped `nros_find_interfaces`, switched to
`nano_ros_node_register(... LANGUAGE C TYPED ...)`. The pre-240.6 shape pinned the
board rlib (and thereby the port's `nros_platform_*`) into the image; the typed
carrier's generated entry no longer references the board `run()`, so `--gc-sections`
/ archive single-pass drops the port (the CLAUDE.md pitfall: *"referencing the
board `run()` is REQUIRED — it pins the board rlib so `--gc-sections` doesn't drop
the platform `nros_platform_*` symbols"*).

**Pre-existing, NOT phase-243.** nuttx failed identically on clean `main` before
243 landed, and 243 doesn't touch the nuttx port linkage (243 changed the platform
*header* + nros-c's *internal* clock/atomics usage; the undefined symbols here are
the port's `#[no_mangle]` exports, header-independent). The other 5 platform-ci
cells are green post-243.

## Fix direction

Make the typed-carrier nuttx link **pin the platform port** again — options:
- have the generated TYPED-carrier entry reference the board `run()` / a board
  anchor symbol (the pre-240.6 mechanism), or
- a `#[used]` force-link anchor for the port crate, or
- whole-archive the platform-port archive in the nuttx link (like the RMW
  whole-archive group).

Owner: NuttX board / 240.x entry-codegen. (The cross-platform deterministic-link
manifest in RFC-0042 D3 / phase-241 would prevent this class structurally.)
