---
id: 678
title: "The threadx-riscv64 Cyclone rows cannot link `__emutls_v.errno`: the provisioned toolchain emits EMULATED TLS and the linked picolibc was built with NATIVE TLS"
status: resolved
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


## RESOLVED 2026-08-18 — option 1: the board uses the toolchain's own libc

The decision above was taken: **use the toolchain's own newlib and stop injecting
Debian picolibc's headers.**

### The defect, precisely

Every site resolving picolibc keyed on the probe's OUTPUT. That cannot
distinguish the two toolchains, because both produce an unusable sysroot string:

| toolchain | `--specs=picolibc.specs -print-sysroot` | old behaviour |
| --- | --- | --- |
| Debian `riscv64-unknown-elf-gcc` | rc=0, prints **nothing** | falls back to the Debian path — correct |
| xPack `riscv-none-elf-gcc` (what `nros setup` provisions) | **rc=1**, "cannot read spec file" | falls back to the Debian path — **wrong** |

So a NEWLIB compiler was handed picolibc's headers. The probe's **exit status**
is the fact that separates them, and all three sites now use it.

### It was in THREE places, and fixing one was not enough

The first attempt fixed only `cmake/toolchain/riscv64-threadx.cmake`. A clean
rebuild then showed picolibc gone from the app TUs while `__emutls_v.errno`
survived — because the references live in `libddsc.a` (`heap.c`,
`ddsi_config.c`), and Cyclone gets its includes from a different file:

| file | role | extra defect |
| --- | --- | --- |
| `cmake/toolchain/riscv64-threadx.cmake` | app + codegen TUs | — |
| `packages/api/nros-c/cmake/nros-threadx.cmake` | `nros_threadx_setup_picolibc()` | — |
| `cmake/platform/nano-ros-threadx.cmake` | **feeds Cyclone** via `include_directories(SYSTEM …)` | probed a HARDCODED `riscv64-unknown-elf-gcc`, so an xPack build asked Debian's compiler |

The third also skips the include entirely when there is no picolibc, rather than
injecting an empty path.

### Verified from CLEAN, which is the only test that counts here

All four `build-cyclonedds` dirs deleted, then `just threadx_riscv64
build-fixtures`:

```
0   __emutls_v references
0   `-isystem /usr/lib/picolibc` injections
c_talker      7 061 048      cpp_talker    7 094 536
c_listener    7 060 352      cpp_listener  7 094 016
```

Full builds (`[1067/1067]`, `[1034/1034]`), not relinks. This matters because an
earlier attempt to close this issue rested on `ninja` in build dirs configured
five days before — those relink against whatever `-L` order the stale configure
captured, which is the same trap that produced this issue's "the C rows are a
working control" claim. **Wipe the build dirs or the result means nothing.**

### The lane is still red, for #0668

`build-fixture-extras` now fails at `rust/talker` with "`#[panic_handler]`
function required, but not found" in `nros-c`. That is
[issue 0668](0668-threadx-rv64-example-shape-differs-from-every-other-standalone.md)
— ThreadX-RV64 being the only standalone example with two entry points — and
phase-366 is actively landing on it. Untouched here.

### Neither reverted attempt is needed

Attempt 1 (sysroot-first archive resolution) selected picolibc, the archive whose
TLS model this compiler cannot use; the fix goes the other way. Attempt 2
(absolute archive path instead of `-L` + `-lc`) addressed a real order dependency
that no longer decides anything once one libc is on the line — worth doing on its
own merits, not as part of this.


### Why this is not #0679's refuted attempt B

[#0679](0679-riscv64-forces-picolibc-headers-onto-newlib-toolchain.md), filed
independently for the same failure, records that defining `__thread int errno;`
in `startup.c` also makes all four rows link — and that it is **unsound**: the
emulated-TLS definition is a different storage from the `.tbss` slot picolibc's
own code reads, so libc sets one `errno` and the application reads another, with
no diagnostic. Its conclusion is the right one: *linking is not the acceptance
criterion for a symbol whose purpose is to carry a value between two pieces of
code.*

This fix is not that, and the images say so. On `c_listener` from the clean
build:

```
nm | grep -c emutls            5      (Rust's own thread-locals + __emutls_get_address)
nm | grep -c __emutls_v.errno  0      <- errno is NOT emulated-TLS here
nm | grep -w errno             B errno      \  ONE storage, newlib's
nm | grep -w __errno           T __errno    /
nm --print-file-name | grep -c picolibc   0
nm -u | wc -l                  0      (no undefined symbols)
```

There is one C library in the image and one `errno` in it. The surviving emutls
objects are `thread_context`, `tsd_thread_state` and `freelist_inner_idx` —
Rust's thread-locals, each defined in the same image that references them, which
is emulated TLS used consistently rather than two models meeting.

#0679's attempt A (making the `-isystem` conditional on the compiler having its
own headers) is also distinct: it left picolibc on the LINK while dropping
`NROS_LIBC_PICOLIBC`, so `startup.c` stopped defining `stdout`/`stderr` that
picolibc still expected — one undefined symbol became three. Here picolibc
leaves the link entirely (`nros_threadx_setup_picolibc` returns before
publishing its lib dir), so newlib defines that stdio itself and `startup.c`
correctly must not.
