---
id: 51
title: Zephyr cxx-compat shims unconditionally shadow the toolchain's real libstdc++ headers
status: resolved
type: bug
area: zephyr
related: [issue-0042, rfc-0042]
resolved_in: ASI FVP workspace-mode bring-up (zephyr/cxx-compat/* — #include_next defer)
---

Same fragility class as [[issue-0042]] (#36 was the NuttX `<cstdlib>`
`#include_next` libc clash) — a C++ std-header clash between the toolchain and a
nano-ros shim, this time on a target that **does** ship a full libstdc++.

## Symptom

A downstream Zephyr+C++ consumer (Autoware Safety Island, board
`fvp-aemv8r-smp`, toolchain `aarch64-zephyr-elf` gcc 12.2 + picolibc) failed to
compile vendored C++ (Autoware) with a cascade that *looked* like a vendored
portability bug:

```
zephyr/cxx-compat/cstdlib:7:9: error: 'abort' has not been declared in '::'
    7 | using ::abort;
…and the cascade it triggers in libstdc++ internals:
bits/stl_uninitialized.h:167: error: no type named 'value_type' in
  'struct std::iterator_traits<__normal_iterator<const std::list<Vector2d>*, …>>'
bits/vector.tcc:472: error: 'constexpr' call flows off the end of the function
```

## Root cause

`zephyr/cxx-compat/` ships stripped `<cstdlib>`/`<cstdio>`/`<cstring>`/`<atomic>`/
`<chrono>`/`<type_traits>`/`<utility>`/… and is put on the include path via
`zephyr_include_directories` for **every** Zephyr cpp build (the original
PICOLIBC gate was removed as "over-conservative"). On a target with a complete
libstdc++:

1. libstdc++'s own `c++/12.2.0/stdlib.h` does `#include <cstdlib>`, which
   resolves to the nano-ros shim (cxx-compat is on `-I`).
2. The shim does `#include <stdlib.h>` — but we are **already inside**
   libstdc++'s `<stdlib.h>` wrapper (its guard is set), so this is a no-op and
   the real C `<stdlib.h>` is never reached.
3. The C names never land in `::`, so `using ::abort;` fails. Under picolibc,
   `<stdlib.h>` with `_HAVE_STD_CXX` places the C names in `namespace std`
   (`_BEGIN_STD_C` = `namespace std { extern "C" {`), not `::` — so the glibc-
   style `using ::abort;` re-export was wrong for this libc regardless.
4. Beyond `<cstdlib>`, the **minimal** shims (`<atomic>` with only
   `atomic<bool>`/`atomic<int64_t>`, partial `<chrono>`/`<type_traits>`/
   `<utility>`) shadow the real complete headers — starving any real C++ TU
   (vendored Autoware) of the full surface, which is what produced the
   `iterator_traits`/`constexpr-flows-off-end` cascade.

The shims were authored for Zephyr configs whose C++ stdlib layer is minimal
(`zephyr/lib/cpp/minimal/include` carries only `<cstddef>`/`<cstdint>`/`<new>`;
native_sim lacks `<cstdio>`). They are correct *there* and wrong wherever a real
libstdc++ is reachable.

## Fix

Each shim now defers to the toolchain's real header when one is reachable, and
falls back to its minimal body only when none exists:

```cpp
#if defined(__has_include_next) && __has_include_next(<cstdlib>)
#  include_next <cstdlib>
#elif !defined(NROS_ZEPHYR_CXX_COMPAT_CSTDLIB)
#define NROS_ZEPHYR_CXX_COMPAT_CSTDLIB
   …minimal fallback…
#endif
```

`#include_next` chains correctly even though cxx-compat is on the include path
twice (the Zephyr module adds it via both `zephyr/CMakeLists.txt` and
`nros_rmw_cyclonedds.cmake`): the fallback body is guard-protected, the
`#include_next` is not, so the real self-guarded libstdc++ header is reached.

Self-configuring: where the toolchain has libstdc++ (`aarch64-zephyr-elf`,
native_sim host gcc) the real header wins; where Zephyr restricts the C++
include set the shim still fills the gap. Verified: the exact failing pattern
(`std::vector<std::list<Vector2d>>` copy + `std::abort` through the doubled
cxx-compat path) compiles clean on `aarch64-zephyr-elf`.

## Residual / follow-up

This is the **same root class** as [[issue-0042]] point 4 ("two libc header
sets reachable per TU, precedence re-wired at every entrypoint"). The structural
fix (one libc-precedence helper; declare-once board capabilities) would also
cover whether cxx-compat belongs on the path for a given board's libc/libcpp
choice — better than the current "always on, defer at the header." Tracked under
RFC-0042 / phase-241.
