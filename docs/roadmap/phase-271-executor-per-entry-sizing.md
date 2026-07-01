# Phase 271 — Per-entry executor sizing (externalised storage)

Status: **In progress (2026-07-01).** Resolves [issue 0110](../issues/0110-executor-max-cbs-per-entry-sizing-knob.md).

## Problem

`Executor`'s callback table + arena are fixed arrays sized by build-time consts
(`MAX_CBS`/`MAX_SC`/`ARENA_SIZE`, from `NROS_EXECUTOR_MAX_CBS` at **`nros-node`'s**
compile). In a shared-target workspace `nros-node` compiles ONCE → one size for
every entry: raising it for a fat native entry bloats every lean embedded entry.
No per-entry knob without the global `[env]` hammer + separate target dirs.

## Why not the "obvious" fixes

- **Const-generic `Executor<CBS>`** — the C/C++ FFI wraps a CONCRETE
  `nros_node::Executor` (`CExecutor = nros_node::Executor`, its opaque C buffer is
  `sizeof(Executor)` probed at build). A type/const-generic Executor has no single
  concrete type/size to wrap. (This is why Phase 68 removed the generics — the
  struct even uses fn-pointers over a `Platform` generic to stay concrete.)
  Toolchain is **stable** → no `generic_const_exprs`, so `ARENA` can't derive from
  `CBS` in an array length either.
- **Heap the arena** — no-alloc embedded can't.
- **Per-entry `[env]` / build-knob** — `[env]` is workspace-global; only separate
  target dirs give per-entry, which needs build orchestration + rebuilds `nros-node`
  per entry (the current showcase-fixture workaround).

## Guiding principle (C/C++ is a THIN wrapper of Rust)

The C/C++ API wraps the Rust API 1:1, so:
- **Public Rust API must avoid generics.** Type/const generics can't be named or
  monomorphised from C — a generic public type has no single ABI the wrapper can
  bind to.
- **Generics are fine in PRIVATE Rust code.**
- **If a generic MUST appear in the public API, its C/C++ interop must be ensured**
  — i.e. the C side binds a single concrete monomorphisation.

So the executor **`Executor` itself must stay non-generic**. A *lifetime*
(`Executor<'s>`) is acceptable: lifetimes are erased at the ABI, and the C side
already holds the executor as an opaque `nros_executor_t*` — a lifetime changes
nothing it can observe. The open question is only how the ENTRY supplies its
per-topology-sized storage across the FFI without a public size-generic.

## Design — externalised storage, non-generic `Executor<'s>`

`Executor` keeps every field except the six sized arrays; those become `&'s mut`
**slices** into caller-owned storage:

```rust
pub struct Executor<'s> {           // NON-generic (lifetime only) → C wraps as before
    session: SessionStore,
    arena: &'s mut [MaybeUninit<u8>],
    entries: &'s mut [Option<CallbackMeta>],
    sched_contexts: &'s mut [Option<SchedContext>],
    sched_context_bindings: &'s mut [SchedContextId],
    sporadic_states: &'s mut [Option<SporadicState>],
    // … all other fields unchanged
}
```

Method bodies index `self.entries[i]` / `self.arena[..]` — slice indexing is
array-compatible, so the ~19 impl sites only gain `<'s>` (mechanical). The design
fork is HOW the entry provides the backing:

### Option A — private const-generic storage + non-generic byte-buffer public API (RECOMMENDED)

Keep a `pub(crate)` const-generic `ExecutorStorage<CBS,SC,ARENA>` (typed arrays,
SAFE, private), but expose it to entries ONLY through a **non-generic public API**:

```rust
// PUBLIC, non-generic — the entry hands raw aligned bytes; nros-node carves them.
pub const fn executor_storage_layout(cbs: usize, sc: usize, arena: usize) -> Layout;
impl<'s> Executor<'s> {
    // backing: a MaybeUninit byte region ≥ executor_storage_layout(...).size(),
    // aligned to .align(). SAFETY: sized/aligned per the layout fn.
    pub unsafe fn open_in(cfg: &ExecutorConfig, backing: &'s mut [MaybeUninit<u8>]) -> …;
}
```

- **Rust entry (`nros::main!`):** emit `static mut BACKING: Aligned<[MaybeUninit<u8>;
  executor_storage_layout(N,SC,A).size()]>` and call `Executor::open_in(&cfg, &mut BACKING)`.
  The size const is the entry's own local value — **no generic crosses the API**.
- **C/C++ FFI:** provide a default-sized `nros_executor_storage_t` opaque buffer
  (`= executor_storage_layout(MAX_CBS, MAX_SC, ARENA_SIZE)`, probe-measured) as a
  SECOND handle (see self-ref below); pass it to `open_in`. Non-generic, clean.
- Internally, `open_in` carves the byte region into the typed slices with correct
  alignment (the only `unsafe`; contained + unit-tested against a typed
  `ExecutorStorage<DEFAULT>` reference layout).

Public surface stays generic-free; the const-generic lives entirely private.

### Option B — public generic `ExecutorStorage<CBS,SC,ARENA>` + generic `open` method

Expose the storage type + a generic `open<const CBS,SC,ARENA>` method. Safer
internals (typed arrays, no `unsafe` carving) but it puts a **public generic** on
the API — permitted only under "ensure C/C++ interop": the C side binds the single
`DefaultExecutorStorage = ExecutorStorage<MAX_CBS,MAX_SC,ARENA_SIZE>` concrete alias.
`Executor` stays non-generic, but the generic storage type + generic method violate
the "avoid generics in public API" preference.

### Choice

**Recommend A**: it honors the principle most strictly (zero public generics — the
generic is `pub(crate)`), and the only cost is a small, well-contained `unsafe`
carve validated against the typed reference layout. B trades that `unsafe` for a
public generic, which the principle disfavors. Both keep `Executor` non-generic and
both need the two-handle C API below.

## Waves (Option A)

- **W1 — private storage + layout + carve (additive-ish).** `pub(crate)
  ExecutorStorage<CBS,SC,ARENA>` (typed arrays) + `pub const fn
  executor_storage_layout(cbs,sc,arena) -> Layout` + a private
  `carve(backing: &mut [MaybeUninit<u8>]) -> SlicesMut` that returns the aligned
  typed sub-slices. Unit-test the carve against a real `ExecutorStorage<DEFAULT>`
  (offsets/alignment/size match `Layout`). No `Executor` change yet.
- **W2 — `Executor<'s>` slices.** Convert the six array fields to `&'s mut [_]`; add
  `<'s>` to the ~19 impl sites. Replace `Executor::new/open/from_session_ptr` inner
  init with `carve(backing)` + populate SC slot 0. Public entry point becomes
  `unsafe fn open_in(cfg, backing: &'s mut [MaybeUninit<u8>])`; a safe convenience
  wraps a default-sized backing for existing callers (a `static`/stack region).
- **W3 — macro.** `nros::main!` computes the entry's entity count → emits an aligned
  `static mut BACKING: …[executor_storage_layout(N,SC,A).size()]` and calls
  `open_in`. No generic crosses the API.
- **W4 — FFI + examples.** C API: a second opaque `nros_executor_storage_t`
  (`= layout(MAX_CBS,MAX_SC,ARENA_SIZE)`, probe-measured) the caller allocates + passes
  to `nros_executor_init(&exec, &storage, cfg)` — two handles, **no self-reference**
  (see below). `nros-cpp` mirrors. Fix the handful of direct-`open` callers/tests.
- **W5 — drop the workaround.** `workspace-rust-native-showcase` drops
  `NROS_EXECUTOR_MAX_CBS = "8"` and still boots its 4-node / 5-callback launch.

## Self-reference hazard (why two C handles)

The C FFI holds the executor **inline** in an opaque buffer (`nros_executor_t {
u64 _opaque[EXECUTOR_OPAQUE_U64S] }`, sized by the `sizeof(Executor)` build probe).
With externalised storage `Executor<'s>` **borrows** the backing; putting both in the
SAME buffer is a **self-referential struct** (`Pin` + unsafe; fragile). So keep them
SEPARATE on every path — the caller owns the backing, the executor only borrows:
- Rust entry / tests: distinct `static`/stack backing + executor.
- C: a second opaque `nros_executor_storage_t` handle. The C API gains one param on
  init — the main surface change; the C++ wrapper mirrors it.

So this is a multi-system refactor (Rust core + C API + C++ wrapper + macro + all
callers), phase-sized and central — implement it as a dedicated, carefully-staged
effort with `just check` / `just ci` at each wave, not rushed.

## Acceptance (from #0110)

A workspace with one fat native entry (≥5 callbacks) + ≥1 lean embedded entry
builds each sized to its own topology — embedded keeps the small arena, native fits
5+ callbacks — WITHOUT a workspace-global `NROS_EXECUTOR_MAX_CBS`; C/C++ stays a
concrete wrapper.
