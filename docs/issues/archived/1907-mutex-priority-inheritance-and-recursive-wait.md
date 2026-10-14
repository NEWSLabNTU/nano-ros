---
id: 1907
title: "POSIX and NuttX mutexes have no priority inheritance, RFC-0002 says
  NuttX does, and a condvar wait under a recursively held mutex deadlocks on
  three ports"
status: resolved
type: bug
area: [platform]
severity: medium
found: 2026-10-15
resolved: 2026-10-15
related: [issue-1196, issue-1906, rfc-0002]
---

**Home: none** — found by issue 1196's design study; no active phase owns the
platform lock contract.

## 1. No priority inheritance on POSIX or NuttX

`nros-platform-posix/src/platform.c`'s `nros_platform_mutex_init` calls
`pthread_mutex_init(m, NULL)`. The default protocol is `PTHREAD_PRIO_NONE`,
on glibc and on NuttX alike. The recursive mutex sets
`PTHREAD_MUTEX_RECURSIVE` and nothing more.

NuttX compiles the same file. It gives a default mutex priority inheritance
only when BOTH `CONFIG_PRIORITY_INHERITANCE` and
`CONFIG_PTHREAD_MUTEX_DEFAULT_PRIO_INHERIT` are set
(`libs/libc/pthread/pthread_mutexattr_init.c`). None of the three defconfigs
under `packages/boards/nros-board-nuttx-qemu/nuttx-config/` sets either one
(checked: no `PRIORITY_INHERITANCE` in any of them).

RFC-0002's capability table (`docs/design/0002-rt-execution-model.md`, the
"PI mutex" row) marks NuttX "✅ default w/ `CONFIG_PRIORITY_INHERITANCE`".
That is wrong twice: it is not the default, and it is not configured.

Issue 1196 exempted mutex waits from its "no unbounded wait" rule on the
grounds that every port has priority inheritance. Zephyr, FreeRTOS and ThreadX
do. POSIX and NuttX do not, so on those two a priority inversion through a
nano-ros mutex is unbounded.

**Fix direction.** Set `PTHREAD_PRIO_INHERIT` on both mutex kinds in the POSIX
port (Linux supports it on every glibc we target), and enable
`CONFIG_PRIORITY_INHERITANCE` in the NuttX defconfigs. Then correct the
RFC-0002 row.

## 2. A condvar wait under a recursively held mutex deadlocks

On Zephyr, FreeRTOS and ThreadX, a condvar wait releases the mutex ONCE:

- Zephyr: `k_condvar_wait` calls `k_mutex_unlock` once
  (`zephyr/kernel/condvar.c:118`).
- FreeRTOS and ThreadX: our ports call `nros_platform_mutex_unlock(m)` once,
  and both build every mutex as a recursive one.

If the caller holds `m` at depth 2 or more, the mutex stays held across the
"wait". Every signaller then blocks on `m`, and that is a deadlock. A timed
wait does not rescue it, because the waiter never released the lock. NuttX is
correct: `pthread_cond_clockwait` uses `pthread_mutex_breaklock`, which
releases every level and restores it after.

The FreeRTOS port's own comment says zenoh-pico holds `_z_mutex_t`
recursively on its read-task paths. No measured deadlock is known.

**Fix direction.** State the contract in `nros/platform.h`: a condvar wait
requires `m` held exactly once. Then either assert it in debug builds (the
kernels expose the owner's lock count), or release and restore every level,
as NuttX does.

## Resolution (2026-10-15)

Both halves are fixed. NuttX itself had the answer, already in the version we
pin; nano-ros had just not turned it on.

### What NuttX does, and why it fit

Read in the vendored kernel:

- `CONFIG_PRIORITY_INHERITANCE` (default `n`) turns inheritance on, and the
  choice "default pthread mutex protocol" then defaults to
  `PTHREAD_MUTEX_DEFAULT_PRIO_INHERIT`. NuttX's help text calls that "POSIX
  non-standard but a reasonable choice for most real-time systems".
- The known hazard is a semaphore used for SIGNALLING: under inheritance its
  waker becomes a permanent "holder" that can be boosted later (NuttX guide
  "Signaling Semaphores and Priority Inheritance"). That applied when every
  semaphore inherited. In our version `nxsem_init` sets `flags = 0`
  (`SEM_PRIO_NONE`), and only mutexes are created with the inherit flag. So our
  wake semaphore (`posix platform.c`), zenoh's spin semaphore (`zpico.c`) and
  NuttX's own condvar semaphore are unaffected, and none needs
  `sem_setprotocol`.
- Its `pthread_cond_wait` releases every level of a recursive mutex
  (`pthread_mutex_breaklock` / `pthread_mutex_restorelock`) and wakes waiters
  in priority order.

On Linux, glibc supports `PTHREAD_PRIO_INHERIT`, recursive mutexes included,
but its condvars wake in arrival order rather than by priority (glibc bug
11588). librtpi is the RT-Linux answer to that, and it refuses recursive
mutexes, which `mutex_rec` needs. The Linux host is a development target, so
that limit is recorded in RFC-0002 rather than worked around.

### Changes

1. `nros-platform-posix`: both mutex kinds set `PTHREAD_PRIO_INHERIT`
   (`nros_posix_pi_mutex_init`). NuttX does not define
   `_POSIX_THREAD_PRIO_INHERIT`, and `pthread_mutexattr_setprotocol` returns
   `ENOTSUP` at runtime without the kernel option, so the port refuses at
   compile time (`#error`) on a NuttX kernel without
   `CONFIG_PRIORITY_INHERITANCE`.
2. All three NuttX defconfigs (`arm`, `arm-smp`, `riscv`) set
   `CONFIG_PRIORITY_INHERITANCE=y`. Each kernel was built, and each resolved
   `CONFIG_PTHREAD_MUTEX_DEFAULT_PRIO_INHERIT=y`.
3. **Inheritance grows NuttX's semaphore-based structs, and the gate caught
   it.** `check-nuttx-libc-struct-sizes` failed on the new config:
   `pthread_rwlock_t` became 72 bytes against a 68-byte mirror in the vendored
   Rust `libc` fork, issue 0570's stack-smash class. The fork commit
   `994735191` (branch `nuttx-0.2`) sizes all four semaphore-based mirrors for
   the worst Kconfig layout: inheritance with an embedded holder, plus
   PROTECT and MAXVALUE. A compile probe measured that layout at 40/48/52/112
   bytes, against mirrors of 44/52/56/120. The gate passes on all three
   configs, and the pin moved through `nros update --source nuttx-libc`.
4. Condvar waits under a recursively held mutex: `nros/platform.h` states that
   the caller holds `m` exactly once. ThreadX (`tx_mutex_ownership_count`) and
   Zephyr's native arm (`k_mutex.lock_count`) refuse a wait that breaks this,
   returning -1 instead of deadlocking. FreeRTOS keeps the count private and
   Zephyr's POSIX arm hides it, so there the rule is written down rather than
   enforced.
5. RFC-0002's "PI mutex" row and RFC-0016's NuttX table now say what is true.

### Verified

- Linux: `c_port_posix.rs` `both_mutex_kinds_inherit_priority` locks each
  mutex kind and reads glibc's futex word, which holds the owner's tid only
  for a PI mutex. It passes, and with `setprotocol` removed it fails
  (`left: 1`).
- NuttX: the ARM fixtures were rebuilt on the PI kernel, and all 9 `rtos_e2e`
  NuttX cells pass (pub/sub, service and action × Rust, C and C++). The image
  links the kernel's inheritance code (`nxsem_boost_priority`,
  `nxsem_add_holder`).
- ThreadX: the board crate builds. Zephyr's native arm compiles in a
  `rust/listener xrce` image, and the POSIX arm in `c/talker zenoh`.
- Not measured: an actual priority inversion bounded at runtime on NuttX.
  That needs three threads at three priorities on the target. The protocol
  is verified as SET and the kernel code as linked; the inversion itself is
  not reproduced.

### Found on the way

phase-499 W5 renames a program's `main` to `nros_ported_main` with a per-SOURCE
compile definition. The NuttX boards compile app sources through cargo and
passed on only the TARGET's definitions, so every NuttX C/C++ image failed to
link (`undefined reference to 'nros_ported_main'`). Both boards now take their
definitions from one helper, `nros_target_ferried_compile_definitions`
(`cmake/NanoRosVerbs.cmake`), which adds the per-source ones.
