---
id: 2397
title: "PX4 fmu-v6x's NuttX: the platform-type size mismatch suspected of phase-547's `mm_free` fault is measured and ruled out; every type is smaller than our default config, and nothing nano-ros sizes for it comes from that config"
status: open
type: bug
severity: high
area: [nuttx, px4, memory]
related: [phase-547, 0570, 1907, 0135, 0460, 1354]
found: 2026-11-10
---

**Home: [phase-547](../roadmap/phase-547-px4-pixhawk-emulator-spike.md)**, W3.
The fault is the last row of W3's findings table: the nano-ros uORB module on
the emulated Pixhawk 6X prints `nros: session open (rmw=uorb)`, then takes a
MemManage fault in `mm_free` (task `netinit`, freeing `0xffffff90`).

## What was suspected

The leading suspect was a `repr(C)` mirror mismatch. The idea: the
cross-built Rust side sizes NuttX `pthread` / `sem` storage from a size probe
run over a default NuttX config. PX4's config might make those types larger,
so NuttX would write past storage nano-ros reserved.

## What was measured

All of it is static. No emulator is needed.

1. **PX4's NuttX config.** PX4-Autopilot at the commit our `third-party/px4/PX4-Autopilot`
   submodule pins (tag v1.17.0)
   was cloned outside any checkout, under `/mnt/wd/nt/p547`, with its two
   NuttX submodules at the commits that PX4 commit pins (`nuttx` on
   PX4's `px4_firmware_nuttx-10.3.0+` branch, and `apps`). Its config
   was generated with PX4's own build:
   ```
   python3 -m venv venv && . venv/bin/activate   # PYTHONPATH cleared
   pip install -r PX4-Autopilot/Tools/setup/requirements.txt
   cmake -S PX4-Autopilot -B build-v6x -G Ninja -DCONFIG=px4_fmu-v6x_default
   ninja -C build-v6x nuttx_context   # writes nuttx/.config + include/nuttx/config.h
   ```
2. **Our default config.** This is `third-party/nuttx/nuttx` at the commit main pins,
   configured as `qemu-armv7a`, the tree `nros locate nuttx-kernel` falls back
   to.
3. **A sizeof/alignof probe.** It covers every NuttX type that nano-ros stores,
   passes or mirrors, plus the two composites the POSIX port builds on top of
   them. It was compiled with Arm GNU 13.2.1 from the SDK store:
   `arm-none-eabi-gcc -mcpu=cortex-m7 -mthumb -mfloat-abi=hard -mfpu=fpv5-d16
   -nostdinc -isystem <tree>/include -isystem <gcc>/include -D__NuttX__`.
   Each fact is read back as a symbol size with `arm-none-eabi-nm -S`. The
   default tree was also compiled with `-mcpu=cortex-a7` (its own board), and
   the result was identical.
4. **nano-ros's own size probe.** `nros-cpp` was cross-built the way the spike
   built it:
   ```
   cargo +nightly-2026-04-11 build -p nros-cpp --target thumbv7em-nuttx-eabihf \
     -Zbuild-std=core,alloc,panic_abort --no-default-features \
     --features alloc,log,panic-platform,rmw-cffi,platform-nuttx --release
   ```
   It was built twice, into separate target dirs: once with `NUTTX_DIR` and
   `CFLAGS_thumbv7em_nuttx_eabihf` naming PX4's tree, once naming the default
   tree.

### NuttX types (size/align in bytes)

| type | PX4 6X | default (qemu-armv7a) | nano-ros storage for it | under-reserved on PX4? |
|---|---|---|---|---|
| `pthread_mutex_t` | 20/4 | 32/4 | runtime `sizeof` from the C port; bound `NROS_PLATFORM_MUTEX_STORAGE_SIZE` 256 | no |
| `pthread_cond_t` | 12/4 | 28/4 | runtime `sizeof`; bound 256 | no |
| `pthread_t` | 4/4 | 4/4 | runtime `sizeof`; bound 512 | no |
| `pthread_attr_t` | 12/4 | 56/8 | caller stack, C port only | no |
| `pthread_condattr_t` | 1/1 | 8/4 | C port local | no |
| `pthread_mutexattr_t` | 1/1 | 1/1 | C port local | no |
| `pthread_rwlock_t` | 44/4 | 72/4 | not used | n/a |
| `pthread_once_t` | 1/1 | 36/4 | C port static | no |
| `sem_t` | 8/4 | 20/4 | inside `nros_wake_t` | no |
| `nros_wake_t` (port composite) | 16/4 | 28/4 | runtime `nros_platform_wake_storage_size()`, heap | no |
| `nros_posix_timer_t` (port composite) | 24/4 | 24/4 | C port heap | no |
| `time_t` | 4/4 | 8/8 | C port local | no |
| `clock_t` | 4/4 | 8/8 | none | n/a |
| `clockid_t` | 1/1 | 4/4 | passed by value | no |
| `struct timespec` | 8/4 | 16/8 | C port local | no |
| `struct timeval` | 8/4 | 16/8 | C port local | no |
| `struct itimerspec` | 16/4 | 32/8 | C port local (timer) | no |
| `struct sigevent` | 8/4 | 16/4 | C port local (timer) | no |
| `struct sched_param` | 4/4 | 48/8 | C port local | no |
| `struct stat` | 60/4 | 88/8 | none | n/a |
| `struct sockaddr_storage` | 128/2 | 128/8 | C port (net) | no |
| `timer_t`, `pthread_key_t`, `socklen_t`, `off_t`, `union sigval` | 4/4 | 4/4 | same | no |
| `struct pollfd` | 24/4 | 24/4 | same | no |
| `fd_set` | 32/4 | 32/4 | same | no |
| `struct addrinfo` | 32/4 | 32/4 | same | no |
| `struct sockaddr`, `_in`, `_in6` | 16/2, 16/4, 28/4 | same | same | no |

Every PX4 size is **smaller than or equal to** the default config's. The
direction the hypothesis needs is NuttX writing more than nano-ros reserved,
and it occurs for no type.

The differences come from two sources. The first is PX4's NuttX fork itself
(branch `px4_firmware_nuttx-10.3.0+`), which carries the older layouts: for
example `struct sem_s` has no `waitlist` and no `val` union, and
`pthread_once_t` is a `bool`. The second is these Kconfig symbols in PX4's
config:

- `CONFIG_SYSTEM_TIME64=n`: 32-bit `time_t`, which halves `timespec`,
  `timeval`, `itimerspec` and `stat`.
- `CONFIG_SCHED_SPORADIC=n`: `pthread_attr_t` and `sched_param` drop their two
  `timespec`s.
- `CONFIG_PTHREAD_MUTEX_ROBUST=y` with `PTHREAD_MUTEX_BOTH=n`.
- `CONFIG_PTHREAD_MUTEX_DEFAULT_PRIO_NONE=y`.
- `CONFIG_SEM_PREALLOCHOLDERS=32`.
- `CONFIG_DISABLE_POSIX_TIMERS=y`.
- `CONFIG_DISABLE_MQUEUE=y`.

`CONFIG_PRIORITY_INHERITANCE=y` is set in both configs.

### What nano-ros actually sizes from a config: nothing

- **The Rust side holds no NuttX type.** Mutex, condvar, task and wake storage
  is sized at runtime from `nros_platform_*_storage_size()` (`NodeWake::new`,
  `nros_platform_api::task`). Those functions return the C port's own
  `sizeof`. The `NROS_PLATFORM_*_STORAGE_SIZE` compile-time bounds (256 / 512)
  are checked against the real types by `_Static_assert` in
  `nros-platform-posix/src/platform.c`. That file compiles clean against PX4's
  headers, so the asserts hold. `timer.c` fails there on `SIGEV_THREAD`, as W3
  already recorded.
- **The hand mirror is not linked.** The Rust `libc` crate's NuttX mirror
  (`third-party/nuttx/libc`: `time_t = i64`, mutex 56 B, `sem_t` 44 B) is
  hand-sized for the largest layout (issues 0570/1907). It is not in this
  image's graph: `cargo tree -i libc` for the feature set above prints nothing.
  Its `time_t = i64` disagrees with PX4's 32-bit `time_t`, but only a `std`
  NuttX image would reach it.
- **The cargo graph compiles no C platform port for this image.**
  `nros-platform-cffi`'s build script prints `cargo:abi_provider=none` under
  both trees. The C port in the image is the PX4 module's own build, against
  PX4's headers.
- **The baked sizes header does not depend on the NuttX tree.** The
  `nros_config_generated.h` and `nros_cpp_config_generated.h` from the two
  builds are byte-identical in every `#define`. Examples: `NROS_EXECUTOR_SIZE`
  89424, `NROS_CPP_EXECUTOR_STORAGE_SIZE` 90296, `NROS_PUBLISHER_SIZE` 604,
  `NROS_SUBSCRIBER_SIZE` 644, `NROS_SESSION_SIZE` 528,
  `NROS_EXECUTOR_MAIN_STACK_MIN` 3808, variant
  `alloc_log_panic_platform_platform_nuttx_rmw_cffi`. These are `size_of` over
  nano-ros's own Rust types for the triple. No build script in the graph reads
  `config.h`.
- **The uORB mirror matches.** `nros-rmw-uorb`'s hand mirror of
  `struct orb_metadata` (`uorb_abi.hpp`) matches PX4 1.17's
  `platforms/common/uORB/uORB.h` field for field. A PX4 build uses the upstream
  header in any case (`NROS_RMW_UORB_USE_PX4_HEADER`). `take` refuses a buffer
  smaller than `o_size` before it calls `orb_copy`.

**Verdict: no mismatch. This suspect is ruled out.** The fault remains
unexplained, so the issue stays open.

## Next suspects

Each needs the emulator tree or the spike's exact build inputs, which this
measurement did not have.

1. **The two halves of one struct come from two builds** (the 0135/0460/1354
   class). Does the sizes header on the PX4 module's include path come from
   the same cargo build, with the same feature set, as the `libnros_cpp.a` it
   links? Compare the module's `NROS_CPP_CONFIG_VARIANT` with the archive's.
   The spike made several scratch builds (`opt-level=z`, fat LTO), and a stale
   header from an earlier feature set under-reserves every C++ handle.
2. **Rust and C free each other's blocks.** Either a `delete` / `free` of a
   block Rust allocated through the `over_align` path (`align > 8`, for
   example the executor's `repr(align(64))` `WakeCtx`), or a Rust `dealloc` of
   a block `malloc` returned. Either one hands `mm_free` a pointer it never
   returned. `0xffffff90` is `-0x70`: a pointer computed from NULL or from
   zeroed memory, not a stray overrun.
3. **The `.nros_boot_config` scratch fix.** W3's UsageFault came from the
   orphan section shifting initialised data by 0x10c. If the scratch
   `KEEP(*(.nros_boot_config))` placement still overlaps `.data` or `.bss`
   initialisation, the heap's own globals start corrupted.
4. **An overrun by another task.** `netinit` does the free, and it is NuttX's
   network bring-up task, not nano-ros. A heap-guarded build would name the
   writer: `CONFIG_MM_FILL_ALLOCATIONS`, a `mm_checkcorruption()` call after
   `session open`, or `CONFIG_DEBUG_MM`.
5. **The work-queue stack.** Already measured not to be the cause at 24,000 B.
   But the baked `NROS_EXECUTOR_MAIN_STACK_MIN` is 3808 B, above `rate_ctrl`'s
   3,150 B default, so a stock build overflows it regardless.

## Build tree

`/mnt/wd/nt/p547` (2.5 GB) holds the PX4 clone, the generated config, the
probe (`probe/sizes.c`, `probe/run.sh`, `probe/table.py`, `probe/cross-cpp.sh`),
both cargo target dirs and the logs. As the spike's ground rule requires, it
sits outside every checkout and is deleted when phase-547 closes.
