---
id: 201
title: "C++ HeapSequence<T> never runs element destructors — heap fields inside a heap sequence's nested elements leak (C and Rust are safe)"
status: resolved
type: bug
severity: low
area: cpp
related: [rfc-0033, phase-229, phase-235]
---

## Problem

`nros::HeapSequence<T>` (`packages/core/nros-cpp/include/nros/heap_sequence.hpp`)
manages its element array as a raw `nros_platform_malloc/free` block:

```cpp
~HeapSequence() { nros_platform_free(data); }   // line 49 — no element dtors
```

That is correct for the intended element set (PODs: primitives,
`nros::FixedString<N>`, heap-field-free nested messages — the phase-229.5
design says "sequence elements stay fixed-capacity ... a single-level heap
allocation"). But **nothing enforces the single-level constraint**: the
capacity config resolves each message type independently, so

```toml
[fields."pkg/Outer.items"]  = { cap = 0, mode = "heap" }   # Inner[] items
[types."pkg/Inner"]
sequence = { cap = 0, mode = "heap" }                      # Inner has heap fields
```

generates `nros::HeapSequence<pkg::msg::Inner>` whose elements each contain
their own `HeapSequence` members. Three leak sites, verified by reading:

1. **Destructor** (`heap_sequence.hpp:49`) — frees the outer array only; each
   element's inner `HeapSequence` destructor never runs → inner arrays leak.
2. **Move-assign** (`heap_sequence.hpp:59-61`) — same shape: overwritten
   `data` freed without element teardown.
3. **FFI deserialize error path** — the generated Rust glue
   (`templates/message_cpp_ffi.rs.jinja` ~line 209) frees the outer heap
   array on a mid-loop read error, but inner allocations made by
   already-deserialized elements' nested deserializers are stranded (the
   repr(C) mirror is raw pointers; no Drop).

## Why low severity

- **C is unaffected** — the generated `<struct>_fini` recurses into each
  nested element's `_fini` before freeing the array (229.5 did this
  deliberately).
- **Rust is unaffected** — `alloc::Vec` drops elements.
- The common nested payloads (`Point`, `Quaternion`, headers…) have no heap
  fields; hitting the leak requires an explicit two-level heap config that no
  in-tree example or test uses today.

## Fix directions (pick one)

1. **Reject at codegen** (cheapest, honest): when a C++ heap sequence's
   nested element type resolves any field to `mode = "heap"`, emit
   `GeneratorError::UnsupportedStorageMode` with a message naming both
   levels — mirrors how heap strings were staged in 229.5 before support
   landed. Requires the resolver to expose "does type X have heap fields"
   (it already resolves per-type; a lookup pass over the element's fields
   at generation time suffices).
2. **Run element destructors**: `~HeapSequence` (+ move-assign) loops
   `data[i].~T()` before `nros_platform_free` when
   `!std::is_trivially_destructible<T>::value` (C++14-compatible via
   `nros::conditional` or a simple `if` — the trait is constexpr). Also fix
   the FFI error path (call the nested `_fini`-equivalent teardown or accept
   the leak-on-error as documented). This makes the two-level config work
   outright but adds dtor semantics to a struct the FFI treats as POD —
   audit the Rust mirror's assumptions before choosing this.

Option 1 now, option 2 if a real two-level use case arrives, is the
recommended sequence.

## History

Caveat first recorded in phase-229 § 229.5 ("tracked as a follow-up if a
nested-heap-in-heap-seq case arises"); promoted to a filed issue during the
2026-07-16 phase-229 completion audit so it can't silently vanish with the
archived phase doc.


## Resolution (2026-07-16) — option 2 implemented: real element lifetime + FFI teardown

All three leak sites fixed, plus a latent API gap found while implementing:

1. **`HeapSequence<T>` element lifetime** (`heap_sequence.hpp`): destructor,
   move-assign, and `clear()` now run `destroy_elements()` — a pseudo-destructor
   loop (`data[i].~T()`, valid for scalars too, compiles to nothing when `T` is
   trivially destructible; no `<type_traits>` needed). `reserve()` switched from
   per-element copy-assign (ill-formed for owning element types — deleted copy)
   to BYTE RELOCATION with a documented trivially-relocatable element contract
   (standard-layout, self-contained pointers — same contract the C runtime's
   sequence realloc assumes). `push_back` placement-new copy-constructs (the old
   assign-into-uninitialized was latent UB for copyable owning types), and a new
   `emplace_back()` placement-new default-constructs in place — the legitimate
   way to BUILD sequences of owning elements (the old API could not: the
   two-level shape only ever came in via the FFI).
2. **Generated FFI teardown** (`message_cpp_ffi.rs.jinja` + generator): every
   generated module now carries `teardown_<pkg>_msg_<name>_fields(&mut repr_c)`
   — the Rust-glue analog of the C `_fini` recursion (frees + NULLS heap
   strings/sequences, recurses into nested elements; empty and inlined away for
   heap-free messages; emitted unconditionally so no transitive-heap analysis is
   needed). Field contexts gained `nested_teardown_fn` mirroring the
   `nested_deserialize_fn` naming.
3. **Deserializer error paths** (both the owned and borrowed bodies): heap
   nested-element buffers are now ZERO-INITED after malloc, so a mid-loop error
   can tear down partially deserialized elements (untouched heap pointers read
   null); the nested error arm tears down elements `0..=i` before freeing the
   outer buffer. Fields already published into `out` stay owned by the C++
   message's (now recursive) destructor — verified all three deserialize call
   sites stack-construct a fresh message.

Tests: `test_cpp_ffi_teardown_fn` + extended `test_cpp_heap_seq_of_nested`
(generation-level, rosidl-codegen) and a RUNTIME lifetime probe
(`nros-cpp/tests/compile/heap_sequence_lifetime.cpp`, compiled AND executed by
`just check-cpp` with a counting allocator) covering dtor / move-assign /
clear / reserve-relocation of a two-level heap element shape — zero live
allocations after each. The old container does not even compile the probe
(no `emplace_back`), confirming the API gap.
