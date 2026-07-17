---
id: 226
title: "C++ ParameterServer sequence storage is a full engine implemented in the header — C1 thin-wrapper violation, Rust core has no counterpart to own it"
status: resolved
type: tech-debt
area: cpp
related: [issue-0201]
---

## Finding (deep audit 2026-07-17, C1)

`packages/core/nros-cpp/include/nros/parameter.hpp:459` —
`ParameterServer<Cap,SeqSlots,SeqPoolBytes>` implements the sequence-
parameter storage engine (SeqRecord table, bump-pool allocator, name
lookup/copy/compare) entirely in the C++ header. Phase-242.3 landed it
there deliberately ("sequences are C++-storage-local, do not cross the
FFI"), but that leaves the C++ layer OWNING behavior with no Rust-core
counterpart: C and Rust users cannot get sequence params, and the storage
semantics live outside the tested core.

## Fix sketch

Design decision first: either promote sequence-parameter storage into the
Rust core with an FFI that doesn't dangle (the reason it was header-local),
or document the C++-only carve-out as an accepted RFC-0044 deviation and
close. Don't leave it implicit.

## Resolution (2026-07-17) — promote the records, keep the pool

The design decision: neither full promotion nor a carve-out. The C param
API is a caller-owns-storage model (even scalars live in the wrapper's
`storage_[Capacity]`), so a C++ byte pool is CONSISTENT with the FFI —
what violated C1 was the parallel RECORD ENGINE (name table, lookup,
type discriminator). Fix:

- Element bytes stay in the inline pool (the stable owner the
  borrow-semantics C array FFI requires), each allocation prefixed by a
  uint64 capacity header so `set_parameter` bounds-checks without a
  record table.
- Records (name → type → ptr/len) move to the C/Rust server via the
  existing `nros_param_{declare,get,set}_{double,integer,bool}_array`
  FFI (declared locally — the macro-generated fns are absent from the
  cbindgen header). Duplicate-name, existence, and element-type checks
  all happen in the server now.
- Deleted: SeqRecord table, name_eq/name_copy/find_seq, the SeqKind
  discriminator. `SeqSlots` template param retained for source
  compatibility, documented as unused.
- BONUS: sequence parameters are now visible through `raw()` — the
  param services / `ros2 param list` can see them (pre-#226 they were
  C++-local and invisible).

Verified: cpp parameters example builds + live roundtrip green
(`mpc_weights[0]=4.0 n=4` read back after set); component-node-poc and
ws-params-cpp (the 242.7 facade consumers) build clean.
