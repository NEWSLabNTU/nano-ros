---
id: 38
title: nros-cpp heap headers need `nros_platform_malloc`/`free`; platform-cffi only declares `alloc`/`dealloc`
status: open
type: bug
area: c-api
related: [issue-0034, issue-0036, phase-235]
---

The CFFI-platform C++ build fails to compile nros-cpp's heap containers because
the platform header it resolves declares only `nros_platform_alloc` /
`nros_platform_dealloc`, not the `nros_platform_malloc` / `nros_platform_free`
that `heap_string.hpp` / `heap_sequence.hpp` call. Surfaced by e2e dispatch run
27396365520 (threadx_riscv64 cell, cpp service-client fixture):

```
nros-cpp/include/nros/heap_sequence.hpp:75:36: error: there are no arguments to
  'nros_platform_malloc' that depend on a template parameter, so a declaration
  of 'nros_platform_malloc' must be available [-fpermissive]
nros-cpp/include/nros/heap_string.hpp:43:21: error: 'nros_platform_free' was not
  declared in this scope; did you mean 'nros_platform_alloc'?
... (heap_string.hpp:54,73,78,86)
ninja: build stopped: subcommand failed.
error: recipe `build-fixture-extras` failed with exit code 2
```

## Root cause — two platform.h variants, only one carries the malloc/free shims

`nros-cpp`'s `heap_string.hpp` / `heap_sequence.hpp` allocate through the C-ABI
`nros_platform_malloc` / `nros_platform_free` (so C and C++ share one allocator).
Those names are the canonical surface in `nros-c/include/nros/platform.h:142,149`,
and the per-RTOS headers shim them over the lower-level alloc/dealloc — e.g.
`nros-c/include/nros/platform/freertos.h:109` defines
`static inline nros_platform_malloc → nros_platform_alloc` (and `free → dealloc`).

But the **CFFI** platform header,
`nros-platform-cffi/include/nros/platform.h:70,78`, declares **only**
`nros_platform_alloc` / `nros_platform_dealloc` — no `malloc`/`free` shims.

On a CFFI cell the compile line has both
`-I .../nros-platform-cffi/include` (plain `-I`) and
`-isystem .../nros-c/include`. `-I` is searched before `-isystem`, so
`#include <nros/platform.h>` resolves to **platform-cffi's** variant — the one
without the malloc/free shims. The heap headers' calls are therefore undeclared.

(threadx_linux is green because the host/linux threadx cpp path resolves the
nros-c platform.h that carries the shims; the baremetal CFFI cells hit the gap.)

## Fix direction

Add the two inline shims to `nros-platform-cffi/include/nros/platform.h`, mirroring
`freertos.h`:

```c
static inline void* nros_platform_malloc(size_t size) { return nros_platform_alloc(size); }
static inline void  nros_platform_free(void* ptr)     { nros_platform_dealloc(ptr); }
```

That keeps the canonical malloc/free C-ABI surface uniform across every platform
header, so nros-cpp's heap containers compile on CFFI platforms too. (Alternative:
point the heap headers at alloc/dealloc — rejected: malloc/free is the documented
canonical surface and every other platform header already exposes it.)

Same honest-red theme as [issue 0034] / [issue 0036]: the e2e lane now runs far
enough to expose pre-existing cpp-fixture compile gaps. Owner: nros-cpp /
platform-cffi.
