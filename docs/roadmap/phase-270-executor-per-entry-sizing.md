# Phase 270 — Per-entry executor sizing (externalised storage)

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

## Design — externalised, const-generic STORAGE + slice-holding Executor

Move the sized storage OUT of `Executor` into a caller-provided struct; `Executor`
holds runtime **slices** into it — one lifetime, no type/const generic, so the FFI
type stays concrete and no-alloc stays satisfied.

```rust
// nros-node — const-generics isolated HERE (the entry instantiates it, macro-sized):
pub struct ExecutorStorage<const CBS: usize, const SC: usize, const ARENA: usize> {
    arena: [MaybeUninit<u8>; ARENA],
    entries: [Option<CallbackMeta>; CBS],
    sched_contexts: [Option<SchedContext>; SC],
    sched_context_bindings: [SchedContextId; CBS],
    sporadic_states: [Option<SporadicState>; SC],
    // + sporadic_atomic_states[cfg alloc]
}
// Executor gains ONLY a lifetime (mechanical across the ~19 impl sites):
pub struct Executor<'s> { session, arena: &'s mut [MaybeUninit<u8>], entries: &'s mut [..], … }
```

- **Rust entry (`nros::main!`):** emit a `static` `ExecutorStorage<TOPOLOGY_N,…>`
  sized from the baked topology entity count → `Executor::open(&cfg, &mut STORAGE)`.
  Per-entry, no env, no-alloc.
- **C/C++ FFI:** a default-sized `static ExecutorStorage<DEFAULT,…>` + `CExecutor =
  Executor<'static>` → concrete, probe-able; C entries keep the env-default.
- Method bodies index `self.entries[i]` / `self.arena[..]` — slice indexing is
  array-compatible, so bodies are largely untouched.

## Waves

- **W1 — storage struct.** Add `ExecutorStorage<CBS,SC,ARENA>` + a `DEFAULT_*`
  const set (== current `config::{MAX_CBS,MAX_SC,ARENA_SIZE}`) + a `.views()`
  accessor. A `DefaultExecutorStorage` alias + a `static` default instance for the
  no-storage-arg callers. Additive; `just check` green.
- **W2 — `Executor<'s>` slices.** Convert the 6 array fields to `&'s mut [_]`; add
  `<'s>` to the ~19 impl/fn sites. `open`/`from_session_ptr` take `&'s mut
  ExecutorStorage<…>` (or its views). Keep a `Executor::open` convenience that
  binds the default static storage for existing callers.
- **W3 — macro.** `nros::main!` computes the entry's entity count + emits the
  sized `static ExecutorStorage` + threads it into `open`.
- **W4 — FFI + examples.** `nros-c`/`nros-cpp` pin the default storage; fix the
  handful of direct-`open` callers/tests.
- **W5 — drop the workaround.** `workspace-rust-native-showcase` drops
  `NROS_EXECUTOR_MAX_CBS = "8"` and still boots its 4-node / 5-callback launch.

## Complication found (FFI memory model) — read before implementing

The C FFI holds the Executor **inline** in an opaque buffer (`nros_executor_t {
u64 _opaque[EXECUTOR_OPAQUE_U64S] }`, sized by the `sizeof(Executor)` build probe).
Externalising storage means `Executor<'s>` **borrows** the storage. If both live in
the SAME opaque buffer that is a **self-referential struct** (needs `Pin` + unsafe;
fragile) — a real design hazard on the C path, NOT just mechanical churn.

**Mitigation:** keep the storage a SEPARATE allocation from the Executor handle on
every path — the caller owns the storage, the Executor only borrows:
- Rust entry: `static STORAGE` + `static`/stack `Executor` (distinct) — fine.
- C API: add a second opaque `nros_executor_storage_t` the caller allocates and
  passes to `nros_executor_init(&exec, &storage, …)`. Two buffers, no self-ref.
  This is a **C API surface change** (+ the C++ wrapper) — the largest part.
- Every direct `Executor::open` caller (tests, a few examples) provides storage.

So this is a multi-system refactor (Rust core + C API + C++ wrapper + macro + all
callers), phase-sized and central — implement it as a dedicated, carefully-staged
effort with `just check`/`just ci` at each wave, not rushed. W1 (the storage
struct) is additive and safe to land first.

## Acceptance (from #0110)

A workspace with one fat native entry (≥5 callbacks) + ≥1 lean embedded entry
builds each sized to its own topology — embedded keeps the small arena, native fits
5+ callbacks — WITHOUT a workspace-global `NROS_EXECUTOR_MAX_CBS`; C/C++ stays a
concrete wrapper.
