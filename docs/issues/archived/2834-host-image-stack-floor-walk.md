---
id: 2834
title: "`nros build`'s stack-floor walk never reaches a host image, so threadx-linux states a stack nothing checks"
status: wontfix
resolved: 2026-11-28
resolved_in: "the change that adds this line"
type: limitation
severity: low
area: [threadx, build, cli]
related: [issue-2800, issue-2400, phase-557]
found: 2026-11-22
---
## What
Since issue 2800, ThreadX's Linux port runs each thread on its ThreadX stack,
and the threadx-linux board states its app stack (`stack_stated::emit`, 64 KiB
by default, knob `NROS_MEMORY_APP_STACK_BYTES`). Nothing checks that
statement:

- `stack_floor::plan_one` (`packages/cli/nros-cli-core/src/cmd/stack_floor.rs`)
  returns before reading the platform lists when the plan's cargo output has
  no cross triple. That is every host image, so a threadx-linux build prints no
  stack-floor verdict, not even "not checked". Measured: `nros build
  condvar_contract_threadx_linux_bringup:threadx-linux` prints none.
- threadx-linux therefore stays on `UNCHECKABLE_PLATFORMS` and off
  `STATING_PLATFORMS`.

## Why it is more than a list move
The walk (`scripts/nros-stack-floor.py`) sums frames from the image's own
code. On a host image the app thread's deepest paths run through the host's
glibc, whose frames the walk has no sizes for. A walk that skips them reads
low, which is the unsafe direction.

## Fix (not started)
1. Let `plan_one` arm a host image whose platform states a stack, instead of
   returning on a missing cross triple.
2. Give the walk the host libc's frames, or refuse a path that enters a
   function it has no size for, naming it.
3. Move threadx-linux to `STATING_PLATFORMS`.

Each threadx-linux image prints `nros: stack peak name=… used=N of=M` for
every thread when it ends (any termination signal included).

## Progress (2026-11-23): sized from the measured peak; the libc walk is deferred

Ruled by the manager, following the user's peak-sizing ruling for measured
rows (the rule issue 2783 applies to a row whose walk is an upper bound):

- `nros build` now arms a threadx-linux cargo image
  (`HOST_MEASURED_PLATFORMS` in `stack_floor.rs`, replacing
  `UNCHECKABLE_PLATFORMS`). Cargo's command runs unchanged (no walk flags);
  afterwards `nros-stack-floor.py --measured-only` reads the board's stated
  stack from cargo's messages and holds it to the image's `[[observed]]` row:
  floor = ceil8(peak x (100 + margin) / 100). A stack below it fails, naming
  `NROS_MEMORY_APP_STACK_BYTES`. An image with no row is reported as not
  checked, with how to measure it.
- A host row carries `unwalked = "<reason>"` instead of `static`; the
  validator refuses one that also records a walk (`static`, `walk_floor`,
  `upper_bound`) and one with no reason. Its `target` is the host triple it
  was measured on. The drift check and the twin lane skip it (nothing to
  re-walk, no twin), and `nros new` stamps a template's stack from its peak.
- The first row: the threadx-linux Rust talker, measured on x86_64 Linux.

**Deferred: steps 1-3 of the fix above (the libc walk).** Estimate 1-2 days:
the walk needs glibc's frames, which the host's `libc.so.6` holds only as
`.eh_frame` CFA rules (no `-fstack-usage`), plus a policy for the calls that
leave glibc into the kernel; and `nros build` without `--target` applies
target rustflags to build scripts too, so the walk's flags need a
build-script-safe carrier on a host image. Until that lands a host image is
MEASURED, never walked: a path the run did not take is not covered.

The cmake road does not check a threadx-linux C/C++ image either; its walk
checks the FreeRTOS carrier only (`cmake/NanoRosStackFloor.cmake`).

## Resolution (2026-11-28): wontfix, the measured peak is the check

Ruled by the manager after the measurement below. A static walk of the host
libc (steps 1-3 of the fix) will not be built for host images, and the
measured-peak road (2026-11-23) is the answer. NSS modules make
`getaddrinfo` impossible to bound in principle. The stdio jump tables and
the signal frames make the walk unsound in practice. The prerequisites
listed at the end (no hostname resolution and no stdio `printf` on image
threads) are NOT filed: they would cost users features to enable a check the
measured peak already covers. Anyone revisiting this starts from the
evidence below.

## Measured (2026-11-28): the host libc cannot be walked soundly as it stands

The deferral was lifted, with "measure first". The measurement says steps 1-3
cannot be made sound for this image without changing what the image calls.
The checked road stays the measured peak (2026-11-23, above).

Method: a prototype walk over the host's glibc (Ubuntu GLIBC 2.35-0ubuntu3.18,
x86_64). Frames come from `libc.so.6`'s `.eh_frame`: the deepest rsp-relative
CFA offset per FDE, the rule `load_cfi_frames` already applies to a cross
image's newlib. The call graph comes from disassembly, with function bounds
from `libc6-dbg`'s symbol table (`libc.so.6` itself has only `.dynsym`). The
walk started at each of the 98 imports of the threadx-linux Rust talker
(`threadx_linux_entry`, `BIND_NOW`, so no lazy-binding resolver runs on a
thread stack).

What the frames say:

- 3,609 FDEs over 3,601 functions. 105 move the CFA off `rsp`, so the frame
  is not static (alloca or a VLA). Static frames are the common case, and
  that half of step 2 works as planned.
- The plain wrappers are small and sound, once the walk does not follow the
  process-ending tails (`abort`, `__fortify_fail`, `__libc_fatal`,
  `__pthread_unwind`). Deepest path:

  | import | deepest path | what blocks a sound bound |
  |---|---|---|
  | `write`, `read`, `send`, `recv`, `poll`, `close` | 64-144 B | nothing |
  | `sem_wait`, `clock_nanosleep` | 144-192 B | 1 indirect call (futex wait) |
  | `pthread_mutex_lock` | 240 B | nothing |
  | `pthread_create` | 864 B | 2 indirect calls |
  | `malloc`, `free` | 4,320 B (first call: `tcache_init` > `arena_get2` > `get_nprocs`) | 1 indirect call |

- If those tails ARE followed, every wrapper reaches the cancellation-unwind
  path or `__stack_chk_fail`, then `__libc_message` / `__printf_fp_l`. Those
  have register-based CFAs, so every path refuses. Not following them is a
  policy (the process is ending), not something the walk proves.

Two imports stay unsound with the tails cut, and the talker's app thread
reaches both:

- `getaddrinfo`, through the NSOS shim's `nx_bsd_getaddrinfo` on session
  open: 36 indirect calls, 13 register-CFA functions and recursion. Its
  deepest cost comes from NSS modules (`libnss_*.so`) that glibc `dlopen`s
  per the host's `/etc/nsswitch.conf`. Their code is in no file the build can
  read, and it differs between hosts by configuration alone.
- `__printf_chk`, through zenoh-pico's diagnostics in `zpico_init_with_config`
  and the `zpico_declare_*` calls: 39 indirect calls (the stdio `_IO_*` jump
  tables), 13 register-CFA functions (`__printf_fp_l`'s alloca among them)
  and recursion (`printf_positional`).

Two more costs are outside every unwind table:

- The Linux port suspends and resumes ThreadX threads with signals
  (`_tx_linux_thread_suspend`, `RESUME_SIG`), and the kernel builds each
  signal frame on the interrupted thread's stack. Its size depends on the
  CPU's xstate: `AT_MINSIGSTKSZ` is 3,376 B on this AVX-512 host. It can be
  larger with AMX, and it can nest, because the suspend handler calls
  `sigsuspend`, which unmasks `RESUME_SIG`. A walk would have to charge
  it per host, read from `getauxval(AT_MINSIGSTKSZ)` at build time.
- The frames hold only for the glibc build they were read from. That is the
  "build where you run" host for a native image, but a walk would have to key
  its result on the libc's build ID and refuse on any other.

So the sound walk is possible only after changes outside the walker:

1. The image's threads must not reach `getaddrinfo`. For example, the NSOS
   shim could resolve a numeric locator with `inet_pton` and refuse a name.
   A static walk cannot bound NSS in principle, not just in this
   implementation.
2. The image's threads must not reach stdio `printf`, or glibc's stdio
   dispatch must be modelled (the jump-table targets are a closed set inside
   libc, but `__printf_fp_l`'s alloca has no static bound).
3. The walker needs a written policy for process-ending tails, the signal
   frame charge, and the build-ID key. Then the ~1-2 day estimate applies to
   steps 1-3 themselves.

Until then a host image is measured, never walked. A path the run did not
take is not covered, and the measured road already says so.
