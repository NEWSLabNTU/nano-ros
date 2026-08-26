---
id: 820
title: "`c_riscv_nuttx_talker_delivers_cross_process` fails on tier 2 — the
  native listener receives none of the riscv-nuttx C talker's /chatter"
status: open
type: bug
area: rmw, testing
related: [issue-0199]
---

## Symptom

Tier 2 (`just ci-matrix`), 2026-08-27. Of 1704 tests, two failed; one
(`test_qemu_rtic_service_e2e`) passes solo and is the usual in-sweep QEMU flake.
This one does not.

```
nros-tests::c_riscv_nuttx_e2e c_riscv_nuttx_talker_delivers_cross_process
  panicked at packages/testing/nros-tests/tests/c_riscv_nuttx_e2e.rs:94:13:
  native listener never received the riscv-nuttx C talker's /chatter — the
  riscv C-lane runtime delivery did not work (archived 0199 fixed the link;
  this is the runtime half)
```

Reproduced SOLO twice, `--retries 0`, 90.3 s each (the test's own
`wait_for_output_count(..., 3, 90s)` budget). The test already carries
`retries = 1` in `.config/nextest.toml` and failed in-sweep with it.

## What is established

* **Not a stale fixture.** The riscv C talker
  (`examples/qemu-riscv-nuttx/c/talker/build-zenoh/c_talker`) was rebuilt
  2026-08-26 23:54 by the `lane=tier2` fixture build, after the tree's last
  source edit. Checked because a museum binary is the usual explanation here
  (#0786 was exactly that).
* **The listener side starts.** The panic is at line 94
  (`wait_for_output_count`), not line 78 (`never became ready`), so the native
  listener spawned, subscribed and printed its readiness marker. What is missing
  is inbound traffic.
* **Not an in-sweep flake.** Its sibling in the same run was; this one fails
  alone on an otherwise idle host.

## What is NOT established

* **Whether it fails on `origin/main`.** It was found on a branch carrying
  phase-384 W1 (error-variant propagation in `nros-node`). That change is
  argued below to be irrelevant, but the control — revert, rebuild the riscv
  and native fixtures, re-run — was NOT executed, so this is reasoning, not
  measurement.
* **Where delivery stops.** Whether the guest publishes at all, whether the
  zenoh router sees it, or whether it is lost between router and listener. The
  guest console is buffered by `ManagedProcess` and is not persisted to
  `test-logs/`, so `--no-capture` shows nothing useful. Getting that output is
  the obvious first step.

## Why phase-384 W1 is argued not to be the cause

W1 changed six `map_err(|_| ...DeserializationError)` to
`map_err(NodeError::Transport)` in `nros-node`. All six are on RECEIVE paths
(`Subscription::try_recv` / `try_recv_raw`, `RawSubscription::try_recv_raw`,
`try_recv_raw_with_attachment`, `try_recv_validated`,
`try_recv_feedback_raw`). The talker in this test is a C PUBLISHER, so none of
them are in its path; the listener is declarative Rust
(`nros::main!(spin = "forever")`), where the change alters an error's NAME and
not whether bytes arrive. The failure is zero of three samples delivered, which
is not a shape an error label can produce.

Note the change is NOT purely cosmetic at the C boundary, which is worth
recording: `nros_c::support` maps `MessageTooLarge`/`BufferTooSmall` to
`NROS_RET_FULL` and `DeserializationError` to `NROS_RET_ERROR`, so a C consumer
of a raw receive now gets `NROS_RET_FULL` where it used to get
`NROS_RET_ERROR`. That is the intended improvement, but it is a behaviour
change on a public return code.

## Reproduce

```
source ./activate.sh
just build-test-fixtures lane=tier2      # or: just nuttx build-riscv-c
cargo nextest run -p nros-tests --test c_riscv_nuttx_e2e --retries 0 \
    -E 'test(c_riscv_nuttx_talker_delivers_cross_process)'
```

Needs `zenohd` and `qemu-system-riscv32`; the test skips cleanly without them.
