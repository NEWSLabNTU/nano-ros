---
id: 1014
title: "`nros_sertype.cpp` includes `<memory>` and `<string>` unconditionally, so
  the Cyclone backend has not compiled for threadx-riscv64 since it landed"
status: open
type: bug
area: rmw, build
related: [0970, 0112, 0332]
---

`nightly` → `threadx_riscv64` → step `Build (threadx_riscv64)`:

```
FAILED: …/nros-rmw-cyclonedds/CMakeFiles/nros_rmw_cyclonedds.dir/src/nros_sertype.cpp.obj
…/packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/nros_sertype.cpp:23:10:
fatal error: memory: No such file or directory
   23 | #include <memory>
compilation terminated.
ninja: build stopped: subcommand failed.
error: recipe `build-fixture-extras` failed with exit code 2
```

Red in every nightly since the file landed — `b4858f941` (2026-08-31,
issue 0970, "the Cyclone backend registers its own sertype").

## Why

The board compiles C++ freestanding. The line carries `-ffreestanding
-nostdinc++ -std=c++14` and one `-isystem` pointing at
`packages/boards/nros-board-threadx-qemu-riscv64/cxx-compat`, which contains
exactly:

```
cstdarg cstddef cstdint cstdio cstdlib cstring
initializer_list new type_traits utility
```

No `<memory>`, no `<string>`. `nros_sertype.cpp` includes both unconditionally.

This is the archived issue 0112 class — "gate `<string>`/std includes on
`NROS_CPP_STD`, not `__STDC_HOSTED__`" — at a new site. The SIBLING TU in the
same directory already carries the lesson:

```c++
// <string.h> (not <cstring>): the riscv64-threadx minimal libcpp does not
// inject strchr/strrchr into namespace std (phase-287; NROS_CPP_STD pitfall).
```

`descriptors.cpp` follows it. `nros_sertype.cpp` did not.

## The includes are not removable — the uses are real

Dropping the two `#include`s does not compile. Both are used:

| symbol | sites | what it does |
| --- | --- | --- |
| `std::unique_ptr<NrosSerdata>` | 103, 135, 161 | RAII around `new (std::nothrow)`, released on the success path (`.release()` at 129, 149, 168, 178) and freed by the destructor on every early `return nullptr` |
| `std::string NrosSertype::type_name` | member at 32 | assigned from `desc->m_typename` (391), read via `.c_str()` (392), compared with `==` (335), iterated by range-`for` for hashing (343) |

So a fix is a small rewrite, not an include change:

* `unique_ptr` → an explicit cleanup on the three failure paths, or a
  freestanding-safe scope guard. The ownership is already simple: allocate,
  bail on failure, release on success.
* `std::string` → `const char*` plus `strcmp`/`strlen`, or a fixed buffer.
  Which one depends on whether `desc->m_typename` outlives the sertype —
  **unverified**, and it decides between borrowing the pointer and copying.
  `descriptors.cpp` reaches for `<string.h>` for exactly this kind of work.

## Why this is filed rather than fixed

The fix cannot be verified where it was written. There is no
`riscv-none-elf-g++` on the machine that diagnosed it, so a rewrite would be
unverified cross-compilation code in a memory-ownership path — and a wrong
`.release()`/`delete` pairing here leaks or double-frees on an embedded target
where a return code is all you get.

## Blast radius — one board, but check before assuming

`nros-board-threadx-qemu-riscv64` is the only board with a `cxx-compat` shim,
and `cmake/toolchain/riscv64-threadx.cmake` is the only toolchain file passing
`-nostdinc++`. So on today's tree this is one lane.

That is NOT the same as "one lane can break". `esp32` and `nuttx` failed
EARLIER in the same nightly, at a preflight step, so they never reached this
compile — whether their C++ leaves would hit it too is untested. Verify after
the preflight fix (`c003ae608`) lands in a nightly rather than assuming the
blast radius is one.

## How to verify a fix

Build the threadx-riscv64 cyclone fixture on a host with the RISC-V toolchain,
or wait for the nightly `threadx_riscv64` cell. The compile is the test: the TU
either finds its headers or it does not.
