# Per-Field Message Capacity Configuration — Design

- **Date:** 2026-06-09
- **Status:** Draft (design approved in brainstorming; awaiting spec review)
- **Area:** codegen (`packages/cli/rosidl-codegen`)
- **Related issue:** `docs/issues/0007-seq-capacity-64.md` (unbounded sequences capped at 64)

## Problem

The codegen hardcodes the local storage capacity of unbounded message fields:

- `NROS_DEFAULT_SEQUENCE_CAPACITY = 64`, `NROS_DEFAULT_STRING_CAPACITY = 256` (Rust)
- `C_DEFAULT_SEQUENCE_CAPACITY = 64`, `C_DEFAULT_STRING_CAPACITY = 256` (C)
- `CPP_DEFAULT_SEQUENCE_CAPACITY = 64`, `CPP_DEFAULT_STRING_CAPACITY = 256` (C++)

all defined in `packages/cli/rosidl-codegen/src/types.rs` and referenced directly
throughout `types.rs` and `generator/common.rs`. There is no override path.

The right capacity is application-dependent. One app wants a multi-megabyte
`sensor_msgs/Image.data` buffer but a tiny `std_msgs/String.data`; another wants
the opposite. A single global constant cannot serve both, and editing the shared
upstream `.msg` files (forking `sensor_msgs`) is unacceptable.

## Goals

1. **Per-field** capacity configuration — `sensor_msgs/Image.data` independent of
   `std_msgs/String.data`, independent even within one message.
2. **Language-agnostic** — one configuration drives the Rust, C, and C++ generators
   identically. Not three parallel configs.
3. **Scenario-local** — configuration lives in the workspace / app, never in the
   shared interface package's `.msg` files.
4. **Compatibility** — explicit `.msg` bounds (`uint8[<=N]`, `string<=N`) remain
   authoritative and are never overridden.

## Non-Goals

- Changing the CDR wire format. Capacity is a *local storage* decision only; an
  unbounded sequence serializes length-prefixed regardless of local capacity, so
  capacity never affects interop or type hashes.
- Implementing `heap` and `borrowed` storage modes in phase 1 (see Phasing).

## Key Facts Grounding the Design

- **Capacity ≠ wire format.** CDR serializes bounded and unbounded sequences
  identically (a `uint32` length prefix + elements). Local capacity is invisible on
  the wire. Therefore the capacity of an *unbounded* field is a free local choice
  with zero compatibility impact.
- **`.msg` bounds are part of the type.** `uint8[<=N]` / `string<=N` participate in
  the rosidl type definition and its hash. The config must not override them —
  doing so could admit invalid messages or reject valid ones. The parser already
  models these as `FieldType::BoundedSequence { .. }` / `FieldType::BoundedString(n)`
  and the generators already use the declared bound. This design leaves that path
  untouched.
- **Generated message code is one library per interface package**, shared by every
  node that links it. Two nodes cannot get different `Image.data` capacities from
  the *same* generated output. In nano-ros each app already regenerates its own
  gitignored `generated/`, so configuration binds naturally **per codegen
  invocation**.

## Configuration Model

### File: `nros-codegen.toml`

Two scopes, deep-merged (app overrides workspace on identical keys):

- **Workspace file** — at the workspace root. Shared defaults for all members.
- **App/node file** — in the app directory (next to `Cargo.toml` / `CMakeLists.txt`).
  Overrides merged over the workspace file.

The merge produces one logical config; precedence resolution (below) then runs on it.

### Grammar

```toml
[defaults]                        # precedence 5 (global fallback)
sequence_capacity = 64
string_capacity   = 256

[packages."sensor_msgs"]          # precedence 4 (whole package)
sequence_capacity = 4096

[types."sensor_msgs/Image"]       # precedence 3 (all unbounded fields in one message)
sequence = { cap = 2_000_000, mode = "borrowed" }

[fields]                          # precedence 2 (sharpest)
"sensor_msgs/Image.data"       = { cap = 2_000_000, mode = "borrowed" }
"sensor_msgs/LaserScan.ranges" = { cap = 1080, mode = "heap" }
"std_msgs/String.data"         = 64        # int shorthand = { cap = 64, mode = "owned" }
```

- `/` separates package from message (ROS convention); `.` separates the field name.
  Keys are quoted so TOML does not split on the dots.
- An entry value is either an **integer** (shorthand for `{ cap = <int>, mode = "owned" }`)
  or an **inline table** `{ cap = <int>, mode = "owned" | "heap" | "borrowed" }`.
- **`mode` defaults to `"owned"`** when omitted.
- `string_capacity` / `sequence_capacity` at the `[defaults]` and `[packages.*]`
  levels are plain integers (owned). String fields use the same per-field/per-type
  override mechanism for non-owned modes.

### Precedence (highest wins)

```
1. .msg explicit bound (uint8[<=N], string<=N)   — authoritative, never overridden
2. [fields]  "pkg/Msg.field"
3. [types]   "pkg/Msg"
4. [packages] "pkg"
5. [defaults]
6. built-in default (64 sequence / 256 string)
```

Only **unbounded** fields (`FieldType::Sequence`, unbounded `FieldType::String`)
consult levels 2–6. Bounded fields stop at level 1.

### Storage modes

| mode       | Rust type            | C / C++ type                  | Cost                       | Phase |
|------------|----------------------|-------------------------------|----------------------------|-------|
| `owned`    | `heapless::Vec<T, N>` / `heapless::String<N>` | fixed `[N]` array / `FixedSequence<N>` | `N` elems always inline    | 1     |
| `heap`     | `alloc::Vec<T>` (cap = hint) | growable seq (alloc-backed)   | dynamic; needs `alloc`/`std` | 2   |
| `borrowed` | `&'a [T]` / `&'a str` into CDR buffer | `{ const T* ptr; size_t len; }` | pointer+len, zero-copy, callback-scoped | 3 |

`owned` is the current behavior, now driven by the resolved `cap` instead of the
constant. `heap` and `borrowed` are deferred (see Phasing); phase-1 codegen rejects
them with a clear error.

## Codegen Changes

### `CapacityResolver`

A new type loaded once per codegen invocation from the merged `nros-codegen.toml`:

```rust
pub struct FieldStorage { pub cap: usize, pub mode: StorageMode }
pub enum StorageMode { Owned, Heap, Borrowed }

impl CapacityResolver {
    /// Returns the resolved storage for an *unbounded* field.
    /// Bounded fields are resolved by the caller from the .msg bound and never
    /// reach this method.
    pub fn resolve(&self, package: &str, message: &str, field: &str, kind: FieldKind)
        -> FieldStorage;
}
```

- `kind` distinguishes sequence vs string so the right `*_capacity` default applies.
- Every current direct reference to `*_DEFAULT_SEQUENCE_CAPACITY` /
  `*_DEFAULT_STRING_CAPACITY` for an unbounded field in `types.rs` and
  `generator/common.rs` is replaced by a `resolver.resolve(...)` call.
- The same resolver instance feeds all three backends. This is the structural
  guarantee of language-agnosticism: one resolver, three emitters. A given field
  yields one `FieldStorage` regardless of target language.
- The built-in constants remain as the level-6 fallback inside the resolver, so a
  missing/empty config reproduces today's output byte-for-byte.

### Config discovery

The codegen entry points gain a configuration source, in priority order:

1. Explicit `--codegen-config <path>` flag on the `nros generate-*` commands and a
   `CODEGEN_CONFIG` argument on the CMake `nano_ros_generate_interfaces(...)`
   function (so C/C++ builds pass it the same way).
2. Auto-discovery: `nros-codegen.toml` in the codegen output's package/app dir, then
   walking up to the workspace root, deep-merging ancestor → descendant.

Absent any file, the resolver uses built-in defaults — no behavior change.

## Phasing

- **Phase 1 — Configuration method + `owned`.** Full grammar, deep-merge, precedence,
  `CapacityResolver`, discovery, wired through all three generators. `mode = "owned"`
  works end-to-end. `heap` / `borrowed` parse but error
  `"storage mode '<m>' not yet supported"`. This delivers the configuration method
  in full and immediately unblocks "bigger owned buffers where the platform can
  afford them, smaller where it can't."
- **Phase 2 — `heap`.** Alloc-backed sequences/strings behind the `alloc`/`std`
  feature gate.
- **Phase 3 — `borrowed`.** Lifetime-carrying generated types and the
  `Subscriber`/callback ripple; C/C++ ptr+len structs. This is the body of issue
  `0007-seq-capacity-64.md` and closes it.

## Testing

- **Resolver unit tests** — precedence ladder (field > type > package > defaults >
  builtin), int-shorthand expansion, bounded-field short-circuit, deep-merge of
  workspace + app files.
- **Golden codegen tests** — a fixture `.msg` set + a `nros-codegen.toml` produce the
  expected Rust/C/C++ types; assert per-field capacities differ within one message
  (big sequence + small string).
- **No-config regression** — empty/absent config reproduces current generated output
  exactly (lock against accidental default drift).
- **Compat** — a `.msg` with explicit bounds plus a conflicting `[fields]` entry: the
  bound wins; the config entry is ignored (and a warning emitted).

## Open Questions

- Warn vs hard-error when a `[fields]` entry targets a bounded field (proposed: warn
  + ignore, since the bound is correct and the config is merely redundant/stale).
- Whether `[packages.*]` / `[types.*]` should also accept the inline-table
  `{ cap, mode }` form for non-owned defaults (proposed: yes, same parser as
  `[fields]`, for symmetry).
