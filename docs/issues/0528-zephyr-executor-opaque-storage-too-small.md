---
id: 528
title: "Zephyr leaves fail `EXECUTOR_OPAQUE_U64S too small for Executor + backing`, blocking the tier-2 fixture build"
status: open
type: bug
area: zephyr
related: [issue-0472, issue-0464, phase-343, phase-336]
---

## Symptom

`just build-test-fixtures lane=tier2` fails in the zephyr module. The leaves die
inside `nros-c`'s compile-time assert:

```
error[E0080]: evaluation panicked: EXECUTOR_OPAQUE_U64S too small for Executor +
backing — increase NROS_EXECUTOR_ARENA_SIZE or NROS_EXECUTOR_MAX_CBS, or adjust
the overhead in build.rs
  --> packages/api/nros-c/src/executor.rs:56:15
   |
56 |   const _: () = assert!(
57 | |     core::mem::size_of::<nros_node::ExecutorInlineStorage>()
58 | |         <= EXECUTOR_OPAQUE_U64S * core::mem::size_of::<u64>(),
```

Both languages and both RMWs are affected — the failing targets include
`build-c-listener-xrce`, `build-rs-talker-xrce`, `build-rs-listener-xrce`,
`build-rs-service-server-xrce`, `build-rs-action-client-zenoh`,
`build-ws-mixed-entry-zenoh`. It takes the whole `zephyr` module down, and
because zephyr is an order-only prerequisite of every other platform, it takes
the tier-2 fixture build with it.

## What has been ruled out

**Not a stale sizes probe**, which is this symptom's usual cause. The shared
probe dir (`build/sizes-probe`, phase-343 W1) was deleted and one failing leaf
rebuilt from scratch: it fails identically, 4 × the same assert. So it is not
the mtime-newest-wins hazard phase-336 W7 created and phase-343 W1 fixed.

**Not phase-346 W2/W3** (the zephyr-lang-rust gpio patch + the new Rust
Cortex-M witness). Checked out `f05d83cb0` — the upstream commit BEFORE that
work — and rebuilt the same leaf: identical failure, same count. The attribution
was measured rather than reasoned, because the timing invited the opposite
conclusion.

So it is either an earlier upstream change or a latent condition this tree
reached; no bisect has been run.

## Why it matters

It blocks `build-test-fixtures lane=tier2`, and therefore `just ci-matrix` — the
tier-2 sweep cannot run at all while it stands. It is the second gate of this
family in a month: issue 0464 removed the two fallbacks that used to HIDE a bad
probe (a poll of the outer target dir, and a table of committed constants that
had rotted ~11 % below the real `size_of::<Executor>()`), which is why this now
fails loudly instead of emitting a short buffer. That is the right behaviour —
the report is the fix working, not the fix breaking.

## Where to look

* `packages/api/nros-c/build.rs` emits `EXECUTOR_OPAQUE_U64S` from the size
  probe; `nros-node::ExecutorInlineStorage` is what must fit. A feature set that
  grows the executor (callback-group count, arena) without the probe seeing the
  same features gives exactly this.
* The probe is keyed `(rustc slug, target, features)`. `build/sizes-probe` on
  this host holds TWO rustc slugs (`1.96.0-nightly`, `1.97.1`) — the zephyr Rust
  leaves build on nightly via `-Z build-std`, the C leaves do not. Worth
  confirming the zephyr C leaf probes under the slug it actually compiles with.
* issue 0472 (13 unguarded opaque-storage macros) is the neighbouring surface.

## Not yet done

No bisect, and no attempt at the suggested remedies
(`NROS_EXECUTOR_ARENA_SIZE` / `NROS_EXECUTOR_MAX_CBS`) — raising a bound to
silence an assert is the wrong first move when nobody has established WHY the
storage grew.
