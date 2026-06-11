---
rfc: 0033
title: "Per-field message capacity configuration"
status: Stable
since: 2026-06
last-reviewed: 2026-06-11
implements-tracked-by: [phase-229, phase-235]
supersedes: []
superseded-by: null
---

# RFC-0033 — Per-field message capacity configuration

## Summary

Generated message bindings store unbounded sequence/string fields in fixed-capacity
containers whose size is a single hardcoded constant (sequences 64, strings 256),
identical across the Rust, C, and C++ generators and unoverridable. The right
capacity is application-dependent — one app needs a multi-megabyte
`sensor_msgs/Image.data` and a tiny `std_msgs/String.data`; another the reverse.
This RFC defines a **language-agnostic, per-field** capacity configuration: a
`nros-codegen.toml` resolved once per codegen invocation into a single resolver that
feeds all three generators, with a precedence ladder that always defers to explicit
`.msg` bounds. It also defines the storage-mode axis (`owned` / `heap` / `borrowed`)
that the per-field value selects, so a large buffer is realized as a heap or
zero-copy borrow rather than dead inline stack.

## Motivation / problem

`packages/cli/rosidl-codegen/src/types.rs` defines `NROS_DEFAULT_SEQUENCE_CAPACITY = 64`
and `NROS_DEFAULT_STRING_CAPACITY = 256`, mirrored as `C_*` and `CPP_*` constants and
referenced directly throughout `types.rs` and `generator/common.rs`. There is no
override path. Consequences (issue
[0007-seq-capacity-64](../issues/0007-seq-capacity-64.md)):

- Large sensor messages (`Image`, `PointCloud2`, `LaserScan`, `OccupancyGrid`) fail
  to deserialize — incoming data exceeds 64 elements → `DeserError::CapacityExceeded`.
- Naively bumping the constant is worse: `heapless::Vec<u8, 65536>` always occupies
  64 KB inline regardless of content, which is fatal on a 64–256 KB MCU.
- Editing the shared upstream `.msg` (forking `sensor_msgs`) to add a bound is
  unacceptable and still can't vary per app.

Constraints: `no_std` embedded targets (no implicit heap), wire-compatibility with
stock ROS 2 / rmw, and the existing one-library-per-interface-package codegen shape.

Two facts shape the whole design:

1. **Capacity ≠ wire format.** CDR serializes bounded and unbounded sequences
   identically (`uint32` length prefix + elements). Local container capacity is
   invisible on the wire — so the capacity of an *unbounded* field is a free local
   choice with zero interop / type-hash impact.
2. **`.msg` bounds are part of the type.** `uint8[<=N]` / `string<=N` participate in
   the rosidl type and its hash; the parser already models them
   (`FieldType::BoundedSequence`, `FieldType::BoundedString(n)`) and the generators
   already use the declared bound. Configuration must never override a bound — doing
   so could admit invalid messages or reject valid ones.

## Design

### Configuration file: `nros-codegen.toml`

Two scopes, deep-merged into one logical config (app overrides workspace on identical
keys); precedence resolution then runs on the merged result:

- **Workspace file** at the workspace root — shared defaults for all members.
- **App/node file** in the app directory (next to `Cargo.toml` / `CMakeLists.txt`) —
  overrides merged over the workspace file.

```toml
[defaults]                        # precedence 5 (global fallback)
sequence = 64
string   = 256

[packages."sensor_msgs"]          # precedence 4 (whole package)
sequence = 4096

[types."sensor_msgs/Image"]       # precedence 3 (all unbounded fields in one message)
sequence = { cap = 2_000_000, mode = "borrowed" }

[fields]                          # precedence 2 (sharpest)
"sensor_msgs/Image.data"       = { cap = 2_000_000, mode = "borrowed" }
"sensor_msgs/LaserScan.ranges" = { cap = 1080, mode = "heap" }
"std_msgs/String.data"         = 64        # int shorthand = { cap = 64, mode = "owned" }
```

- Every level (`[defaults]`, `[packages.*]`, `[types.*]`) takes the same two keys
  `sequence` and `string`; `[fields]` maps `"pkg/Msg.field"` directly to one value.
- `/` separates package from message (ROS convention); `.` separates the field. Keys
  are quoted so TOML does not split on the dots.
- A value is either an **integer** (shorthand for `{ cap = <int>, mode = "owned" }`)
  or an inline table `{ cap = <int>, mode = "owned" | "heap" | "borrowed" }` — the
  same int-or-table form at every level (resolves open question 2).
- **`mode` defaults to `owned`** when omitted. Unknown table keys are rejected.

### Precedence (highest wins)

```
1. .msg explicit bound (uint8[<=N], string<=N)   — authoritative, never overridden
2. [fields]   "pkg/Msg.field"
3. [types]    "pkg/Msg"
4. [packages] "pkg"
5. [defaults]
6. built-in default (64 sequence / 256 string)
```

Only **unbounded** fields (`FieldType::Sequence`, unbounded `FieldType::String`)
consult levels 2–6. Bounded fields stop at level 1. The built-in constants remain as
the level-6 fallback, so a missing/empty config reproduces today's output exactly.

### Storage modes

| mode       | Rust type                                     | C / C++ type                              | Cost                                   | Phase |
|------------|-----------------------------------------------|-------------------------------------------|----------------------------------------|-------|
| `owned`    | `heapless::Vec<T, N>` / `heapless::String<N>` | fixed `[N]` array / `FixedSequence<N>`    | `N` elems always inline                | 1     |
| `heap`     | `alloc::Vec<T>` (cap = hint)                  | growable seq (alloc-backed)               | dynamic; needs `alloc`/`std`           | 2     |
| `borrowed` | `&'a [T]` / `&'a str` into CDR buffer         | `{ const T* ptr; size_t len; }`           | pointer+len, zero-copy, callback-scoped| 3     |

`owned` is today's behavior, now driven by the resolved `cap`. `borrowed` is the
zero-copy direction in issue 0007: the deserializer returns a slice into the CDR
receive buffer (no copy, no fixed capacity), the message struct gains a lifetime, and
the payload is bounded only by `NROS_SUBSCRIPTION_BUFFER_SIZE`.

### Borrowed mode — when it is used and how it meets the user API

`borrowed` is fundamentally different from `owned`/`heap`: it is a **receive-side,
callback-scoped, read-only view**, not a container. A borrowed field is a `&'a [u8]`
/ `&'a str` (C/C++: `{ const T* data; size_t size; }`) pointing into the live CDR
receive buffer; the "deserialize" pass records each field's offset+len instead of
copying.

**When it is the right mode:**

- **Large unbounded payloads on receive** — `sensor_msgs/Image.data` (≈900 KB),
  `PointCloud2.data`, `LaserScan.ranges`. The original issue-0007 case.
- **Allocator-free MCUs** — `owned` can't fit the payload inline and `heap` needs a
  `malloc` per message; `borrowed` needs neither (the bytes already sit in the RX
  buffer). For a 64–256 KB MCU receiving frames it is the only viable mode for big
  payloads.
- **Process-in-callback-then-discard** — vision/DSP over the slice, or a bridge that
  inspects headers while forwarding raw bytes. The view is never retained.

**Hard constraints (what borrowed cannot do):**

- **Callback-scoped only.** The slice is valid only for the duration of the
  subscription callback; the buffer is released/reused immediately after. A borrowed
  `Msg<'a>` therefore **cannot** be returned by `Subscription::try_recv() -> Option<M>`
  (no lifetime anchor outside a callback) and **cannot** be stored past the callback —
  copy the needed parts out instead.
- **Read-only**, receive-only. The publish side owns the data it sends; "borrow on
  publish" is a different mechanism (the `pub_loan` zero-copy loan API, Phase 124), not
  this mode.

**How it cooperates with the existing API:**

The infrastructure already exists — `borrowed` is a *typed* layer over it, not new
plumbing:

- The RMW exposes `sub_borrow`/`sub_release` (Phase 124, `nros-rmw-cffi`) returning the
  raw received buffer with a release token; `RecvView<'a>` (`nros-node executor/handles`)
  is the Rust wrapper. Today that borrow is exposed **only on the polling path**
  (`RawSubscription::try_borrow()`).
- Today's callback path deserializes into an **owned** `M` and calls `FnMut(&M)`
  (`executor/arena.rs::sub_buffered_try_process`). Borrowed mode changes the callback
  to `FnMut(&Msg<'a>)` and makes the dispatch **hold the `sub_borrow` view across the
  callback**, build the typed slice view over it, then release. The C/C++ callbacks
  already receive raw `(data, len)` (`nros-c subscription`), so the C/C++ borrowed type
  is a typed accessor over the same raw callback.
- **Buffer-strategy limit:** a single borrowed view per invocation is well-defined only
  on the **triple-buffer** strategy (queue depth ≤ 1). The SPSC ring (depth > 1) holds
  several messages in flight, so `borrowed` subscriptions are restricted to depth ≤ 1.

**Element alignment — solved by an unaligned decoder, not a fallback copy.** `&[u8]`
/ `&str` are always safe to borrow directly. Multi-byte numerics (`&[f32]` in
`LaserScan.ranges`, `uint16[]`, …) are unsafe to alias as a typed slice: CDR aligns
elements *within* the buffer, but `buffer_base + field_offset` need not satisfy the
element alignment, so `slice[i]` would be UB on strict-alignment targets. Rather than
degrade such fields to `owned`/`heap`, borrowed numeric sequences are exposed through
an **alignment-agnostic view** that decodes each element by value
(`memcpy` of `size_of::<T>()` bytes + little-endian decode), never forming a
`&[T]`/`T*` into the unaligned bytes:

- **Rust** — `nros_core::LeSliceView<'a, T>` (`nros-serdes/src/cdr.rs`), shipped in
  Phase 229.6. `get(i) -> Option<T>` does the unaligned LE decode.
- **C** — a generated `{ const uint8_t* bytes; size_t count; }` view per numeric
  element type + a `..._get(view, i)` inline that `memcpy`s and LE-decodes.
- **C++** — `nros::LeSpan<T>` with `T operator[](i)` doing the same.

So *all* sequence element types can borrow (byte/string directly as
`{const T* ptr; size_t len;}`, numerics via the LE view); only sequence-of-string /
sequence-of-nested are rejected (no flat byte run to alias). This is full parity
across Rust, C, and C++.

**The work is mostly runtime, not codegen.** Generating `Msg<'a>` with slice fields is
the smaller half of phase 3; the substantive change is the executor/subscription
callback-borrow dispatch above. See phase-229 § 229.6.

### Borrowed mode — C and C++ realization

Rust borrowed shipped in Phase 229.6: `mode = "borrowed"` emits `{Msg}View<'a>`
(borrowed fields `&'a [u8]` / `&'a str` / `LeSliceView<'a, T>`, copied fields owned) +
a `{Msg}Borrow` ZST marker + `impl DeserializeBorrowed`, dispatched via
`create_subscription_borrowed`. C and C++ borrowed (phase-235) mirror the *view shape*
but differ in **who walks the CDR**, following the project rule that **C++ wraps the
Rust API and never re-implements serdes**:

- **C — native, pointer-setting deserialize.** C already has its own CDR readers
  (`nros-c/include/nros/cdr.h`). Codegen emits `{Msg}_View` (borrowed fields as
  `{const char* data; size_t size;}` / `{const uint8_t* data; size_t size;}` / the
  numeric LE-view) and `int32_t {Msg}_deserialize_borrowed({Msg}_View*, const uint8_t*
  buf, size_t len)` that walks CDR, bounds-checks against `end`, and **sets pointers
  into `buf`** for borrowed fields (owned fields copied as today). No `malloc`, no
  `_fini`.

- **C++ — wraps a Rust FFI offset seam (no native C++ CDR reader).** The C++ owned
  path already deserializes through the Rust FFI (`ffi_deserialize`); borrowed extends
  that seam. A `{Msg}_ffi_deserialize_borrowed` walks CDR with the Rust reader and
  returns a per-borrowed-field `(offset, len)` struct (offsets relative to `buf`); the
  generated C++ `{Msg}View` then sets `nros::Span<T>` / `nros::StringView` /
  `nros::LeSpan<T>` (`nros-cpp/include/nros/span.hpp`) into the raw callback buffer.
  CDR logic stays single-sourced in Rust. (A pure-C++ `cdr_reader.hpp` was considered
  and rejected for this reason.)

Both ride the existing raw `(data, len)` subscription callbacks
(`nros_subscription_callback_t` / `nros_cpp_subscription_message_callback_t`) — the
borrowed view is a typed accessor, **no new subscription ABI**. Implementation +
work-item breakdown: [phase-235](../roadmap/archived/phase-235-c-cpp-borrowed-views.md).

### Codegen: one resolver, three emitters

A `CapacityResolver` is loaded once per codegen invocation from the merged config:

```rust
pub struct FieldStorage { pub cap: usize, pub mode: StorageMode }
pub enum StorageMode { Owned, Heap, Borrowed }

impl CapacityResolver {
    /// Resolve storage for an *unbounded* field. Bounded fields are resolved by the
    /// caller from the .msg bound and never reach this method.
    pub fn resolve(&self, package: &str, message: &str, field: &str, kind: FieldKind)
        -> FieldStorage;
}
```

Every current direct reference to `*_DEFAULT_SEQUENCE_CAPACITY` /
`*_DEFAULT_STRING_CAPACITY` for an unbounded field — in `types.rs` and
`generator/common.rs` — is replaced by a `resolver.resolve(...)` call. **The same
resolver instance feeds all three backends**: a given field yields one `FieldStorage`
regardless of target language. That single-resolver / three-emitter shape is the
structural guarantee of language-agnosticism — there is no per-language config.

### Config discovery

In priority order:

1. Explicit `--codegen-config <path>` on the `nros generate-*` commands and a
   `CODEGEN_CONFIG` argument on the CMake `nano_ros_generate_interfaces(...)` function
   (so C/C++ builds pass it identically). Cross-links RFC-0023
   (codegen-workspace-discovery).
2. Auto-discovery of `nros-codegen.toml` in the codegen output's package/app dir,
   walking up to the workspace root, deep-merging ancestor → descendant.

Absent any file, the resolver uses built-in defaults — no behavior change.

## Alternatives considered

- **In-`.msg` bounds only** (`uint8[<=N]`, the standard ROS mechanism). Language-
  agnostic and already honored, but global to the type, forces forking shared upstream
  packages, and cannot vary per application. Kept as the authoritative level-1 input,
  rejected as the *configuration* surface.
- **Per-type granularity only** (`sensor_msgs/Image = N`). Simpler keys, but cannot
  express a big-sequence + small-string split inside one message. Rejected; per-field
  is required. Per-type is retained as precedence level 3.
- **Number-only entries** (no `mode`). Terse, but a multi-megabyte cap as inline
  `Vec<u8, 1048576>` is dead stack — useless for the motivating image case. Rejected
  in favor of the explicit `{ cap, mode }` value (mode defaults to `owned`, so the
  terse integer form still works for the common small-field case).
- **Auto-selected mode** (codegen picks inline/heap/borrowed from a size threshold).
  Terser still, but a hidden threshold whose meaning depends on target features is
  surprising. Rejected in favor of explicit per-field `mode`.
- **Non-TOML formats / Rust attributes / C macros / CMake vars.** Any per-language
  source of truth breaks language-agnosticism. Rejected. TOML matches repo convention
  (`nros-sdk-index.toml`, `system.toml`).

## Open questions

1. Warn vs hard-error when a `[fields]` entry targets a *bounded* field. Proposed:
   warn + ignore (the bound is correct; the entry is redundant/stale).
2. ~~Whether `[packages.*]` / `[types.*]` accept the inline-table `{ cap, mode }`
   form.~~ **Resolved (229.1):** yes — every level uses the same int-or-table value
   under `sequence` / `string` keys.
3. ~~`borrowed`-mode lifetime threading through `Subscriber` / callback signatures and
   the C/C++ ptr+len ABI.~~ **Resolved (design):** borrowed is a receive-side,
   callback-scoped, read-only view over the existing `sub_borrow` zero-copy primitive
   — `FnMut(&Msg<'a>)`, depth ≤ 1 only, no `take()`/store/publish. See "Borrowed mode"
   above; implementation in phase-229 § 229.6.

## Changelog

- 2026-06 — created (Draft). Brainstormed design captured at
  `docs/superpowers/specs/2026-06-09-per-field-message-capacity-config-design.md`;
  work breakdown in [phase-229](../roadmap/phase-229-message-field-capacity-config.md).
- 2026-06 — added the "Borrowed mode" section (use cases, callback-scoped constraint,
  `sub_borrow`/`RecvView` integration, alignment caveat); resolved open question 3.
