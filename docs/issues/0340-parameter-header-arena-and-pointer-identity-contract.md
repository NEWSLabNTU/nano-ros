---
id: 340
title: "#226 recurrence: ParameterServer still holds array storage in a header-side bump arena and recovers capacity via reinterpret_cast<const uint64_t*>(ptr)[-1] — an undocumented pointer-identity contract with the C server"
status: open
type: bug
severity: medium
area: core
related: [issue-0226, issue-0329]
---

## Finding (deep audit C,E 2026-07-28 — C1)

`packages/core/nros-cpp/include/nros/parameter.hpp:559` — `ParameterServer` keeps
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
