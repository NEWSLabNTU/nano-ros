---
id: 545
title: "Two core crates cannot run `cargo test` at all — a dead-code error and a test that no longer compiles — and a third test asserts a build-time knob it does not control"
status: resolved
resolved_in: "same-day fix; phase-350 follow-up"
type: bug
area: testing
related: [issue-0095, issue-0436]
---

## Symptom

Three separate breakages, found while routing around them for a whole
working session:

1. **`cargo test -p nros-node` does not compile.** The workspace lints
   set `-D dead_code`, and `Executor::extra_session_ids` is genuinely
   unread without the `rmw-cffi` feature:

   ```
   error: field `extra_session_ids` is never read
     --> packages/core/nros-node/src/executor/spin.rs:1125:16
   ```

   The field is written unconditionally by `open_multi*`, but its only
   reader — `NodeBuilder::resolve_session_slot`
   (`executor/node_record.rs:213`) — is `#[cfg(feature = "rmw-cffi")]`.
   Default `cargo test -p nros-node` enables no such feature.

2. **`cargo test -p nros-platform-cffi` does not compile.** The lib
   test passes a bare fn item where the generated binding expects an
   `Option`-wrapped function pointer:

   ```
   error[E0308]: mismatched types
     --> packages/platform/nros-platform-cffi/src/lib.rs:1499:62
        expected `Option<unsafe extern "C" fn(*mut c_void)>`,
        found fn item `extern "C" fn(*mut c_void) {noop_callback}`
   ```

   bindgen wraps C function-pointer parameters in `Option`; the test was
   not updated when the timer binding was generated. So the whole crate's
   `cargo test` has been failing, including the port conformance suite
   that is supposed to guard the platform ABI.

3. **`test_entry_slots_exhausted` asserts a knob it does not control.**
   It registers four subscriptions and asserts the fifth returns
   `ExecutorFull`, with the comment "MAX_CBS=4 slots". But `MAX_CBS` is a
   build-time knob (`NROS_EXECUTOR_MAX_CBS`, `build.rs:63`, also
   reachable through `$DOTCONFIG`), so any consumer that raises it turns
   this test red for a reason that has nothing to do with the code under
   test.

## Why (3) matters more than it looks

Cargo reads `.cargo/config.toml` from the current directory **upward**.
A workspace that vendors nano-ros as a subdirectory and sets the knob for
its own image — e.g. `NROS_EXECUTOR_MAX_CBS = "32"` for a many-callback
application — silently applies it to the nano-ros submodule build too.
The test then fails with `left: Ok(HandleId(4)), right: Err(ExecutorFull)`,
which reads like a capacity bug in the executor and is not one.

That is exactly how it was misdiagnosed here: repeated `git stash`
checks "on a clean tree" kept failing, because stashing does not change
the *enclosing directory's* cargo config. A standalone checkout passes.

The test should derive its expectation from `MAX_CBS` rather than
assume 4 — then it is correct under every knob setting instead of one.

## Resolution (2026-08-13)

All three fixed. `cargo test -p nros-node` now runs with no flags —
**261 passed, 0 failed** — and `cargo test -p nros-platform-cffi`
compiles again (2 lib tests, plus the 11-case port conformance suite
that guards the platform ABI).

1. `#[cfg_attr(not(feature = "rmw-cffi"), allow(dead_code))]` on
   `extra_session_ids` — allow it exactly where the reader is compiled
   out, rather than blanket-allowing a field that must stay live
   everywhere the reader exists.
2. `Some(noop_callback)` with the callback declared `unsafe extern "C"`.
3. Drive `test_entry_slots_exhausted` from `crate::config::MAX_CBS`:
   register `MAX_CBS` subscriptions, assert the next one is
   `ExecutorFull`.

## Not covered

The arena may exhaust before the slot table does at large `MAX_CBS`
(each entry is budgeted at the action-client worst case), so (3)'s loop
asserts each registration up to the limit succeeds and reports which
bound was hit if one does not. If a future default makes the arena the
binding constraint, this test should be split rather than loosened.
