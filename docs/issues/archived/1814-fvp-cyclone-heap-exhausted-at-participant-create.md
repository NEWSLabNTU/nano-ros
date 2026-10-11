---
id: 1814
title: "The FVP Cyclone image aborts at `dds_create_participant`: Cyclone's 1 MiB receive buffer does not fit the 1 MiB nros heap"
status: resolved
type: bug
area: [rmw, zephyr, memory]
severity: medium
found: 2026-10-11
resolved: 2026-10-11
related: [1288, 1633, 0367]
---

# What happened

`just zephyr verify-fvp-runtime` (`fvp_runtime_ws::fvp_ws_entry_two_tier_publishes`)
failed on `main`. The image boots, starts Cyclone, and dies in `session_create`
before any tick:

```text
*** Booting Zephyr OS build v3.7.0 ***
<inf> cyclonedds: session_create: calling dds_create_participant
nros: HEAP EXHAUSTED (TOO SMALL): request 1048624 bytes, arena 1049088 bytes,
      free 1013728 bytes, largest free block 1013680 bytes, caller 0x463e8
abort()
>>> ZEPHYR FATAL ERROR 4: Kernel panic on CPU 1
```

`0x463e8` is `ddsrt_malloc` (`third-party/dds/cyclonedds/src/ddsrt/src/heap/nros/heap.c`).
It failed identically on the hand-written `realtime-cpp/src/fvp_entry` image and on
the generated one from issue 1288's migration (PR #1972). The migration did not
cause it.

# Root cause (measured)

**The request is Cyclone's receive buffer at its desktop default.**
`src/core/ddsi/defconfig.c` sets `cfg->rbuf_size = 1048576`, and
`nn_rbuf_alloc` (`q_radmin.c:438`) asks for `sizeof (struct nn_rbuf) + rbuf_size`.
That is 1,048,576 + 48 = **1,048,624**, the exact request in the log.

**Why the FVP gets the default and native_sim does not.** `nros-rmw-cyclonedds`
holds an embedded baseline (`cyclone_config.hpp`: `Sizing/ReceiveBufferSize` 64 KiB,
`ReceiveBufferChunkSize` 16 KiB, one receive thread, sized thread stacks,
`Tracing/OutputFile` stderr). Two guards decided where it applied, and both named
the BOARD where they meant the PLATFORM:

- `cyclone_config.hpp`: compile `kEmbeddedCycloneConfig` only for
  `NROS_PLATFORM_FREERTOS || NROS_PLATFORM_THREADX || CONFIG_BOARD_NATIVE_SIM`.
- `session.cpp`: `NROS_CYC_COMPOSE_DOMAIN`, the same list spelled a second time,
  which decides whether `session_create` calls `dds_create_domain` at all.

So on any Zephyr board other than native_sim, no domain was composed and Cyclone
ran on its own defaults. That also meant `CONFIG_NROS_CYCLONE_CONFIG_XML`
(issue 0367's compile-time override) reached nothing on those boards. Measured
on the FVP image's `.config`: `CONFIG_NROS_ZEPHYR_HEAP_SIZE=1048576`,
`CONFIG_NROS_CYCLONE_CONFIG_XML=""`.

**Why it used to pass.** Before issue 1633, ddsrt allocated from libc `malloc`.
The FVP image's newlib arena was not capped at 1 MiB, so a 1 MiB rbuf fit. Since
1633 ddsrt allocates from the nros heap (`CONFIG_NROS_ZEPHYR_HEAP_SIZE`, 1 MiB with
Cyclone). The issue's working hypothesis (a native_sim-only baseline combined with
1633's heap funnel) is confirmed by the fix below, which changes only the guards
and turns the lane green.

# Fix

There is now one predicate, `NROS_RMW_CYCLONEDDS_EMBEDDED_TARGET`
(`FREERTOS || THREADX || __ZEPHYR__`), in `cyclone_config.hpp`. It decides both
whether the baseline is compiled and whether `session.cpp` composes the domain.
That covers every Zephyr board, not only the FVP. The genuinely native_sim parts
of the baseline (multicast off for NSOS, the `127.0.0.1` peer and the widened
participant-index scan) stay gated on `CONFIG_BOARD_NATIVE_SIM`. On other Zephyr
boards `AllowMulticast` stays Cyclone's default, as it was before this change.

Sweep (the class is "a Cyclone policy keyed on the native_sim board"):

```sh
git grep -n 'CONFIG_BOARD_NATIVE_SIM' -- packages/rmw/cyclonedds
```

After the fix, the hits are only the two native_sim-specific blocks inside the
baseline string. The other `CONFIG_BOARD_NATIVE_SIM` users in the tree
(`zephyr/CMakeLists.txt`, `nros_cargo_build.cmake`, `net_wait.c`) are NSOS or
host-toolchain facts, not Cyclone sizing.

# Measured before / after

FVP AEMv8-R (`fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp`), `realtime-cpp`, Cyclone,
same tree with and without the change, incremental rebuild (session.cpp + link):

| | before | after |
| --- | --- | --- |
| `verify-fvp-runtime` | FAIL x2 (180 s timeout; HEAP EXHAUSTED 1,048,624 B at `dds_create_participant`) | **PASS** (3.6 s to the first `[telem] tick=`) |
| ELF text | 1,298,006 | 1,299,076 (+1,070: the baseline string) |
| ELF bss | 5,617,563 | 5,621,659 (+4,096: `session.cpp`'s static compose buffer) |
| data | 29,492 | 29,492 |

native_sim: the macro previously named the board and now names the platform, so
native_sim gets the same baseline it got before. Checked by a runtime test:
`zephyr::example_e2e::case_12_cyclonedds_cpp_pubsub_e2e` (cpp talker + listener,
native_sim/native/64, Cyclone, rebuilt on this tree) **PASS**, with a heap peak of
254,480 of 1,049,088 B.

Note: `/home` was full (0 B free) during this work, so the FVP builds ran with
`NROS_BUILD_ROOT` / `CARGO_TARGET_DIR` on another volume. These are the documented
"move the cache" knobs and do not change the image.
