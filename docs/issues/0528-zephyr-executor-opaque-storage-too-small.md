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


## Cause FOUND 2026-08-13 — the shared sizes-probe key ignores the sizing knobs

Not a stale probe (already ruled out above) but a MIS-KEYED one.

`probe_key()` in `nros-sizes-build` hashes `(target, features)` under a
`rustc_slug` directory. What it does not hash is anything that changes the SIZE
being probed. Since issue 0460, `nros-node`'s `env_usize()` resolves
`NROS_EXECUTOR_MAX_CBS` and friends from the env OR from Zephyr's `$DOTCONFIG`,
and this tree has both kinds of leaf at the same coordinate:

```
examples/workspaces/features/src/zephyr_rust_{params,qos,lifecycle}_entry
    prj.conf: CONFIG_NROS_EXECUTOR_MAX_CBS=16
examples/zephyr/rust/{talker,listener,action-*,service-*}
    no knob — the crate default, 4
```

Same target, same features, therefore the SAME probe dir. Whichever leaf probes
first writes `EXECUTOR_SIZE` for its own `MAX_CBS`; the other reuses it. In one
order that is merely oversized; in the other the 16-CBS leaf compiles against a
constant sized for 4 and dies on `EXECUTOR_OPAQUE_U64S too small`.

That explains the two things that made this look latent: it is ORDER-dependent,
and rebuilding the failing leaf from scratch does not clear it, because the
poisoned state is in the SHARED dir, not the leaf.

The function's own header comment already said "sharing without this key is
worse than not sharing" about the feature axis. The knobs are the other half of
that key.

## Fix landed

`probe_key()` now also mixes `knob_identity()`: every `NROS_*` env var, plus
every `CONFIG_NROS_*` line of `$DOTCONFIG` when Zephyr set one — the same two
routes the consuming crate resolves through, so the key cannot disagree with the
compile. Verified arithmetically that the two leaf classes now land on different
keys (`9a7cf62d91362c22` vs `020c64f9f2153b49`).

**Not yet verified end to end.** `just build-test-fixtures lane=tier2` has not
been re-run, so this issue stays OPEN until the zephyr module builds. The cause
and the mechanism are established; what is missing is the lane.


## Lane run 2026-08-13 — the assert is GONE; tier 2 blocks on something else

`just build-test-fixtures lane=tier2` from a wiped `build/sizes-probe`:

```
EXECUTOR_OPAQUE_U64S asserts   0     (was: six leaves, whole module down)
reached                        leaf 12 (build-rs-action-client-xrce)
```

It then fails at LINK on `undefined reference to nros_platform_clock_{ms,us}` —
RFC-0073's rename reaching a consumer it missed, filed as issue 0548. Different
defect, same lane.

**Both probe orders verified** (the bug was order-dependent):

| order | probe dirs | assert |
| --- | --- | --- |
| knob=16 first, then default | 2 | none |
| default first, then knob=16 (the poisoning order) | 2 | none |

One correction to the earlier note here: my FIRST attempt at the second order
showed a single probe dir and no failure, and that was not a pass — I had wiped
`build/sizes-probe` but not the cargo target dir, so the second build was cached,
`build.rs` never re-ran and nothing re-probed. Re-run from clean, both crates
recompile and a second probe dir appears.

Also retracted: the claim that existing checkouts hold "poisoned" dirs needing a
manual `rm -rf build/sizes-probe`. A knob-LESS leaf mixes nothing extra, so its
key is unchanged (`9a7cf62d91362c22` before and after) while the knob-bearing
leaf moves to a new one — the harmful direction (big-knob leaf reading
small-knob sizes) simply cannot recur. The only residue is a knob-less leaf
possibly reading OVERSIZED sizes, which the assert accepts; that wastes opaque
storage rather than breaking a build.

This issue stays OPEN until tier 2 completes, since that is its stated
acceptance — but its own mechanism is fixed and demonstrated at lane scale.
