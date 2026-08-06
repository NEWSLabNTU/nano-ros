---
id: 460
title: "entry_matrix: two RTOS cells fail — invisible until the nextest timeout stopped killing the run"
status: open
type: bug
severity: medium
area: testing, nuttx, zephyr
related: [issue-0422, issue-0445, phase-295, phase-276, phase-280]
---

## How these surfaced

`entry_e2e::entry_matrix` reported `TIMEOUT [60.003s]` with no output on every
run. It is not a hang: the matrix takes **228 s** because it boots up to 15 RTOS
images (QEMU nuttx/threadx/freertos plus zephyr native_sim) and aggregates its
verdict at the end, so nothing prints until it finishes.

The cause was a stale filter in `.config/nextest.toml`. phase-295 W3.b
consolidated 15 per-cell tests into one test named `entry_matrix`, but the
timeout override still read

```toml
filter = "binary(entry_e2e) and test(zephyr_rust_lifecycle)"
```

— a test name that no longer exists, so the override matched NOTHING and the
whole matrix ran under the default 30 s × 2 ceiling. Fixed to
`filter = "binary(entry_e2e)"` with a 120 s × 3 budget.

With the run allowed to finish, **13 of 15 cells pass** and two fail. Both were
being absorbed by the TIMEOUT verdict — issue 0445's shape, one level up: there
the verdict was staleness, here it is the harness clock, and in both cases a
terminal self-explaining verdict hid a real runtime result behind it.

## The two failing cells

**1. `nuttx-arm/rust/entry_pubsub`**

```
[nuttx-arm rust] native observer never received the entry image's /chatter
```

The test's own note points at phase-280 W3 (`703e840dd`): the Rust entry path's
`entry_net_init` must push the guest IP into `eth0` via `SIOCSIFADDR` before
`Executor::open`, or the image dies in `Transport(ConnectionFailed)`. Worth
checking whether that path still runs in the current image before assuming the
transport is at fault — the observer is native, so either side can be the
silent one.

**2. `zephyr/rust/params`**

```
[zephyr rust params] subscriber never saw the live-read baked param value (250)
```

Note references phase-276 W1 / #128 (`Framework::Zephyr` gained
`apply_param_services`, so launch-baked initials reach the store) and #147/#278
(the observer must be the TYPED int32-sink; the old String listener only matched
while its fixture was a stale pre-W4 Int32 build). Check the observer's type
first — that exact confusion has already produced one false diagnosis here.

## Why they are not in #0422

#0422 indexes the runtime E2E baseline; its `params` row is the interop
`params` binary, not this zephyr entry cell, and it carries no nuttx entry row.
These two were simply never observable while the timeout killed the run.

## Reproduce

```
cargo nextest run -p nros-tests --test entry_e2e      # ~228s, 2 of 15 cells fail
```

## Reporting fixed, both cells still open (2026-08-06)

The two delivery assertions timed out on the OBSERVER and then blamed the guest
— "the embedded LAUNCH-entry runtime delivery did not work" — without ever
showing the guest's output. Either side can be the silent one, and the message
picked one by assertion.

They now print the guest's own log and classify it with
`nros_tests::output::runtime_silence_note`: if the runtime never spoke, the
fault is before delivery and no amount of looking at the transport will find
it. The issue's own advice ("the observer is native, so either side can be the
silent one") is now enforced by the message rather than left to the reader.

**Neither cell is fixed and this issue stays open** — `nuttx-arm/rust/
entry_pubsub` and `zephyr/rust/params` still fail. The first checks are
unchanged: whether `entry_net_init` still pushes the guest IP into `eth0` before
`Executor::open`, and whether the params observer is the TYPED int32 sink.
