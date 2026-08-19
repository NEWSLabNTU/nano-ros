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

## Addendum — what the upstream projects advise, and what this fix leaves behind

Independent research reaching the same conclusion this fix implements, plus the
half it does not cover. Sources at the end.

**picolibc decides TLS at ITS OWN build time.** `-Dthread-local-storage`
defaults to `auto` — *"based on compiler support"* — and `-Dnewlib-global-errno`
(*"use single global errno even when thread-local-storage=true"*) is likewise a
picolibc BUILD option. So whether `errno` is `__thread` is fixed when picolibc
is compiled, by whichever compiler compiled it. A consumer cannot change it,
which is exactly why defining `NEWLIB_GLOBAL_ERRNO` in our own TUs was unsound —
it changes the declaration and not the archive (issue 0679 reached this from an
attempt that linked).

**Zephyr states the rule this fix restores.** Zephyr uses the toolchain-BUNDLED
picolibc, and says the SDK's copy and the module are *"guaranteed to be in
sync"*; building from source is the exception for toolchains with no bundled
copy. Injecting a distro picolibc into a different toolchain is the thing that
principle exists to prevent. Worth noting alongside: the picolibc TLS discussion
records the arm gnu toolchain also shipping without TLS, so a bare-metal GNU
toolchain lacking it is a recurring condition, not an xPack quirk.

**What this fix hands to ThreadX.** picolibc kept `errno` in compiler TLS, so
per-thread `errno` came for free wherever the compiler supported it. newlib
resolves `errno` through `_impure_ptr` — one global pointer to one
`struct _reent` — and ThreadX's mechanism for per-thread library state is a
`TX_THREAD_EXTENSION` slot in `tx_port.h`, saved and restored by the context
switch. That wiring does not exist here: all four extension slots are empty and
there is no `_impure_ptr` or `__retarget_lock_*` anywhere in the tree.

So `errno` and `_reent` are now SHARED across ThreadX threads. That is a
pre-existing property of newlib made REACHABLE by this fix rather than a
regression it introduced — before it, the board did not link at all on a
provisioned host — and it is not an argument for going back, since picolibc's
per-thread `errno` only ever worked where the compiler had native TLS. It is
tracked as [issue 0680](../0680-threadx-newlib-reent-shared-across-threads.md).

### Sources

* picolibc build options — <https://github.com/picolibc/picolibc/blob/main/doc/build.md>
* picolibc TLS design — <https://github.com/picolibc/picolibc/blob/main/doc/tls.md>
* Zephyr picolibc integration — <https://docs.zephyrproject.org/latest/develop/languages/c/picolibc.html>
* ThreadX errno / `TX_THREAD_EXTENSION` — <https://learn.microsoft.com/en-us/answers/questions/1245073/integrating-bsd-library-with-azure-threadx-rtos>
* ThreadX + newlib reentrancy, unanswered — <https://github.com/eclipse-threadx/threadx/issues/448>

## REOPENED 2026-08-19 — the fix reached the cmake path only; cargo still injects picolibc

`066441663` is correct and stays. It did not reach the fourth site, so the
platform still does not build:

```
$ just threadx_riscv64 build-fixtures      # clean tree, nothing local
threadx-riscv64-c-cyclonedds:
  rust-lld: error: undefined symbol: __emutls_v.errno   (x2, from libddsc.a)
BUILD_RC=2
```

### The fourth site

The fix keyed the choice on the picolibc-specs probe's **exit code** — reject
`picolibc.specs` and this is not a picolibc toolchain, so use its own headers.
`packages/boards/nros-board-common/src/threadx_qemu_riscv64_build.rs` asks the
same question and answers it the opposite way:

```rust
fn get_picolibc_sysroot() -> Option<PathBuf> {
    if let Ok(output) = Command::new(...).args([..., "--specs=picolibc.specs",
                                                "-print-sysroot"]).output()
        && output.status.success()
    { ...use it... }
    // Fallback: known system location
    let fallback = PathBuf::from("/usr/lib/picolibc/riscv64-unknown-elf");
    if fallback.join("include").exists() { return Some(fallback); }
```

A FAILED probe falls through to a hardcoded Debian path. Measured on this host:

```
$ riscv-none-elf-gcc --specs=picolibc.specs -print-sysroot ; echo $?
riscv-none-elf-gcc: fatal error: cannot read spec file 'picolibc.specs'
1                                    <- not a picolibc toolchain
$ ls -d /usr/lib/picolibc/riscv64-unknown-elf/include
/usr/lib/picolibc/riscv64-unknown-elf/include        <- taken anyway
```

`get_picolibc_lib_dir()` then feeds `cargo:rustc-link-lib=static=c` from the
same sysroot. So after the fix the board links **newlib through cmake and
picolibc through cargo** — two C libraries in one board, which is the shape this
issue exists to remove, one layer over. The C/C++ leaves go through cmake and
the Rust leaves through cargo, so which libc a leaf gets depends on what language
it is written in.

### Why this was not caught

`066441663`'s verification was a clean rebuild that got further than before. It
did get further — the failure moved from the C++ rows to the C ones — and
"further" was read as "fixed" for the platform rather than for the sites
touched. The C Cyclone rows still fail, and they did before the fix as well.

### A trap for whoever fixes this — measured, not predicted

The board defines `int errno;` in `c/board_threadx_qemu_riscv64.c`, commented as
a bare-metal global. Under newlib that global is genuinely unreachable —
`<errno.h>` makes `errno` the macro `(*__errno())` — so it reads like dead code.
**It is not dead on the cargo path**, because that path still links picolibc,
where `errno` is a real extern symbol, and the board's definition is the one
near enough to satisfy `libzpico_sys`:

```
rust-lld: relocation R_RISCV_PCREL_HI20 out of range: -524334 is not in
          [-524288, 524287]; references 'errno'
          referenced by libzpico_sys(...endpoint.o)
          defined in libnros_board_threadx_qemu_riscv64(libc_errno_errno.c.o)
```

That is what deleting it produces today: twelve out-of-range relocations, because
the reference falls through to picolibc's own copy half a megabyte away. The
global becomes removable only AFTER both paths agree on newlib — and that
ordering is the whole content of this note.

### Fix

Make `get_picolibc_sysroot()` treat a failed specs probe the way the cmake side
does: not a picolibc toolchain, return `None`, let the compiler use its own
headers and its own `libc.a`. The hardcoded `/usr/lib/picolibc/...` fallback is
the defect, not a safety net — it is the line that pairs a distro libc with
whatever compiler happens to be resolved.

Sequencing that follows from the trap above:

1. cargo path stops injecting picolibc — both paths on the toolchain's libc;
2. THEN `int errno;` is dead everywhere and can go (issue 0680's note);
3. THEN newlib's `__retarget_lock_*` are worth wiring, and are wired once
   rather than into a glue file two different libcs compile.

### Acceptance

Not "the build gets further". `just threadx_riscv64 build-fixtures` to
`BUILD_RC=0`, with zero `__emutls_v.errno` and zero out-of-range relocations —
and the C, C++ AND Rust rows all producing artifacts, since each exercises a
different one of the two paths.

## Root cause of the REOPEN — the fix is correct and existing build trees never see it

The fourth site above is real and fixed below, but it is not why the C rows kept
failing. That has a sharper cause, and it invalidates the earlier verification
rather than adding to it.

**`CMAKE_C_FLAGS` is a CACHE variable seeded from `CMAKE_C_FLAGS_INIT` on the
FIRST configure only.** A toolchain file is not re-applied to an existing build
directory; CMake rewrites `CMakeCache.txt` on every configure — so its mtime
looks current — while the flags inside it are the ones computed the first time.

Measured on the leaf that kept failing:

```
$ ls --time-style=+%m-%d_%H:%M .../c/listener/build-cyclonedds/CMakeCache.txt
08-19_04:25                                   # today, AFTER the 08-18 17:20 fix
$ grep -m1 PICOLIBC .../CMakeCache.txt
CMAKE_CXX_FLAGS:STRING= … -isystem /usr/lib/picolibc/riscv64-unknown-elf/include
                          -DNROS_LIBC_PICOLIBC=1
```

Deleting the two C Cyclone build directories and reconfiguring:

```
picolibc references in the fresh CMakeCache : 0
__emutls_v.errno errors                     : 0
```

So `066441663` works. It simply cannot reach a tree that already exists, and
every tree on this machine did. This is the museum-binary class (issue 0475) in
its configure-time form: the inputs changed, the artifact did not, and nothing
said so.

**Consequence for anyone verifying a toolchain-file change:** a rebuild is not a
test of it. The build directory must be deleted. That is also why the earlier
"clean rebuild" read as progress — the failure moved between rows because the
rows had been configured at different times, not because anything was fixed.

## Fixed here — the cargo path's hardcoded fallback

`get_picolibc_sysroot()` fell through to `/usr/lib/picolibc/riscv64-unknown-elf`
whenever that directory existed, INCLUDING when the specs probe had just failed.
That fallback is removed: a failed probe means the compiler is not a picolibc
toolchain, so `None` leaves it to use its own headers and its own `libc.a` —
the same rule `066441663` applied on the cmake side.

Unlike the cmake side, this one takes effect immediately: cargo re-runs
`build.rs`, so there is no cache to invalidate.

## Still failing, and NOT this issue

With fresh trees and both paths on the toolchain's libc, the platform gets
further and stops somewhere new:

```
error: `#[panic_handler]` function required, but not found
error: could not compile `nros-c` (lib)
```

Cause not established. It appeared on the first from-scratch configure this
board has had in a while, and nothing in this issue's fix touches panic
handlers, so the likely reading is another failure the picolibc one was standing
in front of — the third time in this sequence (0674 → 0678 → this). Filed
separately rather than absorbed here, because "the build gets further" is
precisely the reasoning this reopen exists to correct.


## 2026-08-19 — both libc halves verified on a WIPED tree; the third failure is isolated

`just threadx_riscv64 build-fixtures` with every `examples/qemu-riscv64-threadx/*/*/build-*`
deleted first — the only run that means anything here, since a toolchain fix
cannot reach an existing tree:

| | count |
| --- | --- |
| `__emutls_v` | **0** |
| `sys/reent.h: No such file` | **0** |
| `riscv64-threadx libc = newlib` | 32 configures, all agreeing |

### What the reent failure actually was (issue 0680's half)

0680's `reent.c` is newlib-only — it swaps `_impure_ptr`, and picolibc has no
`<sys/reent.h>` at all because its `errno` lives in compiler TLS. Since THIS
issue made the libc a per-toolchain fact, that file cannot be unconditional.

The guard reads the choice the toolchain PUBLISHES rather than re-deriving it:
`NROS_RISCV64_LIBC` (CACHE, so it survives `try_compile` re-execution), and the
board appends `reent.c` only when it says `newlib`. That is issue 0674's rule —
"the code that CHOOSES the libc now publishes the choice" — applied to CMake
instead of `startup.c`. Before it, the choice reached the preprocessor and
nothing else, so a CMakeLists deciding whether to COMPILE a libc-specific source
had to guess, and 0680 guessed newlib for every host.

### A correction to this issue's own reopen note

The reopen says a stale tree cannot see the fix, and that is right. I first read
the Debian compiler in those trees as a PREFIX-SELECTION bug — it is not. A
fresh configure picks the xPack toolchain and prints `libc = newlib`;
`CMAKE_C_COMPILER` is simply sticky, so a tree first configured before the SDK
toolchain existed keeps its original compiler and its `*_AR`/`*_RANLIB`
siblings. The selection logic is correct; only the cache is old.

### Still failing, now isolated — and it is NOT this issue

```
→ examples/qemu-riscv64-threadx/rust/talker (-DNROS_RMW=cyclonedds, build-cyclonedds/)
   Compiling nros-c v0.5.0
error: `#[panic_handler]` function required, but not found
```

The C/C++ cyclone leaves build `nros-c` WITH `panic-platform` on the same run
(`--features=ros-humble,rmw-cffi,alloc,platform-threadx,panic-platform`), so the
corrosion path is fine. The RUST leaf is the one that fails, and the mechanism is
issue 0644's: `nros-c` is `crate-type = ["staticlib", "cdylib", "lib"]`, a
staticlib is a FINAL artifact, so rustc demands the lang item while compiling it
— while the Rust image already declares its own ending in `app_main.rs`
(`nros::panic_to_platform!()`), so it cannot also take `nros-c/panic-platform`
without shipping two providers.

That is the per-image singleton question of RFC-0077 / issues 0618 and 0668 for
a Rust image that also links the C staticlib, not a libc problem. Filed as its
own thing rather than absorbed here — the third distinct failure this sequence
has uncovered (0674 → 0678 → this), each standing in front of the next.

## VERIFIED and CLOSED 2026-08-19

The option-1 fix holds. Verified on a CLEAN tree — the reopen note was right that
a stale tree cannot see it, and that is what kept this open.

```
emutls errors (`__emutls_v.errno`) : 0
`sys/reent.h` errors               : 0
toolchain prefix                   : ~/.nros/sdk/riscv-none-elf-gcc/14.2-nros1/bin/riscv-none-elf
C   cyclone executables linked     : c_talker, c_listener, c_action_{client,server},
                                     c_service_{client,server}
C++ cyclone executables linked     : cpp_listener, cpp_action_{client,server},
                                     cpp_service_client
```

Both languages link, which is the property this issue was opened on the absence
of — and it is the C++ rows, the ones originally reported, that are now proven
rather than assumed.

### Why it looked unfixed for a day

`CMAKE_C_COMPILER` is sticky. A tree first configured before the SDK toolchain
existed keeps the Debian compiler and its `*_AR`/`*_RANLIB` siblings forever, so
the fix is invisible until the build dir is deleted. This issue's own correction
says so ("the selection logic is correct; only the cache is old"), and it
reproduced here independently: 22 stale `build-*` dirs under
`examples/qemu-riscv64-threadx/` were resolving `/usr/bin/riscv64-unknown-elf-gcc`
until they were wiped, after which the xPack prefix appeared immediately.

Worth stating as a rule rather than an anecdote: **on this board, a toolchain
change is not testable incrementally.** Any future libc/toolchain work here must
delete the build dirs before believing a result — a green or a red both mean
nothing otherwise.

### What remained behind it, and where it went

The only failure left in that run was `nros-c` failing `#[panic_handler]` in the
RUST Cyclone leaf — not a libc problem, and not this issue. It was filed as
0688, retired into [0692](archived/0692-rust-cyclone-image-links-two-rust-staticlibs.md),
and fixed by `eb54c1170` ("the rust-cyclone seam is not an entry, so nothing gave
nros-c a panic policy").

So the sequence this platform surfaced — 0674 → 0678 → 0692 — is now closed end
to end, each having stood in front of the next.
