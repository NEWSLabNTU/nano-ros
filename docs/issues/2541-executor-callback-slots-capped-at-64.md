---
id: 2541
title: "An executor holds at most 64 callbacks, because its ready bits are one u64"
status: open
type: limitation
severity: low
area: [executor, runtime]
related: [rfc-0084, rfc-0108, phase-555]
found: 2026-11-14
---

## What

`MAX_CALLBACK_SLOTS` is 64 (`packages/core/nros-node/src/executor/spin.rs`).
The executor carries a callback's index in a `u64` active mask
(`1u64 << i`), RFC-0084's pushed arrival bits are one `u64` per executor
(`take_entity_ready_bits`), and both ready sets of RFC-0108 assert
`N <= 64` (`FifoReadySet`, `EdfReadySet` presence bitmap,
`executor/ready_set/mod.rs`). So one executor can register at most 64
callbacks, whatever the per-entry sizing says.

RFC-0084 left this as its open question 3: widening to `BitSet<N>` needs a
lock-free multi-word set-bit (or a per-word atomic array) on the arrival
path, which runs in a backend callback or an ISR.

## Why it is filed now

Phase-555 W3 flipped RFC-0084 to Stable, and the rule for that flip is that
no open question stays only in the RFC. No image in the tree is near 64
today; the cap is a ceiling, not a measured failure.

## Acceptance

Either an executor with more than 64 callbacks registers, dispatches in rank
order and receives pushed arrivals for all of them, measured; or the cap is
recorded as the design (a refusal at registration that names the limit, and
a sentence in RFC-0084) and this issue is closed as wontfix.
