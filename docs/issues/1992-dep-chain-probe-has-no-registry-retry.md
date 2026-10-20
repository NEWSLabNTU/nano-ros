---
id: 1992
title: "dep-chain's NuttX cell fails on a crates.io HTTP/2 framing error with no retry at all, although the root `net.retry = 10` names exactly that error and that gate"
status: open
type: bug
area: [ci, tooling]
severity: low
found: 2026-10-20
related: [1625, 1626]
---

## Measured

- Run: post-submit 39403426847 (push, 2026-10-20 09:48 UTC, main at
  `be91e3949`).
- Job: 118565369353, `dep-chain (7 board×rmw cells)`, step
  `just check dep-chain`. The other job in the run passed.
- The failing cell is `qemu-armv7a-nuttx:zenoh`, the same cell as all three
  occurrences in issue 1625. Its stderr, as printed by the probe:

```
  [FAIL] cargo tree did not resolve (default features), exit 101; last 20 lines of its stderr:
      Updating crates.io index
      Updating git repository `https://github.com/NEWSLabNTU/ros-launch-manifest.git`
  error: failed to get `hash32` as a dependency of package `heapless v0.8.0`
      ... which satisfies dependency `heapless = "^0.8"` of package `std_msgs v0.0.0 (examples/qemu-armv7a-nuttx/rust/talker/generated/std_msgs)`
      ... which satisfies path dependency `std_msgs` of package `nuttx_rs_talker v0.1.0 (examples/qemu-armv7a-nuttx/rust/talker)`
    Caused by:
      failed to load source for dependency `hash32`
    Caused by:
      unable to update registry `crates-io`
    Caused by:
      download of ha/sh/hash32 failed
    Caused by:
      curl failed
    Caused by:
      [16] Error in the HTTP2 framing layer
```

The cell started at 10:01:44 and `nros setup` finished at 10:02:04, so the
probe failed within seconds.

## Why this is not just issue 1625 again

Issue 1625 ended by naming this a crates.io download transient and deferring
the remedy to issue 1626's class. 1626 then raised `[net] retry = 10` in the
root `.cargo/config.toml`. That file's comment says cargo retries "an HTTP/2
framing error" and lists "`dep-chain`'s `cargo tree`" among the reds it was
written for.

This run shows the retry did not happen. The probe prints the last 20 lines
of `cargo tree`'s stderr, and those 20 lines begin with cargo's first output
line, `Updating crates.io index`. So this is the whole stderr. It contains no
`warning: spurious network error (N tries remaining)` line, which cargo
prints before each retry. Cargo gave up on the first failure.

Possible reasons, none of them established:

- The leaf `examples/qemu-armv7a-nuttx/rust/talker` does not see the root
  config. For example, an environment variable such as `CARGO_NET_RETRY`
  overrides it, or the probe's config walk stops before the root.
- Cargo does not treat this failure as spurious on the sparse-index
  download path, whatever `net.retry` says.

Only 1626's `api-parity` tool has an outer retry loop (`is_registry_failure`
in `scripts/api_parity/extract_rust.py`). `scripts/ci/dep-chain-check.sh`
runs `cargo tree` once.

## What it is NOT

- Not a dependency-graph defect: the error is a download, and the same cell
  resolved on the previous post-submit (39402858063, at `868b847c1`).
- Not a regression from `be91e3949`: the probe failed before compiling
  anything.

## What would close it

1. **Find out why no retry ran.** Run the probe's `cargo tree` in that leaf
   with `cargo config get net.retry` (nightly `-Zunstable-options`) or with
   `CARGO_LOG=cargo::util::network=debug`. That shows whether the root config
   applies and whether this error is classed as spurious.
2. **Then give the probe the retry it lacks.** Either make `net.retry` reach
   it, or wrap the probe's `cargo tree` in the same "retry a registry failure,
   then report it as one" loop that `api-parity` has. A registry failure that
   survives the retries should still fail, and say it was the registry.
