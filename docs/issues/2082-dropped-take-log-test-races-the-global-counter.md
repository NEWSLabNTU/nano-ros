---
id: 2082
title: "`dropped_take_log_tests` reset a process-global counter that other tests in the binary also advance, so the gate goes red under load"
status: open
type: bug
area: [testing, executor, ci]
severity: low
found: 2026-10-26
related: [issue-0757, phase-505]
---

# The dropped-take log tests race the global drop counter

**Home:** [phase-505](../roadmap/phase-505-ci-lanes-reach-and-infra-robustness.md) W2.

## Problem

`report_dropped_take` (`packages/core/nros-node/src/executor/arena.rs`) logs
the first dropped take and then every 64th:

```rust
let n = DROPPED_TAKES.fetch_add(1, Ordering::Relaxed);
if n != 0 && !n.is_multiple_of(64) {
    return;
}
```

`DROPPED_TAKES` is a process-global `static` (on purpose: issue 0757). The
tests in `dropped_take_log_tests` serialize among THEMSELVES (`SERIAL`), store
`0` into the counter, call `report_dropped_take` once, and assert exactly one
line was logged. Any other test in the same `nros-node` lib binary whose path
reaches `report_dropped_take` can advance the counter between that store and
that call. The call then sees `n != 0` and logs nothing:

```
executor::arena::dropped_take_log_tests::an_unknown_size_and_a_staging_refusal_print_as_unknown
assertion `left == right` failed: the first drop must be reported: []
  left: 0
 right: 1
```

That is `just ci gate` step 7, `check-build` → `node-std-tests`, on
2026-10-26, on a branch that changes nothing in `nros-node`. The same module
passed 5 of 5 runs alone, and `just check node-std-tests` passed 3 of 3 with
the host otherwise idle. The failing run had 26 build gates running in
parallel.

`node-std-tests` is in the `ci gate` lane, which the merge queue runs, so this
can eject a batch for a cause that is not in the code being merged.

## Fix direction

Make the assertion independent of the shared counter. Two options:

- split the decision from the effect: a pure `drop_report_line(err, cap,
  recorded, n) -> Option<String>` that the tests call with `n = 0` (and with
  `n = 1` / `n = 64` for the rate limit, which today is not tested at all),
  with `report_dropped_take` as the thin wrapper that owns the counter; or
- give the tests their own counter (a parameter, or a `cfg(test)` seam), never
  the production static.

Either way the test then asserts what it names. Acceptance: the module passes
under `cargo test -p nros-node --lib -- --test-threads=64` repeated.
