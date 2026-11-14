---
id: 2545
title: "No phase assigns callbacks to cores or splits an end-to-end deadline across heterogeneous images"
status: open
type: enhancement
severity: low
area: [executor, scheduling, smp]
related: [rfc-0108, phase-555]
found: 2026-11-14
---

## What

RFC-0108 keeps one ready set inside one core's scheduling domain. On SMP it
runs one executor core shared by per-core schedulers; across heterogeneous
processors it runs separate images joined by a transport. Two things it
leaves to "a separate, later phase that this design stays compatible with":

- assigning callbacks (and so their levels) to cores;
- deciding who splits an end-to-end deadline into per-image segment
  budgets: rlm, or each image's derivation from a stated segment budget
  (RFC-0108 "Deferred").

No phase exists for either.

## Why it is filed now

Phase-555 W3 normalized RFC-0108's status to Stable; deferred work goes to
an issue rather than staying only in the RFC.

## Acceptance

A phase doc owns load partitioning and names RFC-0108, or the project
records that one core per image is the supported shape and closes this.
