---
id: 195
title: "threadx-riscv64 cyclone two-qemu pubsub: boots, 0 messages delivered"
status: resolved
type: bug
area: threadx-riscv64
related: [phase-287]
---

## Summary

`test_threadx_riscv64_cyclonedds_two_qemu_pubsub` fails deterministically
(3/3 nextest retries, near-solo run) with the listener receiving 0 samples:

```
nros-tests::threadx_riscv64_qemu test_threadx_riscv64_cyclonedds_two_qemu_pubsub
  Listener: expected at least N received messages, got 0
```

Observed 2026-07-14 during the phase-287 W7 full-matrix sweep on freshly
rebuilt fixtures (`just build-test-fixtures` green, threadx_riscv64 stage OK).
Both QEMU instances boot (the failure is the delivery assert, not a readiness
gate).

## Leads

- Cyclone two-instance pairs are the classic identical-identity trap: baked
  IP/MAC → identical entropy → SPDP sees the peer as itself (the zenoh ZID
  analogue), or a shared domain with another lane. Check the pair's baked
  identities + domain first (archived 0161 / the cyclone fixture-pair 50–58
  domain convention).
- The riscv64 lane has prior art for silently-broken rebuilds: archived 0131
  (NULL c_app_main on rebuild), 0138 (`--allow-multiple-definition`), and the
  sizes-header race recurrence (agent-memory note) — verify the images embed
  a sane app before chasing the network.

## Repro

```sh
just build-test-fixtures
cargo nextest run -E 'test(test_threadx_riscv64_cyclonedds_two_qemu_pubsub)'
```

## RESOLVED — 2026-07-15: descriptors never registered (.init_array never ran) + a buildability stack

**Delivery root cause:** the Cyclone message descriptors register via
`__attribute__((constructor))` TUs (`register_std_msgs_*`), but the riscv64
flat bare-metal image has no crt0/`__libc_init_array` and nothing walked
`.init_array` — the section sat in the ELF as an ORPHAN (present, no bound
symbols, never executed). Every reader/writer create then failed
(`nros_executor_register_subscription -> -1`, visible in the listener boot
log) → 0 delivery while both nodes "boot" fine. The CLAUDE.md "no POSIX-style
ctor sections on RTOS" pitfall, C-descriptor flavour. Fix:
- `config/link.lds`: explicit `.init_array` output section with
  `__init_array_start/__init_array_end` bounds (KEEP + init-priority sort),
  plus `PROVIDE(end = .)` (newlib heap-base symbol, see below).
- `board_threadx_qemu_riscv64.c` (`nros_board_init_eth`): walk the ctor table
  once per boot, before NetX init / the app thread.

**Buildability stack (the lane could not even be rebuilt locally):**
1. 287-W6 (`ee4ae142e`) dropped the leaves' `set(NANO_ROS_PLATFORM threadx)`
   (ament shape), and `just/threadx-riscv64.just` — unlike threadx-linux's
   recipe — never passed `-DNANO_ROS_PLATFORM`. The root default `posix`
   configured `nros-platform-posix` under the riscv cross toolchain →
   `FindThreads` fatal at configure, all fixtures dead. Added
   `-DNANO_ROS_PLATFORM=threadx_riscv64` to `base_defs` + the cmake helper.
2. `nros-rmw-cyclonedds/CMakeLists.txt` gated `NROS_PLATFORM_THREADX` on
   `STREQUAL "threadx"`, missing the `threadx_riscv64` variant → the platform
   branch fell to the hosted `#include <chrono>` path → fatal on bare-metal.
   Now `MATCHES "^threadx"` (the root CMakeLists' own mapping convention).
3. The Debian `gcc-riscv64-unknown-elf`/picolibc toolchain ships NO libstdc++,
   but the cyclone link set includes `stdc++` (the RMW wrapper is C++). The
   toolchain file now resolves a rv64gc/lp64d `libstdc++.a` — the active
   compiler's own multilib first, else the nros SDK `riscv-none-elf-gcc`
   (`rv64imafdc_zicsr/lp64d`) — and appends `-lnosys` (STANDARD_LIBRARIES,
   end-of-line) for the newlib reent syscalls the SDK libstdc++ references;
   `end` comes from the lds. The stubs never run (picolibc + the ThreadX byte
   pool own allocation; Cyclone transients go through ddsrt_malloc).

Verified: `test_threadx_riscv64_cyclonedds_two_qemu_pubsub` PASS 2/2 (~6 s,
was a 94 s zero-delivery timeout ×3 retries) on freshly rebuilt fixtures.
