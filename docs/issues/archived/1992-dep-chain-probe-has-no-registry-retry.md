---
id: 1992
title: "dep-chain's NuttX cell fails on a crates.io HTTP/2 framing error with no retry at all, although the root `net.retry = 10` names exactly that error and that gate"
status: resolved
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

## Resolution (2026-10-23)

**Why no retry ran: the cell's cargo is not the one `net.retry` was
measured against.** The root config does reach the leaf. Nothing in the
job sets `CARGO_NET_RETRY`, and cargo's config walk from
`examples/qemu-armv7a-nuttx/rust/talker` ends at the repository root. But
`examples/qemu-armv7a-nuttx/rust-toolchain.toml` pins `nightly-2026-04-11`
(cargo `eb94155a9`, 1.96.0-nightly), for `-Z build-std` against the patched
NuttX libc. That cargo's sparse-index fetch returns the curl error wrapped in
`http_async::Error::Easy`, and its `maybe_spurious` downcasts only to a bare
`curl::Error`. So `[16] Error in the HTTP2 framing layer` is never classed as
spurious, and `net.retry` never applies. Cargo fixed it upstream on 2026-04-17,
six days after the pin, in `963787c759` ("add http_async types to spurious
error detection"). Stable 1.99, which the other six cells use, has the fix.

The stderr fingerprints it: `Caused by: curl failed` is that nightly's
`#[error("curl failed")]` on `http_async::Error::Easy`. The string is in the
nightly's binary and not in 1.99's. Issue 1626's `api-parity` samples carry
the same line, because `api-parity` runs `cargo +<pinned nightly> doc`. That
is why its outer loop was the one remedy that held.

**The fix is item 2's second option**, because bumping the nightly is tied
to the patched libc version and belongs in its own change:

- `scripts/lib/cargo_registry.py` is now the one classifier. It moved from
  `scripts/api_parity/extract_rust.py`, which imports it, and it adds a CLI
  for shell callers. Its self-test runs on every invocation, so every
  dep-chain cell exercises it, and its first sample is this run's stderr.
- `scripts/ci/dep-chain-check.sh` retries a failed `cargo tree` whose stderr
  classifies as a registry failure, up to 3 attempts at 20 s × attempt. A
  failure that survives says `could not reach the crates.io registry after 3
  attempt(s) (issue 1992)` and that it is NOT a dep-chain verdict, and the
  cell still fails. A non-registry failure is not retried and prints as
  before.
- The root `.cargo/config.toml` comment no longer claims `net.retry` covers
  this case under the pinned nightly.

Checked with a fake `cargo` driving the probe's loop: two HTTP/2 failures and
then a success give `[ok]` after 3 calls; persistent failures give the
registry `[FAIL]` after 3 calls; a resolver error gives the ordinary `[FAIL]`
after 1 call.
