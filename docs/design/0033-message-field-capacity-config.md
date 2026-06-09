---
rfc: 0033
title: "Per-field message capacity configuration"
status: Draft
since: 2026-06
last-reviewed: 2026-06
implements-tracked-by: [phase-229]
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
3. `borrowed`-mode lifetime threading through `Subscriber` / callback signatures and
   the C/C++ ptr+len ABI — deferred to phase 3; resolved as part of closing issue 0007.

## Changelog

- 2026-06 — created (Draft). Brainstormed design captured at
  `docs/superpowers/specs/2026-06-09-per-field-message-capacity-config-design.md`;
  work breakdown in [phase-229](../roadmap/phase-229-message-field-capacity-config.md).
