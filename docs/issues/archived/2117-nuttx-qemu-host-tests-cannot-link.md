---
id: 2117
title: "nros-board-nuttx-qemu's unit tests cannot link on the host, so no lane runs them"
status: resolved
type: bug
area: [boards, nuttx, testing]
severity: low
found: 2026-10-27
related: [issue-1472, issue-2047, issue-2048]
resolved: 2026-10-28
---

## What

`cargo test --lib` in `packages/boards/nros-board-nuttx-qemu` failed to link:

```
error: entry symbol `main` declared multiple times
```

`scripts/run-excluded-crate-tests.sh` (the `excluded-crate-tests` gate, issue
1472) recorded this as a structural exemption — "cannot link for the host" — so
the crate's tests ran in no lane.

## Why

Two layers, the second hidden by the first:

1. `mod entry` was unconditional. It exports NuttX's init entry `nsh_main` and
   declares the image's C `main` in an `extern "C"` block. On the host the test
   harness defines its own `main`, and the two collide. The module only means
   anything on NuttX.
2. With that gated, the test binary still failed at `nros_platform_alloc` /
   `nros_platform_dealloc`. The board's global allocator routes through
   `nros-platform-cffi`, whose symbols the NuttX C port supplies on target and
   nothing supplied on the host.

And behind both, a test that had never run failed:
`the_default_is_the_shared_base_config` still asserted the pre-phase-514
`192.0.3.10` / `192.0.3.1`. The default has been the kernel's own plan,
generated from the defconfig's `CONFIG_NETINIT_*` (10.0.2.15/24, gateway
10.0.2.2), since phase-514 W4.

## Resolution

- `mod entry` is `#[cfg(target_os = "nuttx")]`.
- A dev-dependency on `nros-platform-cffi` with `posix-c-port` supplies the
  platform symbols to test builds only. NuttX is a POSIX kernel, so the POSIX
  reference port is the honest stand-in. The lock gains two dependency edges,
  via `just lock-update`.
- The stale test asserts the kernel's plan and is renamed
  `the_default_is_the_kernels_eth0_plan`.
- The exemption is removed from `run-excluded-crate-tests.sh`, so the gate runs
  the crate: `nros-board-nuttx-qemu` 3 passed, and the lane is green over all 4
  crates (51 tests).
