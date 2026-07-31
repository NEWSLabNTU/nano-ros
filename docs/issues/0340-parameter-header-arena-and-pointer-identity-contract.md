---
id: 340
title: "#226 recurrence: ParameterServer still holds array storage in a header-side bump arena and recovers capacity via reinterpret_cast<const uint64_t*>(ptr)[-1] — an undocumented pointer-identity contract with the C server"
status: resolved
type: bug
severity: medium
area: core
related: [issue-0226, issue-0329]
---

## Finding (deep audit C,E 2026-07-28 — C1)

`packages/api/nros-cpp/include/nros/parameter.hpp:559` — `ParameterServer` keeps
array-parameter storage in a hand-rolled bump arena **inside the public header**:
`seq_pool_`, `align_up`, and an out-of-band `uint64_t` capacity word written
immediately before each block. On `set` it recovers that capacity by reading

```cpp
reinterpret_cast<const uint64_t*>(cur)[-1]
```

off a pointer handed back by `nros_param_get_*_array`.

Two problems:

1. **Bookkeeping state in the shim** (C1): allocation policy and capacity tracking
   are behavior, not type adaptation, so they belong behind the CFFI.
2. **An undocumented pointer-identity contract**: the `[-1]` read only works because
   the pointer the C server returns is *exactly* the pointer the header handed it,
   with the header's own header-word still sitting in front of it. Nothing in
   `nros/parameter.h` states that guarantee. The moment the Rust side starts copying
   arrays, returning an interior pointer, or handing back server-owned storage, this
   reads whatever precedes the buffer — a silent out-of-bounds read of adjacent heap.

## Why this is filed rather than folded into #226

Issue **#226** ("C++ param sequence storage in the header") is **resolved and
archived** — it removed the parallel `SeqRecord` storage engine. The arena came back
in a different shape for array parameters. This is the class recurring after a fix,
which is the pattern named in `CLAUDE.md` ("Fix the CLASS, not the reported site")
and in the shared callout on #326/#328.

## Fix

Move the capacity to where the record lives:

- add a `capacity` field to `nros_param_array_t`, or a
  `nros_param_get_*_array_capacity()` accessor, so the C++ side bounds-checks against
  **server-owned** state and the `[-1]` read disappears;
- keep only the caller-owned byte arena in the header, which the borrow-semantics FFI
  genuinely requires;
- failing that (if the FFI shape cannot change now), **document the pointer-identity
  guarantee in `nros/parameter.h`** so the Rust side cannot start copying arrays
  without knowingly breaking the header.

Confidence note: the finder rated this medium confidence and the report-only tier did
not refute it; the lead confirmed the `[-1]` read and the arena members exist as
described, but did **not** trace every `nros_param_get_*_array` implementation to
prove the returned pointer is always the caller's. If it turns out one backend
already copies, this is a live bug rather than a latent contract.

## Resolved (2026-07-28) — option 3, with the hazard made loud

### Why not the preferred fix

The issue's first choice is to carry the capacity in server-owned state (a
field on `nros_param_array_t`, or a `nros_param_get_*_array_capacity()`).
Checked, and it is a bigger change than the issue assumes:

`nros_param_array_t` is `{ data, len }` — no capacity — and
`nros_param_declare_*_array` takes BORROWED storage: the server records the
caller's pointer verbatim and never copies. So the server does not *have* a
capacity to report; giving it one means changing the declare signature, which
is a breaking public C ABI change to a **hand-mirrored FFI struct** — precisely
the drift class that has bitten three times (CLAUDE.md, "Hand-mirrored FFI
structs drift on append"). Not something to land quietly inside a medium-
severity cleanup.

### What was done

**The dangerous half is fixed.** The `[-1]` read is now guarded by a
pointer-provenance check: `set_seq_impl` verifies `cur` lies inside this
header's own `seq_pool_` before trusting the word in front of it. If the C side
ever starts copying arrays, returning an interior pointer, or handing back
server-owned storage, the result is a loud `NROS_RET_INVALID_ARGUMENT` instead
of a silent out-of-bounds read of adjacent memory — which was the actual
danger.

The bound is inclusive at the top on purpose: a zero-capacity array declared at
the end of the pool yields `dst == seq_pool_ + SeqPoolBytes` (`base == end`),
a legitimate block whose capacity word still precedes it. An exclusive bound
would have rejected it — a regression the guard nearly introduced.

**The contract is now written down** where the party who could break it will
read it: a doc block in `nros/parameter.h` states that `get` returns the very
pointer `declare`/`set` was given, that `parameter.hpp` depends on this, and
that changing it requires changing `parameter.hpp` in the same commit. It also
records the proper fix and why it was deferred.

### Not fixed — stated plainly

Problem 1 (bookkeeping state living in the shim) **remains**. The bump arena,
`align_up` and the out-of-band capacity word are still in the public header.
Only the memory-safety hazard was removed; the layering complaint stands, and
closing it needs the ABI change above.

### Coverage gap found while verifying

**No in-tree C++ source calls the array declare/set API at all** — a grep for
`declare_integer_array` / `declare_double_array` / `declare_bool_array` across
`examples/` and `packages/testing/` returns nothing, and
`tests/cpp_parameters.rs` does not exercise arrays. So this arena has zero
runtime coverage, which is how a `[-1]` read survived a resolved issue on the
same class. The guard above is correct by construction (`declare_seq_impl`
always returns `seq_pool_ + hdr + 8` with `end <= SeqPoolBytes`), but nothing
would catch a regression.
