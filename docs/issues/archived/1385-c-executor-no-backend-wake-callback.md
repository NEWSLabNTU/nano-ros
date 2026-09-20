---
id: 1385
title: "No C executor installs a backend wake callback, on any backend — and
  the naive fix is unsafe because `Executor::drop` never clears one"
status: resolved
type: bug
area: [api, core, rmw]
severity: medium
found: 2026-09-18
related: [phase-417, phase-124, issue-1384]
resolved: 2026-09-21
---

## What is true

`Executor::install_wake_signal_on_primary` hands the backend
`nros_rmw_runtime_wake_cb` plus a context pointer, so an asynchronous arrival
can cut a parked `spin_once` short. Three paths call it, all in
`packages/core/nros-node/src/executor/spin.rs`: `Executor::open`,
`open_multi` and `open_sized`'s shared `open_in`.

The C executor reaches none of them. `nros_executor_init`
(`packages/api/nros-c/src/executor.rs`) constructs through
`CExecutor::from_session_ptr_in` — the BORROWED-session constructor — and that
constructor installs nothing. So `has_async_wake` stays `false` for the entire
life of every C executor, on every backend.

It is not backend-specific and it is not a poll-only-backend story: zenoh
(`packages/rmw/zenoh/nros-rmw-zenoh/src/shim/session.rs`) and cyclonedds
(`packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/vtable.cpp`) both implement
the `set_wake_callback` slot. XRCE and uORB leave it `NULL` and are unaffected
by definition. Rust images on zenoh or cyclone get the wake; C images on the
same backend do not.

## What a C caller loses

`spin_once`'s wait decision, quoting the live code:

```rust
if was_woken {
    0
} else if self.has_async_wake && let Some(wake) = self.node_wake.as_ref() {
    let _ = wake.wait_ms(timeout_ms as u32);   // woken early by the backend
    …
    0
} else {
    // No wake primitive linked, or a poll-only backend: drive the
    // transport for the full timeout.
    timeout_ms
}
```

A C executor always takes the `else`. It is not a hang — `drive_io(timeout_ms)`
blocks in the transport's own recv, so data arriving on the primary session
still returns promptly — but everything the callback exists for is gone: an
arrival signalled from a backend worker thread or an ISR rather than from the
recv we are parked in cannot shorten the wait, and the executor waits out the
caller's whole budget instead.

The cross-thread `Executor::wake()` / `cancel()` path is unaffected: those set
`wake_flag` directly and reach the `was_woken` arm without any backend
involvement. The loss is specifically the BACKEND's async wake.

For the size of that, phase-417 W4.e (PR #1064) measured the structurally
identical `was_woken` defect one arm over: 400 ms to dispatch against 50 ms for
the same trigger issued while the spin was already parked.

## Why the naive fix is unsafe, which is the interesting half

Calling `install_wake_signal_on_primary` from `nros_executor_init` would leave a
backend holding a callback into freed executor state.

The context pointer is `Arc::as_ptr(&self.wake_ctx)` — executor-owned, and freed
when the executor drops. The callback's own SAFETY comment states the obligation:

```rust
// SAFETY: ctx points at a `WakeCtx` owned by an Executor still
// alive at the time of the call. Executor::drop must clear the
// callback via `set_wake_callback(None, _)` on all sessions
// before dropping wake_ctx; this happens in `install_wake_*`
// teardown path.
```

**There is no such teardown path.** `set_wake_callback(None` appears nowhere in
the tree — the string occurs only in that comment. `impl Drop for Executor`
(spin.rs) runs the shutdown hooks, drops component cells and drops the arena
entries; it never touches the sessions' callbacks.

On the Rust paths that window is narrow and has stayed latent: the session is
`SessionStore::Owned`, so it dies inside the same `Executor::drop`. On the C
path it is wide open by construction. `nros_executor_init` takes a session
pointer borrowed from `nros_support_t`; `rclc_executor_fini` does
`drop_in_place` on the executor and then **zero-fills `_opaque`**, while the
session lives on until a later `nros_support_fini`. Between the two calls the
backend holds a callback whose context has been freed and whose storage has been
overwritten, and any arrival in that window calls it.

## Fix shape, and the order

Two coupled defects. The order is not negotiable:

1. **First**, give the executor a teardown that clears what it installed:
   `set_wake_callback(None, ptr::null_mut())` on the primary session and on
   every extra session, in `Executor::drop`, before `wake_ctx` is dropped. This
   is the obligation the existing SAFETY comment already asserts, so it is a
   bug fix on its own account even with no C change — it closes the narrow
   Rust-side window too. Note cyclonedds already clears its own callback when
   the session is destroyed (`session.cpp`, `session_set_wake_callback(session,
   nullptr, nullptr)`), which is the backend half of the same contract; the
   executor half is what is missing.
2. **Then** install from `nros_executor_init`, i.e. make
   `from_session_ptr_in`'s C caller do what the three `open*` paths do. Doing
   this first converts a missed optimisation into a use-after-free.

Acceptance for step 1 is a test that drops an executor and then drives the
session — a backend that would have called a dangling callback must call
nothing. Acceptance for step 2 is a C-side latency assertion with a LOWER bound
on the fast case, not just an upper bound (the "assert lower bounds on timing"
rule in CLAUDE.md): a trigger delivered mid-spin must dispatch well inside the
spin budget, and the same trigger must still dispatch when the budget is long.

## Not fixed in PR #1064

W4.e found this while fixing the `was_woken` arm and deliberately left it: the
wave's boundary was the wait decision, and this needs a drop-side change in
`nros-node` plus a C-side install, with the ordering constraint above.

## Resolution (2026-09-21)

Three fix commits. Two of them are the "Fix shape" section above, in the order
it requires; the third is a defect the install EXPOSED rather than created, and
it had to land between them because the install alone turns
`node_guard_condition.c` red.

### 1. The clear, first and on its own account

`Executor::clear_wake_signal` — `set_wake_callback(None, ptr::null_mut())` on
the primary session and every extra session — run from TWO places, not one:

* `Executor::close()`, BEFORE `Session::close`. `close()` leaves the executor
  alive, so a clear that only lived in `drop` would reach a session the backend
  had already torn down. The issue's fix shape named only `drop`; this is the
  second call site it needs.
* the END of `Executor::drop`'s body — after the shutdown hooks and the arena
  entries, before the fields (and therefore `wake_ctx`) are dropped. LAST
  rather than first, deliberately: the context is a field alive for every line
  above it, so clearing at the end is the SHORTEST window in which a callback
  can fire into live state, and entity teardown above may still be talking to
  the backend.

It latches (`wake_cb_installed`), so `close()` then drop clears once.

**The predicate is not `wake_ctx.is_some()`.** The ctx is also allocated by
`signal_fd()` and by every guard condition, neither of which installs anything
on a session, and a tier executor built over a BORROWED session
(`open_with_session*`) installs nothing at all. Clearing on those would take the
boot executor's callback away — the per-tier model shares one session across
RTOS tasks by design. So the flag records what THIS executor installed, and
nothing else.

Two executors sharing one session remains last-writer-wins on the backend's
single slot: the first to tear down clears the second's wake and the second
degrades to `drive_io(full timeout)`. A lost optimisation where the alternative
is a freed pointer; stated in the code, not left to be discovered.

### 2. The C-side install

`nros_executor_init` calls `install_wake_signal_on_primary` (now `pub`, for the
same reason `set_primary_identity` is), AFTER the `ptr::write` so the context is
taken from the executor at its final address.

### 3. The defect that only became reachable once 2 landed

`spin_once`'s `was_woken` fast arm consumed the wake FLAG and returned without
entering the wait, leaving the wake PRIMITIVE posted. Every port's primitive is
a coalescing binary semaphore, so the next spin's `wait_ms` returned instantly
on a signal already acted on — that spin did not wait at all. The arm now drains
it with the non-blocking `wait_ms(0)`.

Not a C-only defect: a Rust executor on zenoh or cyclonedds has always had it,
one spin at a time. It was invisible because reaching it needs a spin that
ENTERS the wait arm after one that took the fast arm, and the wait arm needs
`has_async_wake` — which no C executor had. `node_guard_condition.c` went red on
three dispatch counts the moment the C executor started entering it.

### Measured

`executor_backend_wake.c`, one arrival 60 ms into a 400 ms budget, stub backend:

```
              idle spin   arrival dispatched at
  before        401 ms          400 ms
  after         400 ms           60 ms
```

`node_guard_condition.c`'s own mid-spin case moved with it — **400 ms → 50 ms**
for a guard trigger issued at 50 ms — and now asserts both bounds instead of
only "within the budget". That is the same shape phase-417 W4.e measured one arm
over (400 ms vs 50 ms), which this issue cited as the size of the loss.

### Tests, and how they are falsifiable

`packages/api/nros-c/tests/run/executor_backend_wake.c`, wired into
`just check c`. A latency number alone cannot separate the two halves — "the
spin came back early" is equally true of an install with no clear and of a spin
that never waited — so the probe reads the BACKEND'S OWN SLOT in both
directions. `stub_rmw_backend.c` gained `nros_stub_rmw_wake_cb_installed()`
(read the slot back) and `nros_stub_rmw_invoke_wake()` (fire the stored callback
the way a worker thread would). It stored a callback nobody could see and nobody
could fire, which is what made the executor's half of this contract untestable.

It asserts, in order: nothing installed before an executor exists; nothing
installed by opening the SESSION alone; installed after `nros_executor_init`;
an idle-spin control; an arrival mid-spin cut short, with a LOWER bound as well
as an upper one; the between-spins-then-mid-spin pair that catches §3; NOT
installed after `rclc_executor_fini`, and nothing left to call; and a second
executor over the same session installing its own, so a clear that latched the
backend off for good is caught rather than read as a pass. The post-fini branch
deliberately does not INVOKE a callback it finds: on a tree with this defect
that call IS the use-after-free, and a probe should report a fault rather than
perform one.

**Negative controls, both run, both red:**

```
CONTROL A — install reverted, clear kept:
  FAIL: nros_executor_init INSTALLED the runtime wake callback on the backend
  FAIL: the backend HAD a callback to fire
  FAIL: UPPER BOUND: an arrival signalled from the backend mid-spin must CUT THE WAIT SHORT
         idle spin 401 ms, woken spin 400 ms, arrival at 60 ms
  FAIL: a second executor over the same session installs its OWN callback
  (and node_guard_condition.c: FAIL UPPER BOUND ... idle spin 400 ms, parked wake 400 ms)

CONTROL B — clear reverted, install kept (the order this issue forbids):
  FAIL: the backend STILL holds a wake callback after rclc_executor_fini -- its
        context points into `_opaque`, which fini has just zero-filled
  FAIL: and hands it back again

CONTROL C — §3's drain reverted (measured before the fix existed):
  FAIL: LOWER BOUND: the spin AFTER a fast-arm wake must still WAIT
         between-spins 0 ms, spin after it 0 ms
```

Control A is the pre-fix tree for the install; control B is the unsafe order the
issue names; control C is the reading that turned "node_guard_condition is red"
into a diagnosis instead of a guess.

### Ran

`just check c` (all C checks passed), `check cbindgen-headers`, `abi-bindings`,
`ffi-struct-mirrors`, `rmw-abi-shape`, `api-parity`, `api-parity-ledger`,
`check no-std` (thumbv7m + riscv32imc), `check fast`, `cargo test -p nros-c
--lib` (142), `-p nros-node --lib --features std` (433) and `std,rmw-cffi`
(218), `-p nros-rmw --lib` (56), clippy `-D warnings` on nros-node
(`std`, `std,rmw-cffi`) and nros-c (shipped features), all `--all-targets`.

`check fast`: 4 of 328 red, all environmental to an agent worktree and none
touched here — `capability-conditionals` and the two XRCE gates want submodules
this checkout lacks (`xrce-source-manifest` dies in its own self-test on a
sandbox `PermissionError`), and `codegen-version-refusal` case E resolves the
MAIN checkout's `nros` off PATH because this worktree has none: measured, with
PATH scrubbed it SKIPs and the file passes. `unsafe-census` went red on the one
new unsafe block and is re-baselined with the reason in its commit.

### What this does NOT close

The wake callback is a per-SESSION slot with one writer. Two executors over one
session still overwrite each other's, and the first teardown clears both. Making
that safe needs either a per-executor slot in the vtable or a "clear only if it
is still mine" read-back, and neither is worth the ABI move for a shape nothing
in the tree builds today.
