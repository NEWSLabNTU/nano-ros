---
id: 226
title: "C++ ParameterServer sequence storage is a full engine implemented in the header — C1 thin-wrapper violation, Rust core has no counterpart to own it"
status: open
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
