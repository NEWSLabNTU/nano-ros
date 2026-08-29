---
id: 879
title: "the serial link cannot resynchronise after a peer reset — the router
  loops on `Unexpected Init flag in message` until it is restarted"
status: resolved
type: bug
area: rmw
related: [issue-0852, issue-0839]
---

## Problem

Reset the board while the `rmw_zenoh` router holds the serial link, and the
router enters a repeating error it never leaves:

```
ERROR rx-0 ThreadId(06) zenoh_link_serial::unicast:
  Read error on Serial link serial//dev/ttyUSB0 => serial/<uuid>:
  Unexpected Init flag in message
  at io/zenoh-links/zenoh-link-serial/src/unicast.rs:164
```

It repeats every ~18 s indefinitely. The board is healthy and re-sending `INIT`
as it should on a fresh boot; the router still holds the previous link-level
session and treats an `INIT` arriving mid-session as a protocol violation rather
than as a peer that restarted.

**The only recovery is restarting the router.** No timeout, lease expiry or
retry on either side clears it.

## Why this is not the same as a lost session

A TCP peer that reboots gives the router a closed socket, so the transport is
torn down and rebuilt. A serial link is a permanently-open file descriptor: the
pipe never "closes", so there is no event that tells the router its peer is
gone. The `INIT` byte IS that event, and it is currently rejected instead of
acted on.

## Why it matters beyond convenience

It silently corrupts measurements. Any experiment that resets the board — every
A/B of two firmware builds, every reconnect test — must restart the router
between runs, or the second run is conducted against a router stuck in this
loop and reports a dead link that has nothing to do with the firmware under
test. Several results during [issue 0852](0852-*) had to be discarded for
exactly this reason before the pattern was recognised.

It is also a real deployment property, not only a lab annoyance: a board that
watchdog-resets in the field does not come back until someone restarts the
router.

## Distinguishing it from the identity problem

[Issue 0864](archived/0864-board-zid-is-identical-on-every-boot.md) had a
similar symptom — measurements that depended on router history — and is fixed.
This is a **different** layer and survives that fix. 0864 was the zenoh session
identity; this is the serial link-level framing state, one layer below, and it
does not care what zid the board presents.

## Fix direction

`INIT` arriving on an established serial link is not a protocol error; it is the
peer announcing it restarted. The link should tear down its session state and
complete the new handshake, which is what `_Z_FLAG_SERIAL_RESET` appears to
exist for.

That is a change in `zenoh-link-serial` (the Rust router side), so it belongs
upstream in eclipse-zenoh rather than in nano-ros. Worth confirming first
whether zenoh-pico's own serial link has the same asymmetry when the ROUTER
restarts.

Interim, and worth writing into any bring-up runbook: **restart the router after
any board reset.**


## Resolved — the flood was the bug, not the router's error handling

The board was sending **ten rapid INITs per reopen attempt**, and `_z_reopen`
retries every second, so a failed link produced a storm — 840 INIT frames on the
wire in one measured run.

`zenoh-link-serial` treats an INIT on an established link as a protocol error
and tears the link down. That part is *recoverable*: the teardown is exactly
what should let the next handshake succeed. What prevented it was that the nine
INITs following the first arrived **during the teardown-and-re-listen window**,
each erroring again and re-wedging the link. Self-sustaining, which is why this
issue originally recorded "only restarting the router recovers".

The retry came from `b0afc537`, added for the **cold start** — an MCU's first
frame goes out microseconds after reset into a disturbed line, and a single lost
INIT left the link dead. That justification is about the first connect only. Its
comment also claimed "a peer that is already initialised answers a second INIT
with RESET", which is what the protocol says and **not** what this router does.

Fix: full retries on the first connect, **one attempt** afterwards, and let
`_z_reopen`'s own one-second backoff pace the rest.

### Measured, direct link, mr_canhubk3/s32k344

| | before | after |
| --- | ---: | ---: |
| INIT frames in 75 s of failed reopens | ~840 | **31** |
| board reset under a live router | wedged forever | **recovers, 3 for 3** |
| router `Unexpected Init` errors | endless | **one per reset, then recovery** |
| self-reconnect after the router returns | never | **works, no board reset** |
| cold start still connects | yes | **yes** (graph resolves) |

Goal completion is unchanged within noise — 6/10 and 8/10 across two runs,
against 9/10 for the allocator fix alone. This change is about the link
recovering at all, not about throughput.

### The upstream half is no longer required

`zenoh-link-serial` accepting a mid-session INIT as "the peer restarted" would
still be the tidier protocol behaviour, and is worth reporting. It is no longer
needed for this board to recover.
