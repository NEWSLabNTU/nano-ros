---
id: 1924
title: "`nros build` refuses both async Linux examples on a fresh checkout: `futures`, `futures-channel`, `futures-io` and `tokio-macros` are in no pinned range"
status: resolved
type: bug
severity: medium
area: [cli, build, examples]
related: [issue-1756, issue-1873]
found: 2026-10-16
---

## Measured

On a fresh worktree at `d10d92ee8d`, after `nros sync`, `nros build` refuses
both async examples after a successful compile:

```
examples/linux/rust/action-client-async: `futures` 0.3.34 is in no pinned range — `nros update --crate futures` from this leaf adds it to the pin set
examples/linux/rust/action-client-async: `futures-core` 0.3.34, pinned at 0.3.32 — ...
examples/linux/rust/action-client-async: `tokio-macros` 2.7.2 is in no pinned range — ...
examples/linux/rust/service-client-async: `tokio-macros` 2.7.2 is in no pinned range — ...
```

The full list for `action-client-async` is `futures`, `futures-channel`,
`futures-io` and `tokio-macros` with no pinned range, plus `futures-core`,
`futures-sink`, `futures-task` and `futures-util` resolved at 0.3.34 against a
0.3.32 pin. `service-client-async` refuses on `tokio-macros` alone.

`locks/cargo-registry.lock` holds `tokio` 1.53.1 but not `tokio-macros`, and
none of the four unpinned `futures*` crates. These two leaves are the only ones
in the tree that name `futures` or tokio's `macros` feature, and their locks are
untracked (they depend on generated message crates), so no locked root ever
resolved those crates into the pin set.

The four off-pin crates are the ones the drift check realigns with
`cargo update --precise`. The four with no range are what it cannot realign,
so it refuses (phase-485 W4, `orchestration::drift`).

## What it is NOT

- **Not a CI blocker.** The fixture build compiles Linux Rust leaves with a bare
  `cargo build` (`just/linux.just`, `build_one`), which the drift check does
  not reach. The fixtures for `linux_async_roundtrip_e2e` build in CI. Only the
  user-facing `nros build` refuses.

## What would close it

The pin set covers what these two leaves resolve. That means `nros update
--crate <name>` from each leaf, as the refusal says, with the resulting
`locks/cargo-registry.lock` diff reviewed. `nros build` then succeeds in both
leaves on a fresh checkout. If a leaf with an untracked lock can resolve crates
no locked root does, `check-cargo-registry-pins` should also see them (the
same gap issue 1873 closed for retired ranges, in the other direction).

## Resolution

Two parts: the pins, and a tool defect that pinning them uncovered.

**The tool could not do what the refusal told the user to do.** The `futures`
crates are released in lockstep, and each one requires the others at its own
version: `futures-util` 0.3.34 requires `futures-core`, `futures-channel` and
`futures-io` at `^0.3.34`. Two loops moved such crates one at a time, in lock
order:

- `realign_leaf`, called by `nros update`;
- `enforce_cargo_root`, the realignment step of the drift check in
  `nros build`.

In lock order, `futures-core` comes before `futures-util`, so `futures-core`
is refused while `futures-util` still requires 0.3.34. Neither loop came back
to retry it. `nros update --crate futures --precise 0.3.32` had the same
problem one step earlier: it moved only `futures` and left `futures-channel`
at 0.3.34. Every follow-up `--crate futures-channel` was then refused by
`futures-util`.

The fix:

- One shared helper, `orchestration::drift::realign`, retries refused moves
  until a round moves nothing. Both loops use it.
- With `--precise`, `nros update --crate` moves the named crate together with
  every crate of the leaf that is off its pin.
- Unit tests cover a family listed in the order that blocks a single pass, and
  a crate that never moves.

**The pins.** Taken from `examples/linux/rust/action-client-async` with
`nros update --crate futures|futures-channel|futures-io --precise 0.3.32`.
`tokio-macros` 2.7.2 was folded in by the first run. All four are new ranges,
pinned to match the `futures-*` 0.3.32 already in the set, so no tracked root
lock moved. Because no locked root resolves them, they are also named in
`locks/leaf-only-pins.toml`.

**Measured after the fix:**

- From a fresh seed (leaf lock removed, then `nros sync` and `nros build`),
  both async leaves build with no drift and no realignment, at `futures*`
  0.3.32 and `tokio-macros` 2.7.2.
- From the old all-0.3.34 leaf lock, `nros build` prints `realigned 7
  crate(s)` and succeeds. The single pass would have left `futures-core` off
  its pin and refused.
- `check-cargo-registry-pins`: 497 pins, one per range.

The last paragraph of "What would close it" (the gate seeing leaf-only crates
from untracked locks) is not done. The keep list is still hand-written.
