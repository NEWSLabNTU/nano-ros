---
id: 678
title: "The threadx-riscv64 Cyclone rows cannot link `__emutls_v.errno`: the provisioned toolchain emits EMULATED TLS and the linked picolibc was built with NATIVE TLS"
status: open
type: bug
severity: high
area: build, boards
related: [issue-0674, issue-0664, issue-0657, phase-251]
---

## Symptom

`just threadx_riscv64 build-fixtures`, with
[issue 0674](archived/0674-threadx-riscv64-cyclone-link-undefined-stdio.md) fixed:

```
rust-lld: error: undefined symbol: __emutls_v.errno
>>> referenced by heap.c
>>>               heap.c.obj:(ddsrt_malloc_s) in archive
>>>               .../cpp/listener/build-cyclonedds/lib/libddsc.a
>>> referenced 10 more times
```

Both `threadx-riscv64-cpp-cyclonedds` rows (talker, listener). `BUILD_RC=2`.

## Why it appears now

It was always there; #0674 was in front of it. That issue's
`undefined symbol: stdout` / `stderr` killed the platform before any C++
Cyclone row reached its link, so this is CLAUDE.md's "one fix can unmask the
next", not a regression from the fix.

Measured after the #0674 fix, same build:

| row | result |
| --- | --- |
| `threadx-riscv64-c-cyclonedds` | **links** — `c_talker`, `c_listener` produced |
| `threadx-riscv64-cpp-cyclonedds` | `__emutls_v.errno` undefined |
| `undefined symbol: stdout`/`stderr` anywhere | **0 occurrences** |

## What the symbol is

`__emutls_v.errno` is the compiler-emitted control object for a `__thread`
variable named `errno`. picolibc declares `errno` thread-local, so every
reference compiles to an emutls lookup and the DEFINITION has to come from
whichever TU defines the variable — picolibc's `libc.a`.

The C rows link the same `libddsc.a` and resolve it. The C++ rows do not, so the
difference is in the C++ link line, not in Cyclone.

The obvious suspect is the C++ lane's deliberately different libc surface:
`cmake/toolchain/riscv64-threadx.cmake` gives C++ `-nostdinc++` plus the board's
`cxx-compat/` shim (issue 0657), and resolves a separate `libstdc++.a` for the
Cyclone wrapper (issue #195). Whether any of that changes which `libc.a` is on
the link line, or its ORDER relative to `libddsc.a`, is NOT established here.

## Not the same as #0664

[Issue 0664](archived/0664-threadx-rv64-cyclone-never-subscribes.md) is also
about emutls on this board and is a different failure: `__emutls_get_address`
calling `malloc` and `abort()`ing at RUNTIME because `_sbrk` refused, fixed by
giving `.heap` 64 KiB. That one links and dies; this one does not link. A fix
there does not touch this.

## Direction

1. **Diff the two link lines.** `ninja -t commands` for the C and C++ Cyclone
   executables in the same build tree, and compare which libc archive appears
   and where. The C row is a working control in the same tree, which is the
   cheapest evidence available and does not exist for most link bugs.
2. Only then decide between "the C++ link is missing picolibc's `libc.a`" and
   "it has it in an order that does not pull the emutls object" — those have
   different fixes and the error cannot distinguish them.

## Not verified

* whether the zenoh C++ riscv64 rows link. They were not built in this run, and
  they share the toolchain's C++ surface, so they may be equally affected —
  #0674 turned out to be exactly that kind of coverage artifact.

## Investigation 2026-08-18 — the cause is a TLS MODEL mismatch, not the link line

Direction 1 (diff the two link lines) was the right first move and it did find a
difference — but the difference was a symptom, and chasing it produced two
changes that had to be reverted. Recorded in full so the next attempt starts
from the real fact.

### The real fact

**The provisioned compiler and the linked picolibc disagree about how
thread-local storage works, and neither can be talked out of it.**

`errno` is `__thread` in picolibc's headers (`sys/errno.h:58`, via
`NEWLIB_THREAD_LOCAL` ← `PICOLIBC_TLS`; `picolibc.h:99` explicitly `#undef`s the
`NEWLIB_GLOBAL_ERRNO` escape). Two implementations of `__thread` exist, and this
build has one of each:

```
$ riscv-none-elf-gcc … -isystem <picolibc>/include -c 'int f(void){return errno;}'
$ nm e.o
                 U __emutls_get_address
                 U __emutls_v.errno          <- EMULATED tls

$ nm --format=sysv <picolibc>/lib/rv64imafdc/lp64d/libc.a | grep -w errno
errno |0000000000000000| B | TLS |0000000000000004| |.tbss   <- NATIVE tls
$ nm <picolibc>/…/libc.a | grep -c emutls
0
```

So picolibc's archive contains **no emutls symbols at all** — nothing anywhere
can define `__emutls_v.errno`. And the compiler cannot be asked for native TLS:

```
$ riscv-none-elf-gcc … -fno-emulated-tls
riscv-none-elf-gcc: error: unrecognized command-line option '-fno-emulated-tls'
```

The xPack `riscv-none-elf` toolchain `nros setup` provisions is built without
native TLS for this target; emulated TLS is its only model. Debian's picolibc
was built by a compiler that has native TLS. The two are not compatible at any
symbol that is `__thread`, and `errno` is one.

### Correction to this issue as filed

**"The C Cyclone rows link and produce binaries, so a working control exists" is
WRONG.** That observation came from incremental build trees. Deleting the four
Cyclone build dirs and rebuilding from clean, on unmodified `main`, fails on the
**C** leaves:

```
failing leaves: qemu-riscv64-threadx/c/listener, .../c/talker
```

Which language appears to fail is decided by `-L` ordering, which differs
between configures, and by whether `--gc-sections` happens to drop the
referencing code in that image. There is no working control; there is one bug
that surfaces wherever a retained caller touches `errno`.

### Tried and REVERTED — do not retry these

1. **Sysroot-first archive resolution** (`nros-threadx.cmake`): make
   `${_sysroot}/lib/.../libc.a` win over the compiler's own
   `-print-file-name=libc.a`, so headers and archive come from one install.
   Correct in principle and it did make the choice deterministic — but it
   selects picolibc, which is exactly the archive whose TLS model the compiler
   cannot use. Failure moved from C++ to C.
2. **Naming the archive absolutely** instead of `-L<dir>` + `-lc`
   (`nano-ros-board-riscv64-qemu.cmake`). Removes a real order dependency —
   `-lc` was resolving against whichever `-L` CMake happened to emit first, and
   both orders were observed — but it does not address the TLS model, so the
   link still fails. Worth revisiting AFTER the decision below, not before.

### The decision this needs

Not a flag. Pick which C library this board uses with the provisioned toolchain:

1. **Use the toolchain's own newlib** and stop injecting Debian picolibc's
   headers. Measured self-consistent: the same TU compiles to `U __errno`, and
   the xPack `libc.a` defines `__errno` (T) and `errno` (B) — no TLS, no emutls.
   `startup.c` already carries the `#elif defined(__NEWLIB__)` arm for it
   (issue 0674). The cost is everything that assumed picolibc: the `cxx-compat`
   shim's rationale, `_sbrk`/`.heap` (issue 0664), and the phase-155.E reason
   the headers were forced in the first place.
2. **Use the compiler that built the picolibc being linked** — Debian's
   `riscv64-unknown-elf-gcc` — when it is present, and treat "matches the libc"
   as a resolution criterion in `_nros_riscv64_find_prefix`. Keeps picolibc;
   costs the property issue 0657 bought, that a `nros setup` host builds this
   board with what it provisioned.

Either is defensible; they are not interchangeable, and the choice belongs to
whoever owns the board's libc story rather than to whoever hits the link error.
