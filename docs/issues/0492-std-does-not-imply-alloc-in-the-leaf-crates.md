---
id: 492
title: "`std` implies `alloc` in half the stack and not the other half — the default feature state cannot serialize a heap message field"
status: open
type: bug
area: build
related: [rfc-0033, rfc-0005, rfc-0006, issue-0493, phase-345]
---

## The contradiction

Two spellings of "an allocator is available" coexist in the workspace and they
disagree about whether `std` turns the other one on.

`std` **does** imply `alloc`:

| crate | declaration |
| --- | --- |
| `nros` | `std = ["alloc", …]` |
| `nros-node` | `std = ["alloc", …]` |
| `nros-rmw-zenoh` | `std = ["alloc", …]` |
| `nros-c` | `std = ["alloc", …]` |

`std` does **not** imply `alloc`:

| crate | declaration |
| --- | --- |
| `nros-core` | `std = ["nros-serdes/std"]`, `alloc = ["nros-serdes/alloc"]` |
| `nros-serdes` | `std = []`, `alloc = []` |
| `nros-rmw` | `std = ["nros-core/std", "log"]`, `alloc = ["nros-core/alloc"]` |
| `nros-params` | `std = []`, `alloc = []` |
| `nros-platform` | `std = []`, `alloc = []` |

The source layer does not follow the Cargo layer. `nros-core/src/lib.rs:19`
gates the `alloc` crate — and `heap::{Vec, String}`, the RFC-0033 `mode =
"heap"` re-export — on `any(feature = "alloc", feature = "std")`, i.e. it
treats `std` as implying `alloc`. `nros-serdes` does not: `primitives.rs:278`
gates `mod alloc_impl` on `feature = "alloc"` alone.

So `nros-core/std` forwards `std` to `nros-serdes` and **stops there** — the
sibling never gets `alloc`.

## The reproducer

`nros-core`'s default feature set is `["std"]`. A user who writes the natural
thing gets it:

```toml
[dependencies]
nros-core   = { path = "…/nros-core" }    # default = ["std"], no alloc
nros-serdes = { path = "…/nros-serdes" }
```

```rust
use nros_core::heap::Vec as HeapVec;      // exists: gated any(alloc, std)
use nros_serdes::traits::Serialize;

pub fn f(v: &HeapVec<u32>, w: &mut nros_serdes::cdr::CdrWriter) {
    v.serialize(w).unwrap();
}
```

```
error[E0599]: no method named `serialize` found for reference `&std::vec::Vec<u32>` in the current scope
```

`cargo tree --format "{p} {f}"` on that graph confirms the cause — `nros-serdes
v0.5.0 default,std`, no `alloc`, so `alloc_impl` is not compiled. The type a
generated `mode = "heap"` field names is reachable; its serializer is not.

`String` hides the same hole: `v.serialize(w)` on a `String` auto-derefs to the
ungated `impl Serialize for str` (`primitives.rs:198`) and compiles. Only `Vec<T>`,
which has no ungated deref target, surfaces it. That is why this has not been
caught — the failure is per-type, not per-build.

## Scope check — what is *not* broken

Every single-feature build compiles clean today:

```
nros-core     --no-default-features --features std    -> 0 errors
nros-core     --no-default-features --features alloc  -> 0 errors
nros-serdes   --no-default-features --features std    -> 0 errors
nros-rmw      --no-default-features --features std    -> 0 errors
nros-params   --no-default-features --features std    -> 0 errors
nros-platform --no-default-features --features std    -> 0 errors
```

Nothing in-tree hits the bad state, because every in-tree consumer reaches
`nros-core` through `nros` or `nros-node`, and both of those spell `std =
["alloc", …]`. The hole is only reachable from **outside** — a user crate
depending on `nros-core` / `nros-serdes` directly, which is exactly the shape
the generated message crates have (`nros-builtin-interfaces` et al. forward
`std = ["nros-core/std", "nros-serdes/std"]` and declare no `alloc` feature at
all).

## Dead declarations found alongside

Features declared in `[features]` with zero `cfg` sites in `src/`/`build.rs`
and no forwarding to a dependency — enabling them is a no-op the manifest
advertises as a knob:

- **`nros-platform/alloc`** — 0 `cfg` sites, forwards nowhere. Its only effect
  is being implied by `global-allocator = ["alloc"]`. A user who enables
  `nros-platform/alloc` expecting an allocator gets nothing.
- **`nros-rmw-cyclonedds/std`** — 0 `cfg` sites, no forwarding.

(The `std` features on the nine generated interface crates are pass-through
only, which is correct — they forward to `nros-core`/`nros-serdes`. Not dead.)

## Direction

One rule, stated once, for every crate that can build `no_std`:

1. **`std = ["alloc", …]` everywhere.** There is no supported configuration
   where the standard library is present and the allocator is not, and the
   source layer already assumes this. Make Cargo agree.
2. **`alloc` is the real axis.** `std` is `alloc` plus OS services (clock,
   sockets, `std::error::Error`). Document that in one place and have the
   per-crate feature tables point at it, rather than each crate re-deriving it.
3. **Delete or wire the dead declarations** — `nros-platform/alloc`,
   `nros-rmw-cyclonedds/std`.
4. **Gate it.** A `check-feature-contract` script asserting, over every
   workspace member: if a crate declares both `std` and `alloc`, then `std`
   lists `alloc`; and every declared `std`/`alloc` feature either has a `cfg`
   site or forwards to a dependency.

Item 1 is a behaviour change for out-of-tree consumers on the `std`-without-`alloc`
path — that path is the bug, so the change is the fix, but it belongs in a
release note. See phase-345 for sequencing, and issue 0493 for the second half
of the same manifest problem (the `default` feature splitting compile units).
