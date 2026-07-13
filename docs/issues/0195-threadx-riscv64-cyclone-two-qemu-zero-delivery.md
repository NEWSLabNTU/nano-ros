---
id: 195
title: "threadx-riscv64 cyclone two-qemu pubsub: boots, 0 messages delivered"
status: open
type: bug
area: threadx-riscv64
related: [phase-287]
---

## Summary

`test_threadx_riscv64_cyclonedds_two_qemu_pubsub` fails deterministically
(3/3 nextest retries, near-solo run) with the listener receiving 0 samples:

```
nros-tests::threadx_riscv64_qemu test_threadx_riscv64_cyclonedds_two_qemu_pubsub
  Listener: expected at least N received messages, got 0
```

Observed 2026-07-14 during the phase-287 W7 full-matrix sweep on freshly
rebuilt fixtures (`just build-test-fixtures` green, threadx_riscv64 stage OK).
Both QEMU instances boot (the failure is the delivery assert, not a readiness
gate).

## Leads

- Cyclone two-instance pairs are the classic identical-identity trap: baked
  IP/MAC → identical entropy → SPDP sees the peer as itself (the zenoh ZID
  analogue), or a shared domain with another lane. Check the pair's baked
  identities + domain first (archived 0161 / the cyclone fixture-pair 50–58
  domain convention).
- The riscv64 lane has prior art for silently-broken rebuilds: archived 0131
  (NULL c_app_main on rebuild), 0138 (`--allow-multiple-definition`), and the
  sizes-header race recurrence (agent-memory note) — verify the images embed
  a sane app before chasing the network.

## Repro

```sh
just build-test-fixtures
cargo nextest run -E 'test(test_threadx_riscv64_cyclonedds_two_qemu_pubsub)'
```
