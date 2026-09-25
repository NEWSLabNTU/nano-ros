---
id: 1116
title: "rustdoc diagnostics in crates the book does not publish — filed as ~70
  and five crates that cannot document, measured three weeks later as 179 and
  twelve"
status: resolved
type: tech-debt
area: docs, core
severity: low
found: 2026-09-06
related: [1110, 0319, 0896, phase-452]
resolved: 2026-09-25
---

# What issue 1110 fixed, and what it deliberately did not

Issue 1110 put rustdoc on a lane a pull request runs (`just check rustdoc-links`, in the `compile-smoke`
job). Its scope is the **deployed** crate set — the six crates `just book`
publishes — because that is what keeps the docs deploy green.

A workspace-wide pass is a different story. Measured on a cold pass,
2026-09-06:

```
$ cargo doc --no-deps --workspace $HOST_UNCHECKABLE
… ~70 diagnostics …
error: could not document `nros-node`
error: could not document `nros-orchestration-ir`
error: could not document `nros-platform`
error: could not document `nros-serdes`
error: could not document `zpico-alloc`
```

Five crates do not document at all. The diagnostics fall into three kinds, and
they need different fixes:

1. **A link to an item that no longer exists** — the 1110 shape, e.g.
   `crate::Node::session_mut` (3×), `crate::Executor::register_lifecycle_node`
   (2×), `Executor::open_multi`. Each is a rename or deletion whose prose was
   not moved with it.
2. **A public item linking to a PRIVATE one** — ~14 of them
   (`ExecutorSizing` → `carve`, `loan` → `TxArena::release`, `dispatch_callback`
   → `DispatchSlot`). rustdoc rejects these because a reader of the public page
   cannot follow the link. The fix is prose or `pub(crate)` visibility, not a
   link.
3. **Markdown that reads as a link and is not one** — `[iu]{8,16,32,64}` in
   `nros-serdes::schema`, `[must_use]`, `[planned]`. Backticks, not brackets.

## Why this is not urgent, and not nothing

Nothing publishes these crates, so no deploy is broken by them and no user sees
a 404. What they cost is the ability to WIDEN the gate: `rustdoc-links` cannot
grow to the workspace while 70 diagnostics stand, so every crate outside the
published six keeps the property 1110 was about — a doc link can rot with
nothing to say so.

## Doing it

A ratchet is the wrong tool here (the count only goes down, and the fixes are
mechanical). Better: fix by kind, in three commits, then widen
`NROS_RUSTDOC_CRATES` in `scripts/build/rustdoc-set.sh` to the workspace and
delete this issue's reason for existing. The gate is ~3 s warm on six crates and
was measured at 3.2 s warm on the whole workspace, so cost is not the obstacle.

## Resolved — phase-452 W4, 2026-09-25

Fixed, all of it, and the numbers above were STALE by the time anyone came to
implement them. Re-measured with the deployed feature set and `--keep-going`:
**179 diagnostics, twelve crates that could not document at all**, not "~70 and
five". Two separate reasons, both worth knowing before trusting a count in an
issue:

* The `~70` was taken with NO `--features`, which documents less code. That
  command still answers 140 today.
* The `five` was the count `cargo doc` REACHED. Without `--keep-going` the pass
  stops at the first crate that fails, which is the same property this issue's
  parent (the `rustdoc-links` gate) was written about — so the list was never
  the list, it was the prefix.

The rest is three weeks of ordinary drift on a surface nothing checked, which
is the argument for the gate rather than for the cleanup.

### What the diagnostics actually were

Mostly not rotten LINKS — rotten PROSE. `crate::Node` after the node type moved
to `NodeHandle`; a `run` entry point three board crates still document in the
present tense that phase 212.N.7 deleted; a `BoardInit` trait
`nros-board-common`'s own module docs advertise and has not declared since
212.N.1; `RunScope::build_lane` for what is `CiLane::build_lane`; and
`nros_executor_register_client`, named in four places including the committed
cbindgen header, for a symbol that is `nros_executor_add_client`. A doc-link
gate is cheap and it catches these because a renamed item stops resolving.

### The remedy is NOT the one written under "Doing it"

That section proposed widening `NROS_RUSTDOC_CRATES` to the workspace.
phase-452 W4 did not, and the reason is the reason that list is narrow: it is
what `just book` deploys and what the pull-request lane answers for, so a crate
the book does not publish must not be able to make a PR red over a deploy that
is fine. The workspace pass is a SECOND scope instead —
`just check rustdoc-workspace` / `scripts/check-rustdoc-workspace.sh` — sharing
this file's feature set and splicing the deployed source table into its own, so
the two can never disagree about a shared precondition. It runs in gate.yml's
`compile-smoke` job, the one lane that provisions every source it needs, and
costs 8.6 s warm.

It also does NOT use a ratchet, and this issue was right about that: a count
that may only shrink is a weaker statement than a count that is already zero.

### Two things the work corrected

* **`-D warnings` is load-bearing, and it is not what the workspace already
  had.** Deny reaches only the crates that write `[lints] workspace = true`,
  and `cargo doc` exits 0 over a warning — so a gate without the flag would
  have gone green over the 17 diagnostics the board crates were carrying and
  over `nros-cargo-profile`'s two ambiguous ``[`env`]`` links. Those two were
  found BY adding the flag.
* **The source table had to grow, exactly as `rustdoc-set.sh` predicted.** The
  workspace pass runs `cyclonedds-sys`'s and `nros-rmw-xrce-cffi`'s build
  scripts as well, so cyclonedds, micro-cdr and micro-xrce-dds-client join
  zenoh-pico. Measured one submodule at a time: `mbedtls` and `px4-rs` are not
  needed, and a row for a source nothing needs would turn an ordinary checkout
  into a reported skip for nothing.
