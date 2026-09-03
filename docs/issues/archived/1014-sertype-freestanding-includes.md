---
id: 1014
title: "`nros_sertype.cpp` includes `<memory>` and `<string>` unconditionally, so
  the Cyclone backend has not compiled for threadx-riscv64 since it landed"
status: resolved
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

## RESOLVED — and the lifetime question dissolved

Fixed in the same pull request that filed this.

The first draft said the fix could not be verified here because there is no
`riscv-none-elf-g++` on this machine. **That was wrong, and the mistake is worth
recording:** the toolchain was not on `PATH`, but it was in the SDK store all
along, at `~/.nros/sdk/riscv-none-elf-gcc/14.2-nros1/bin/riscv-none-elf-g++`. A
`command -v` came back empty and that was read as "absent" rather than "not on
PATH" — the same shape as concluding a module had no `setup` recipe from a
`grep` of a filename that does not exist.

**The `std::string` did not need replacing. It needed deleting.**
`ddsi_sertype_init_flags` does `tp->type_name = ddsrt_strdup(type_name)`
(`ddsi_sertype.c:176`), so Cyclone already owns a heap copy in the BASE
`ddsi_sertype::type_name`, freed by `ddsi_sertype_fini` in `sertype_free`. The
derived member was a redundant second copy whose only job was to hold the string
long enough to pass `.c_str()` to a function that immediately duplicates it.

So the question this issue flagged as unverified — does `desc->m_typename`
outlive the sertype — **does not need answering**. The descriptor's name is
handed straight to `ddsi_sertype_init_flags`, and the only copy that survives is
Cyclone's. Equality and hashing read the base field: same bytes, same order, so
the hash values are unchanged, which matters because a shifted sertype hash
would silently stop matching remote types.

`std::unique_ptr` became a 20-line `OwnPtr` with the same shape, so the call
sites are unchanged (`!d`, `d.get()`, `d->field`, `d.release()`).

### What the compiler caught that reading did not

`auto d = OwnPtr<T>(p);` does not compile under `-std=c++14`: it is
copy-initialisation and needs an accessible copy or move constructor, since
guaranteed elision is C++17. `unique_ptr` got away with the spelling by having a
move constructor. Rather than add a move this type never uses, the call sites
are direct-initialised. Found by compiling, not by reviewing.

### Verified

* **Freestanding, real toolchain**: `riscv-none-elf-g++ 14.2.0`,
  `-ffreestanding -nostdinc++ -std=c++14 -Wall -Wextra`, `-isystem` the
  `cxx-compat` shim and nothing else — the `OwnPtr` used exactly as at the three
  call sites, `std::strcmp`, and the FNV loop all compile. (`<cstring>` is in the
  shim and exports `std::strcmp`/`std::strlen`; `<memory>` and `<string>` are
  confirmed absent, which is the original failure.)
* **Hosted, the real translation unit**: clean under `-Wall -Wextra`.
* **Behaviour**: `just check rmw-cyclonedds` — 23/23 tests pass.

The one thing still NOT verified here is a full cross build of the TU, because
that needs a ThreadX-configured Cyclone; the installed 0.10.5 headers in the
store are POSIX-configured and pull `sys/socket.h`. The nightly
`threadx_riscv64` cell is the check that closes it.

## Blast radius — one board, but check before assuming

`nros-board-threadx-qemu-riscv64` is the only board with a `cxx-compat` shim,
and `cmake/toolchain/riscv64-threadx.cmake` is the only toolchain file passing
`-nostdinc++`. So on today's tree this is one lane.

That is NOT the same as "one lane can break". `esp32` and `nuttx` failed
EARLIER in the same nightly, at a preflight step, so they never reached this
compile — whether their C++ leaves would hit it too is untested. Verify after
the preflight fix (`c003ae608`) lands in a nightly rather than assuming the
blast radius is one.

## How to verify

Build the threadx-riscv64 cyclone fixture, or wait for the nightly
`threadx_riscv64` cell. The compile is the test: the TU either finds its headers
or it does not.
