---
id: 578
title: "A `0` size probe emits the most under-sized opaque macro there is, and only a build-script warning says not to link it"
status: resolved  # not a live hole — the path is already closed at COMPILE time
type: bug
area: build
related: [issue-0472, issue-0464, issue-0360]
---

## What this claimed

Split out of issue 0472's item 2 on 2026-08-15. The claim: when the size probe
returns `0`, `nros-build-helpers` emits the smallest possible opaque width and
only *warns*

> `EXECUTOR_SIZE probe returned 0 … The emitted CPP_EXECUTOR_OPAQUE_U64S will be
> 1; do not link the resulting rlib.`

so "nothing stops that artifact reaching a link", and the fix was to poison it
with 0360's variant-symbol mechanism.

**I filed that from 0472's text without checking it. It does not hold.**

## What is actually true — three mechanisms already close it

1. **No header is written, and the stub `#error`s.** On `probe_executor == 0`,
   `generate_config` returns early *before* writing the per-build header
   (`c.rs`). A C/C++ consumer then resolves `<nros/nros_config_generated.h>` to
   the committed stub, whose body is
   `#error "nros_config_generated.h must be supplied per-build by the build
   system…"`. So an unprobed build fails at COMPILE time for every C/C++
   consumer — strictly earlier than the proposed link-time failure. The `1`
   never reaches a C caller's `_opaque`.

2. **The `1` has no Rust consumer either.** `nros-c`'s `config` module is
   `#[cfg(all(not(cbindgen), feature = "rmw-cffi"))]`, and probe-zero is the
   no-`rmw-cffi` case. Both executor assertions (`nros-c`'s and `nros-cpp`'s)
   are `rmw-cffi`-gated too, which is also why they do not fire spuriously on a
   check run. An artifact built without `rmw-cffi` contains no RMW code and no
   consumer of these sizes.

3. **The fat-LTO case in the docstring is historical.** `c.rs` still documents
   "the workspace's fat-LTO release profile … the probe returns 0 for every
   entry", which would have made a blanket link-poison break real release
   builds. Measured 2026-08-15: `cargo build -p nros-c --release
   --no-default-features --features std,rmw-cffi,platform-posix,ros-humble`
   succeeds with **zero** `probe returned 0` warnings. `Cargo.toml` explains
   why — the probe was reworked to read "byte-identical sizes under
   `lto = "fat"` on host and `thumbv7m-none-eabi`".

So the proposed fix would have added a mechanism for a hole that is already
closed, against a risk (breaking LTO builds) that was real when the docstring
was written and is not now.

## What IS worth attention, and is not this

`nros_config_generated.h`'s stub has an `#else` arm:

```c
#if defined(NROS_PLATFORM_NUTTX)
#include "nros/nros_config_generated_nuttx.h"
#else
#error …
#endif
```

That committed NuttX header bypasses the probe entirely, and it is the file
issue **0464** caught rotted ~11 % low — found by the executor's assertion,
which is the same guard family issue 0472 has now extended to the other nine
macros. A committed snapshot going stale is a different mechanism from an
absent probe, and it already has an owner.

## Disposition

Resolved as not-a-live-hole. The stale part is the WARNING's wording — "do not
link the resulting rlib" describes a danger the `#error` stub and the `rmw-cffi`
gating already prevent — and the stale docstring about fat-LTO probing zero.
Neither is worth a mechanism; both are worth not being believed the next time
someone reads them, which is what this issue is now for.
