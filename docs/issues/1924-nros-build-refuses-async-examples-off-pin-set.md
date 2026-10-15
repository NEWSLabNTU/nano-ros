---
id: 1924
title: "`nros build` refuses both async Linux examples on a fresh checkout: `futures`, `futures-channel`, `futures-io` and `tokio-macros` are in no pinned range"
status: open
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
