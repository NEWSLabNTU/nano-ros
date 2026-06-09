# Phase 229 — Per-field message capacity configuration

**Goal.** Deliver the language-agnostic, per-field message capacity configuration
specified in **RFC-0033** — a `nros-codegen.toml` resolved once per codegen
invocation into a single `CapacityResolver` that feeds the Rust, C, and C++
generators identically, replacing the hardcoded `*_DEFAULT_SEQUENCE_CAPACITY` (64) /
`*_DEFAULT_STRING_CAPACITY` (256) constants. Closes the configuration half of issue
[0007-seq-capacity-64](../issues/0007-seq-capacity-64.md); the `borrowed` storage
mode (phase 3 below) closes the issue entirely.

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

### 229.4 — Wire resolver through the generators (owned)  🟡 IN PROGRESS
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
- ⬜ **Serialized-size-max** (`compute_serialized_size_max`) still uses default
  `CPP_DEFAULT_STRING_CAPACITY` for worst-case wire/buffer sizing — should scale with
  a configured cap so big-payload buffers size correctly (follow-up, note in 229.3/.5).
- ✅ **Activated** the production message paths (Rust/C/C++) via 229.3 discovery —
  `generate_package` + the C/C++ args-file paths now receive a discovered resolver.
- ⬜ **Service/action** still pass `empty()` internally (their `generate_*` fns don't
  take a resolver yet) — a real resolver + correct per-part keys is a follow-up.
- **Files:** `packages/cli/rosidl-codegen/src/types.rs`,
  `.../generator/{common,msg,srv,action}.rs`, `.../rosidl-bindgen/src/generator.rs`.

### 229.5 — `heap` storage mode  ✅ DONE (Rust + C + C++; primitive sequences + strings)
`mode = "heap"` → growable containers (`cap` ignored — unbounded). **Heap strings now
done on all three** (Rust `nros_core::heap::String`; C rclc `rosidl_runtime_c__String`
struct `{ char* data; size_t size, capacity; }` + read-u32-then-`memcpy` deserialize +
`_fini`; C++ `nros::HeapString` + shared `nros_cpp_heap_str_t` FFI repr). Only heap
**sequences of strings / nested messages** remain rejected (a further follow-up).
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

### 229.6 — `borrowed` storage mode (phase 3, closes issue 0007)  ⬜
Lifetime-carrying generated types (`struct Image<'a>`), deserializer returns slices
into the CDR receive buffer, `Subscriber`/callback signature ripple, C/C++ ptr+len
ABI structs. Bounded only by `NROS_SUBSCRIPTION_BUFFER_SIZE`.
- **Files:** generators, deserializer, subscription/callback path, C/C++ FFI glue.
- On landing: set issue 0007 `status: resolved`, `resolved_in: Phase 229`, move to
  `docs/issues/archived/`.

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
  bound wins; the entry is ignored with a warning.
- **Phase gating** — phase-1 build emits a clear error for `heap`/`borrowed`; lifted
  as 229.5 / 229.6 land.
- `just ci` green.

## Notes

- Phasing maps to RFC-0033 storage modes: **P1** 229.1–229.4 (config method +
  `owned`, full language-agnostic surface, immediately useful), **P2** 229.5 (`heap`),
  **P3** 229.6 (`borrowed`, closes issue 0007).
- Design rationale + alternatives live in RFC-0033 — this doc is the work breakdown
  only. Resolve RFC-0033 open questions (1: warn-vs-error on bounded entry; 2:
  inline-table at package/type levels; 3: borrowed lifetime threading) before flipping
  the RFC to `Stable` (with the matching ARCHITECTURE.md update in the same commit).
