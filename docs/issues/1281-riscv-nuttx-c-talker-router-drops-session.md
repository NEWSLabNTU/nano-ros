---
id: 1281
title: "`c_riscv_nuttx_talker_delivers_cross_process` fails again: the guest connects and the router drops its session after 28–79 s"
status: open
type: bug
area: [testing, rmw, nuttx]
related: [0820, 0801, 1280, phase-445]
---

## Symptom (as reported, not yet reproduced by the filer)

`nros-tests::c_riscv_nuttx_e2e c_riscv_nuttx_talker_delivers_cross_process`
fails on `main`'s NuttX RISC-V C lane. Reported by the phase-445 agent that
fixed the NuttX-RISC-V link facts (PR #888), 2026-09-11: the guest connects to
the router, and the router drops the session somewhere between 28 s and 79 s
into the run; the native listener never receives `/chatter`.

What the report establishes about the CAUSE is only what it rules out:

- **Not #888.** Same failure with the pre-change FFI layout, both tries, on an
  idle machine, and the two layouts compile to the same image — cargo
  recompiled nothing when switching — and neither #873 nor #880 touches that C
  lane.
- **Not a museum binary** in the sense of issue 0820's first root cause: the
  image was built fresh by the fixture lane in that session.

## Why it is filed separately from 0820

Issue 0820 (resolved) covered this same test twice:

1. a stale artifact — a cmake custom command ran `cargo` with no rebuild edge
   (fixed, gated by `check-cargo-custom-command-depfile`);
2. a **domain split**: the guest's default node (from `nros_support_init`)
   declared on domain 0 while the `nros_node_init` node and its publisher
   declared on domain 1 — one session, two domains, the mirror image of issue
   0801 — so the listener on domain 0 never matched.

The symptom here is different — the router DROPS the session, which neither
cause produced — so it is a new defect or a new face of the second one, not a
recurrence of the first. It needs a router-side log to tell which.

## Reproduce

```sh
just nuttx build-riscv-c            # in a checkout whose NUTTX_DIR is its own (issue 1280)
cargo nextest run -p nros-tests --test c_riscv_nuttx_e2e \
  c_riscv_nuttx_talker_delivers_cross_process --retries 0
```

with `ZENOHD_LOG=debug` on the router, to see whether the drop is a lease
expiry (the guest stops sending keepalives — the zenoh-pico lease path CLAUDE.md
records for Zephyr's per-fd send/recv serialisation) or a protocol close, and
whether the guest's tokens still split across domains as in 0820.

## What is NOT established

- Whether this is a lease timeout, a transport close, or a guest crash.
- Whether the 0820 domain split has returned.
- Whether it reproduces on a second host.
