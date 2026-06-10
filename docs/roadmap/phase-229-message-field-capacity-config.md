# Phase 229 — Per-field message capacity configuration

**Goal.** Deliver the language-agnostic, per-field message capacity configuration
specified in **RFC-0033** — a `nros-codegen.toml` resolved once per codegen
invocation into a single `CapacityResolver` that feeds the Rust, C, and C++
generators identically, replacing the hardcoded `*_DEFAULT_SEQUENCE_CAPACITY` (64) /
`*_DEFAULT_STRING_CAPACITY` (256) constants. Closed the configuration half of issue
[0007-seq-capacity-64](../issues/archived/0007-seq-capacity-64.md); the `borrowed`
storage mode (phase 3 below) closed the issue entirely (Rust borrowed + C/C++ heap;
C/C++ borrowed views deferred to [issue 0021](../issues/0021-cpp-c-borrowed-views.md)).

**Status.** In progress (2026-06-10). **`owned` is complete across all three
languages (Rust/C/C++) with discovery + CLI/CMake wiring; `heap` (primitive
sequences) is done + verified on all three (Rust `alloc::Vec`, C rclc-style
malloc'd struct + `_fini`, C++ `nros::HeapSequence` + repr(C) FFI).** Remaining:
heap strings / seq-of-string-or-nested (C/C++ follow-up) and `borrowed` (229.6 —
its own multi-crate runtime effort, closes issue 0007). Design-of-record is RFC-0033.

Landed on branch `phase-229-message-field-capacity-config`:
`a6b30ca9` (1: config core) · `8f158447` (2: nros Rust) · `4b91423f` (3a: C) ·
`45877183` (3b: C++) · `415abe79` (4/229.3: discovery + activation) ·
`40aca32e` (5/229.5: heap Rust).

**Priority.** P2 — large sensor messages are unusable on embedded today, but bounded
`.msg` types and the raw-CDR API are working interim workarounds. Phase 1 (owned)
unblocks the common "size the buffer per app" need; phases 2–3 extend reach.

**Depends on.** RFC-0033 (design-of-record), RFC-0023 (codegen-workspace-discovery —
config discovery shares the workspace walk-up), RFC-0010 (zero-copy raw API — the
`borrowed` mode builds on the same CDR-buffer borrow), issue 0007 (the limitation
this resolves). Parser already models bounded vs unbounded fields
(`FieldType::{Sequence, BoundedSequence, String, BoundedString}`) — no parser change.

## Overview

Capacity is a **local storage** decision, invisible on the CDR wire, so the capacity
of an *unbounded* field is a free per-app choice with zero interop impact. Explicit
`.msg` bounds (`uint8[<=N]`, `string<=N`) are part of the rosidl type and stay
authoritative — configuration only fills in unbounded fields.

The configuration is one `nros-codegen.toml` (workspace + app scope, deep-merged),
resolved by precedence:

```
1. .msg explicit bound   2. [fields]   3. [types]   4. [packages]   5. [defaults]   6. built-in (64/256)
```

Each per-field value is `{ cap, mode }` (integer = `{ cap, mode = "owned" }`
shorthand); `mode ∈ owned | heap | borrowed`, default `owned`. One resolver, three
emitters — that is what makes it language-agnostic.

## Architecture

```
nros-codegen.toml (workspace root)  ─┐
nros-codegen.toml (app dir)         ─┴─► deep-merge ─► CapacityResolver
                                                           │  resolve(pkg, msg, field, kind) → {cap, mode}
                                                           ▼
      ┌─────────────── same FieldStorage per field ───────────────┐
      ▼                          ▼                                 ▼
  Rust emitter              C emitter                        C++ emitter
  heapless::Vec<T,N> /      [N] array /                      FixedSequence<N> /
  alloc::Vec / &'a [T]      ptr+len                          span / ptr+len
```

`.msg` bound short-circuits the resolver at level 1; bounded fields never call
`resolve`.

## Work Items

### 229.1 — `nros-codegen.toml` schema + loader  ✅ DONE
Parse and deep-merge the config. `packages/cli/rosidl-codegen/src/config.rs`.
- `[defaults]`, `[packages."pkg"]`, `[types."pkg/Msg"]` each take `sequence` /
  `string` keys; `[fields]` maps `"pkg/Msg.field"` → value.
- Value = integer (owned shorthand) or `{ cap, mode }` inline table; `mode` defaults
  to `Owned`. Unknown keys rejected (`deny_unknown_fields`).
- `StorageMode = Owned | Heap | Borrowed` (`+ is_phase1_supported()`).
- Deep-merge (`merged_with`): per-key for defaults/packages/types, app wins; `[fields]`
  entries atomic (app replaces workspace).
- **Files:** `packages/cli/rosidl-codegen/src/config.rs` (new),
  `packages/cli/rosidl-codegen/src/lib.rs` (export).

### 229.2 — `CapacityResolver` + precedence ladder  ✅ DONE
`resolve(package, message, field, kind)` → `FieldStorage { cap, mode }` by the
field > type > package > defaults > builtin ladder. Built-in `NROS_DEFAULT_*` are the
level-6 fallback (empty config == today's output). 9 unit tests green.
- **Files:** `packages/cli/rosidl-codegen/src/config.rs`.

### 229.3 — Config discovery + entry-point wiring  ✅ DONE
- `CapacityResolver::from_file` / `discover(start, stop)` (walk-up, root→leaf merge,
  closest wins) / `resolve_for(explicit, start, stop)` (explicit `--codegen-config`
  wins over discovered). `CODEGEN_CONFIG_FILENAME = "nros-codegen.toml"`. 3 unit tests.
- `generate_package` (rosidl-bindgen) takes `&CapacityResolver`; production paths
  build it: `generate_from_package_xml` (Rust) + `generate_c_from_package_xml`
  discover from the **manifest dir**; `generate_c_from_args_file` /
  `generate_cpp_from_args_file` (CMake JSON path) + the single-package + `ws` codegen
  paths discover from **output/source dir**.
- `--codegen-config <path>` flag on `nros generate` (multi-lang + rust); threaded
  through `GenerateConfig` / `GenerateCStandaloneConfig` / `GenerateCArgs`
  (`codegen_config` field).
- CMake `nros_generate_interfaces(... CODEGEN_CONFIG <path>)` → JSON arg.
- **Files:** `config.rs`, `rosidl-bindgen/src/generator.rs`,
  `cargo-nano-ros/src/lib.rs`, `nros-cli-core/src/cmd/{generate,ws}.rs`,
  `cmake/NanoRosGenerateInterfaces.cmake`.

### 229.4 — Wire resolver through the generators (owned)  ✅ DONE
Replace every direct `*_DEFAULT_SEQUENCE_CAPACITY` / `*_DEFAULT_STRING_CAPACITY`
reference for an **unbounded** field with `resolver.resolve(...)`. `mode = "owned"`
emits the resolved-`N` container (today's shape, parameterized). `heap`/`borrowed`
emit `GeneratorError::UnsupportedStorageMode` in phase 1.
- ✅ **nros (Rust) message path** — `nros_type_for_field_with_capacity` (types.rs);
  `field_to_nros_field{,_with_mode}` resolver-aware + `Result` + phase gate
  (common.rs); `generate_nros_message_package` / `generate_nros_inline_message`
  take a `&CapacityResolver` (msg.rs). 5 golden tests in `tests/nros_test.rs`
  (big-seq+small-string in one msg, type-level default, bounded short-circuit,
  empty==builtin, heap/borrowed error). Full cli workspace builds; 70+ green.
- ✅ **nros service/action** — builders updated to the new arity; pass
  `CapacityResolver::empty()` + ROS-convention message keys (`_Request`/`_Response`,
  `_Goal`/`_Result`/`_Feedback`) so behavior is unchanged and a real resolver drops
  in later. (srv.rs, action.rs)
- ✅ **C message path** — `c_type_for_field_with_capacity` +
  `c_array_suffix_for_field_with_capacity` (types.rs); `build_c_field` resolver-aware
  + `Result` + phase gate, overrides the unbounded `char[N]` suffix and the inline
  sequence struct `[N]` + `sequence_capacity` (common.rs); `generate_c_message_package`
  takes a `&CapacityResolver` (msg.rs). C service/action pass `empty()` + ROS keys.
  2 golden tests (`mod.rs`): big-seq+small-string, borrowed→error.
- ✅ **C++ message path** — `cpp_type_for_field_with_capacity` +
  `repr_c_type_for_field_with_capacity` (types.rs); `resolve_cap_override` helper
  resolves+gates once per field; `build_cpp_field` + `build_cpp_ffi_field` take the
  resolved `cap` so header `FixedString<N>`/`FixedSequence<…,N>` **and** the FFI repr
  `[u8; N]` + `sequence_capacity` + `string_capacity` all agree (common.rs);
  `cpp.rs::build_fields` + `generate_cpp_message_package` thread `&CapacityResolver`.
  C++ service/action pass `empty()` + ROS keys. 2 golden tests (`mod.rs`):
  header+FFI cap agreement, heap→error.
- ✅ **Serialized-size-max** — `compute_serialized_size_max` already scales with the
  resolved owned cap: top-level strings parse the cap from `repr_c_type` (`[u8; cap]`),
  sequences use the resolved `sequence_capacity`; heap fields contribute only the CDR
  length prefix (the heap publish path sizes the buffer at runtime). The remaining
  `CPP_DEFAULT_STRING_CAPACITY` uses are for *element* strings inside arrays/sequences,
  which are not per-field configurable. No change needed.
- ✅ **Activated** the production message paths (Rust/C/C++) via 229.3 discovery —
  `generate_package` + the C/C++ args-file paths now receive a discovered resolver.
- ✅ **Service/action** — all eight `generate_{nros,c,cpp}_{service,action}_package`
  (+ the nros inline variants) now take a `&CapacityResolver` and thread it into their
  request/response/goal/result/feedback field builders, keyed by the ROS-convention
  `<Service>_Request` / `<Action>_Goal` … names. Callers (rosidl-bindgen
  `generate_package`, cargo-nano-ros C/C++ paths) pass the discovered resolver. Test:
  `service_request_field_honors_capacity_config` (request `Vec<u8,4096>` + response
  `String<16>` from config).
- **Files:** `packages/cli/rosidl-codegen/src/types.rs`,
  `.../generator/{common,msg,srv,action}.rs`, `.../rosidl-bindgen/src/generator.rs`.

### 229.5 — `heap` storage mode  ✅ DONE (Rust + C + C++; sequences incl. of strings & nested, + strings)
`mode = "heap"` → growable containers (`cap` ignored — unbounded). Covers heap
primitive sequences, heap strings, **heap sequence-of-strings**, and **heap
sequence-of-nested-messages**. Sequence elements stay fixed-capacity (unbounded
*count*, bounded element) — a single-level heap allocation.
- **Rust** — `Vec<heapless::String<N>>` / `Vec<NestedMsg>` (already worked via the
  generic heap-vec deserialize; verified by `heap_compile_check`).
- **C** — element-typed heap struct (`char (*data)[N]` for strings, `NestedStruct*`
  for nested); deserialize does per-element `read_string` / `_deserialize_inline`;
  `_fini` calls each nested element's `_fini` before freeing the array. `gcc -Werror`
  verified (heap primitive seqs + heap string + heap `string[]` in one message).
- **C++** — `nros::HeapSequence<nros::FixedString<N>>` / `HeapSequence<NestedMsg>`
  (FixedString is `char[N]`, nested structs are POD → trivially copyable, safe in
  the raw-malloc'd HeapSequence). FFI heap serialize/deserialize gained string +
  nested element branches (`[u8; N]` slots / nested `*_fields` fns). Verified — FFI
  `.rs` `cargo check` + `.hpp` `g++` with heap seqs + heap string + heap `string[]`.
- **Issue 0021 retracted** — the suspected FFI repr mismatch was a wrong reading:
  `nros::FixedString<N>` is `char[N]` (no leading `size` field), so it already
  matches the Rust mirror `[u8; N]`. Fixed and heap seq-of-strings were both correct.
- **C++ caveat** — `HeapSequence<T>` frees its array via `nros_platform_free` but
  does not run element destructors, so a heap sequence whose **nested element type
  itself has heap fields** would leak the inner heap on drop. Nested messages without
  heap fields (the common case — `Point`, `Quaternion`, …) are unaffected. Tracked
  as a follow-up if a nested-heap-in-heap-seq case arises.
- ✅ **Rust path** — `nros-core` exposes `pub mod heap { pub use alloc::{String, Vec} }`
  under `any(feature="alloc", feature="std")` (the `extern crate alloc` cfg widened to
  match); `nros_type_for_field_heap` emits `nros_core::heap::{Vec<T>, String}`;
  `NrosField.is_heap` drives the deserialize template (growable `Vec::new()` + `push`
  with no `CapacityExceeded`, `String::from(&str)`). Serialize is unchanged (`.len()` /
  `.as_str()` / iteration work for both). Works in crate + inline modes.
- ✅ **Verified** — `tests/heap_compile_check.rs` (`--ignored`) generates a heap message
  (`uint8[] + string + string[] + int32`), drops it in a temp crate path-dep'd on real
  nros-core/nros-serdes, and `cargo check` passes. Plus `heap_mode_emits_alloc_containers`
  golden test.
- ✅ **C primitive sequences** — `c_type_for_field_heap` emits the rclc-style
  `{ T* data; size_t size, capacity; }`; deserialize mallocs via
  `nros_platform_malloc`; `<struct>_fini` frees; serialize shared. `gcc -Werror`
  verified (`tests/c_heap_compile_check.rs`). Heap strings / seq-of-string/nested
  still rejected.
- ✅ **C++ primitive sequences — runtime + FFI codegen done.**
  - `nros::HeapSequence<T>` (`packages/core/nros-cpp/include/nros/heap_sequence.hpp`) —
    RAII, non-copyable/movable, layout `{ T* data; size_t size; size_t capacity; }`
    (rclc shape, repr(C)-bridgeable), allocates via `nros_platform_malloc/free` so the
    SAME allocator spans the Rust↔C++ FFI.
  - `cpp_type_for_field_heap` → `nros::HeapSequence<elem>` (header); `CppStorage`
    enum threads owned-vs-heap; `SequenceStructDef.is_heap` + `CppFfiField.{is_heap,
    element_repr_type}` → Rust mirror `#[repr(C)] { data: *mut T, size: usize,
    capacity: usize }`.
  - FFI serialize: raw-pointer `*data.add(i)`. FFI deserialize: `nros_platform_malloc`
    + populate + **free on mid-loop read error** (no leak); `extern "C"` malloc/free
    decls gated on `has_heap`.
  - **Publish path**: heap messages serialize into a `nros_platform_malloc`'d buffer
    sized from the runtime sequence lengths (`serialized_size_max + Σ(16 + size·elemsz)`),
    not the fixed stack array; freed after publish.
  - **Verified** — `tests/cpp_heap_compile_check.rs` (`--ignored`): the FFI `.rs`
    `cargo check`s against real nros-serdes **and** the `.hpp` (`nros::HeapSequence`)
    `g++ -std=c++14 -fno-exceptions -fno-rtti`s. + 2 golden tests.
  - Heap strings / seq-of-string/nested stay rejected (as in C).
- **Files:** `nros-core/src/lib.rs`, `types.rs`, `templates.rs`,
  `generator/common.rs`, `templates/message_nros.rs.jinja`, `tests/`.

### 229.6 — `borrowed` storage mode (phase 3, closed issue 0007)  ✅ Rust done; C/C++ → issue 0021
**Status (2026-06-10).** The **Rust** path is complete and E2E-validated.
**Issue 0007 is resolved**: large payloads are representable on every target —
allocator targets via `heap` (all 3 langs, 229.5), allocator-free targets via
`borrowed` (Rust, this section). The remaining **borrowed views for C/C++** are an
alloc-free optimization (C/C++ already have `heap`), tracked as
[issue 0021](../issues/0021-cpp-c-borrowed-views.md).

- ✅ **Runtime seam** (`670a62a4`): `nros_serdes::DeserializeBorrowed<'a>` +
  `nros_core::BorrowedMessage` GAT marker (`type View<'a>`) + executor
  `SubBufferedBorrowedEntry` / `sub_buffered_borrowed_try_process` (triple-buffer
  only; depth>1 → `Unsupported`) + `NodeCtx::create_subscription_borrowed::<B,_>`
  (uses `KEEP_LAST(1)`).
- ✅ **Rust codegen** (`5097a7a7`): `mode = "borrowed"` emits `{Msg}View<'a>`
  (borrowed fields `&'a [u8]`/`&'a str`, others copied) + `{Msg}Borrow` ZST marker,
  alongside the unchanged owned `{Msg}` (additive — owned still publishes).
- ✅ **Alignment guard** (`40e5c97e`): multi-byte numeric sequences
  (`float32[]`, `uint16[]`, …) → `nros_core::LeSliceView<'a, T>` — borrows raw LE
  bytes zero-copy, decodes per element (no `&[T]` cast → no buffer-alignment
  requirement). Single-byte (`uint8`/`int8`/`bool`) stay true `&'a [u8]`. Sequences
  of strings/nested rejected (no fixed-width span).
- ✅ **E2E** (`aeed3d4d`): owned-publish-wire → borrowed-subscribe through the
  MockSession executor `spin_once` — `ImageView` with `&[u8]` + `LeSliceView<f32>`
  decodes correctly.
- ⬜ **C/C++ span views** (slice 5): `{const T* data; size_t size}` over the
  existing raw `(data,len)` callback. Deferred to [issue 0021](../issues/0021-cpp-c-borrowed-views.md)
  (alloc-free C/C++ optimization; issue 0007 already closed via Rust borrowed +
  C/C++ heap).

Original design (the runtime seam was proven before codegen leaned on it):

1. **Callback-scoped buffer borrow (the substantive work).** Today's callback dispatch
   (`nros-node executor/arena.rs::sub_buffered_try_process`) deserializes into an owned
   `M` then calls `FnMut(&M)`. Change it to hold the existing `sub_borrow` view
   (`RecvView<'a>`, already used on the polling path) **across** the callback and
   release after, calling `FnMut(&Msg<'a>)`. Restrict borrowed subscriptions to the
   **triple-buffer** strategy (queue depth ≤ 1); reject depth > 1 (SPSC ring holds
   several in flight → no single well-defined view).
   - **Files:** `nros-node/src/executor/{arena.rs, spin.rs, node.rs, handles.rs}`.
2. **Borrowed-view deserialize.** A pass that walks the CDR fields recording
   `(offset, len)` per borrowed field and materialises `Msg<'a>` with `&'a [u8]` /
   `&'a str` slices (no copy). Owned/heap fields in the same message still copy.
   - **Files:** generated message code + `nros-serdes` (a borrow-aware reader).
3. **Codegen — lifetime-carrying types.** `mode = "borrowed"` emits `struct Image<'a>`
   with `&'a [T]` / `&'a str` fields (Rust), `{ const T* data; size_t size; }` views
   (C/C++). Replaces the phase-1 `UnsupportedStorageMode` error for `borrowed`.
   - **Files:** `types.rs`, `generator/{common,msg,cpp}.rs`, the message templates.
4. **Alignment guard.** `&[u8]` is unconditional; for `&[T]` where `T` needs > 1-byte
   alignment (`f32`/`f64`/…), emit a runtime alignment check on
   `buffer_base + field_offset` with a fallback (copy into a scratch / degrade that
   field to owned) on strict-alignment targets.
5. **C/C++ surface.** The C/C++ callbacks already receive raw `(data, len)`
   (`nros-c subscription`); the borrowed type is a typed ptr+len accessor over that
   same callback — no new ABI, just generated views + a span-like C++ wrapper.

- ✅ Done for Rust: issue 0007 set `status: resolved`, `resolved_in: Phase 229`,
  moved to `docs/issues/archived/`. C/C++ borrowed views tracked separately as
  [issue 0021](../issues/0021-cpp-c-borrowed-views.md).
- **Out of scope (by design):** `take()`/polling returning a borrowed message, storing
  a borrowed message past the callback, and any publish-side borrow (that is the
  Phase 124 `pub_loan` loan API).

## Acceptance

- **Resolver unit tests** — precedence ladder (field > type > package > defaults >
  builtin), integer-shorthand expansion, bounded-field short-circuit, deep-merge of
  workspace + app files.
- **Golden codegen tests** — a fixture `.msg` set + `nros-codegen.toml` produce the
  expected Rust/C/C++ types; assert two unbounded fields in one message resolve to
  different capacities (big sequence + small string).
- **No-config regression** — empty/absent config reproduces current generated output
  byte-for-byte (locks against default drift).
- **Compat** — a `.msg` with explicit bounds plus a conflicting `[fields]` entry: the
  bound wins; the entry is ignored (verified by `bounded_field_ignores_config`). The
  *warning* on a stale bounded-field entry (RFC-0033 open question 1) is deferred — the
  behavior is correct; a library `eprintln` is left out as low-value noise.
- **Phase gating** — phase-1 build emits a clear error for `borrowed`; `heap` lifted
  in 229.5, `borrowed` in 229.6.
- `just ci` green.

## Notes

- Phasing maps to RFC-0033 storage modes: **P1** 229.1–229.4 (config method +
  `owned`, full language-agnostic surface, immediately useful), **P2** 229.5 (`heap`),
  **P3** 229.6 (`borrowed`, closes issue 0007).
- Design rationale + alternatives live in RFC-0033 — this doc is the work breakdown
  only. Resolve RFC-0033 open questions (1: warn-vs-error on bounded entry; 2:
  inline-table at package/type levels; 3: borrowed lifetime threading) before flipping
  the RFC to `Stable` (with the matching ARCHITECTURE.md update in the same commit).
