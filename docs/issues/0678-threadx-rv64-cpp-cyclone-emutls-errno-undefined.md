---
id: 678
title: "The threadx-riscv64 C++ Cyclone rows fail to link with `undefined symbol: __emutls_v.errno` — unmasked once #0674's `stdout`/`stderr` failure stopped hiding them"
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
