---
id: 96
title: "In-process (same-executor) node-to-node delivery does not happen — pub/sub AND service"
status: resolved
type: bug
area: core
related: [phase-263]
resolved_in: "issue-0096 fix — host zenoh-pico same-session loopback"
---

## Summary

Two nodes registered on the **same** `Executor` (one `nros::main!` entry → one zenoh-pico
session) did not deliver to each other. A same-session subscriber got **zero** callbacks from
a same-session publisher; a same-session service client's blocking `call_for_name` query never
reached the same-session queryable/server, so it timed out. External processes received
normally. This affected every single-entry multi-node demo whose nodes are meant to talk: the
phase-263 A1 service showcase, the B1 safety workspace (talker → safe_listener), and the basic
talker+listener quickstart entry. The A1/B1 Track-D demos were therefore split into
cross-process entries (the supported topology at the time).

## Root cause

zenoh-pico's same-session loopback (publisher→local-subscriber and query→local-queryable) is a
**compile-time** feature, `Z_FEATURE_LOCAL_SUBSCRIBER` / `Z_FEATURE_LOCAL_QUERYABLE`. The
generated config header (`packages/zpico/nros-zpico-build/src/lib.rs`) hardcoded **both to 0**
for every target, so the loopback path (`src/session/loopback.c`) was compiled out and
same-session callbacks never fired. Nothing in the Rust/C shim could compensate — zenoh-pico's
C API does not expose locality at declaration time; same-session delivery is gated solely by
these flags.

## Fix

`nros-zpico-build` now enables `Z_FEATURE_LOCAL_SUBSCRIBER` / `Z_FEATURE_LOCAL_QUERYABLE` for
**host / native** targets (`!is_embedded_target(target)`) and keeps them **0 on embedded**
(the loopback + write-filter code is RAM-unbudgeted there; a single process rarely needs
in-process multi-node on a microcontroller). The flags are purely additive: a local match sets
the zenoh-pico write filter to `OFF` (`src/net/filtering.c`), so the network publication to
**external** subscribers is preserved (verified — external `/sum` delivery still works).

Regression guard: `examples/workspaces/rust/src/native_service_inprocess_entry`
(`service_inprocess.launch.xml`) boots `add_server` + `add_client` in ONE process;
`tests/service_roundtrip_inprocess_e2e.rs` asserts the server-computed sums 1,2,3 reach an
external `/sum` subscriber — proving the same-session client→server→reply→publish chain.
Confirmed red (flags 0) → green (flags 1). The minimal 2-node entry is used instead of the
6-node `native_showcase_entry`, whose action nodes fail to register in one process (a separate
limitation, unrelated to 0096).

## Remaining (embedded)

In-process multi-node delivery on **embedded** stays off by design (RAM). If an embedded
single-process multi-node topology ever needs it, enable the flags per-target behind a size
probe (cf. issue 0110 on per-entry executor sizing) — track as a new enhancement, not this bug.
