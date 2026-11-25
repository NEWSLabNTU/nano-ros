---
id: 2963
title: "The stack-floor twin measures only template images' main thread, so a thread role's peak is taken by hand"
status: open
type: limitation
severity: low
area: [tooling, zephyr, stack-floor]
related: [phase-557, issue-2846, issue-2400]
found: 2026-11-25
---

## What

`scripts/stack-floor-twin.py` (`just zephyr stack-floor-twin`) re-measures
an image's run-time stack peak on its Renode or QEMU twin and checks it
against the `[[observed]]` row. It can measure only one kind of row:

- **Template keys only.** `--measure` and the normal lane build a fresh
  `nros new` project (`<template>/<lang>/<rmw>@<board>`). A bringup key
  (`<bringup>@<image>`), which names an in-tree workspace image, is not
  measurable, although the drift check (`scripts/stack-floor-drift.py`)
  already builds those.
- **The main thread only.** The peak is read from the analyzer's `main` line
  (`main_peaks(console, thread="main")`), so only role `OPEN` is measured. A
  role on a thread of its own, such as `SNTP_RESYNC`
  (`nros_sntp_resync_tid`), is read by nobody.
- **No peers beyond the router.** An image whose thread talks to another
  service (an SNTP server, a DNS server) needs that service on the host too.

So issue 2846's `SNTP_RESYNC` row on `mps2-an385-zephyr` was measured by
hand, and its `method` records the command. Nothing re-measures it: if the
re-sync path's run-time depth grows, the drift check sees only a static
walk that is an upper bound (about 90 KiB through a type-bound edge), and
never the peak.

## Fix direction

- Accept a bringup key in the twin, built the way the drift check builds
  it (its `[[workspace_fixture]]` row).
- Map a role to the thread the analyzer names: `OPEN` → `main`,
  `SNTP_RESYNC` → `nros_sntp_resync_tid`. Keep the map next to the role's
  root in `cmake/NanoRosStackFloor.cmake` so the two cannot drift apart.
- Start the host services a row needs. For `SNTP_RESYNC`, that is an SNTP
  responder and a DNS responder: issue 2846's `method` has both.

## Acceptance

`just zephyr stack-floor-twin --only pubsub_bringup@zephyr_cortex_m_talker_clock_sntp_resync`
re-measures the `SNTP_RESYNC` row: two runs read the same peak, and that
peak matches the row.
