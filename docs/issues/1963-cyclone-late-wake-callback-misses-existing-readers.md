---
id: 1963
title: "Cyclone: a session wake callback installed after a reader exists never fires for that reader"
status: open
type: bug
severity: low
area: [rmw, cyclonedds]
found: 2026-10-19
related: [0889, phase-397]
---

## Measured

Found while writing `nros-rmw-cyclonedds/tests/entity_wake.cpp` (phase-397
W2.b). The first draft created two publishers and two subscriptions, and only
then called `set_wake_callback`.

The subscription with no entity callback then received its sample, but the
session wake never fired. The test failed with "an unregistered reader's
arrival did not reach the session wake" after 100 publishes over 5 s.

The only change in the passing version is the order: `set_wake_callback`
first, then the entities. It then reports `session 2`.

## Likely cause

`session_set_wake_callback` installs a data_available listener on the
PARTICIPANT (`ensure_listener`, `session.cpp`). The measurement suggests a
reader takes its parent's listener when the reader is CREATED, and that a
later `dds_set_listener` on the participant does not reach readers that
already exist. This is inferred from the two orderings, not read in Cyclone's
source.

## Why it is low severity today

Every in-tree caller installs the wake at session open, before any entity
exists:

- `Executor::open` calls `install_wake_signal_on_primary`.
- A tier joining a shared session joins a fan-out whose callback the session's
  first executor already installed.

A caller that installs late gets a poll-bound executor. It is not deaf:
`spin_once` still waits out its bound and takes the data. The cost is the
latency that issue 0889 removed.

## Fix direction

- Option 1: in `session_set_wake_callback`, walk the session's existing
  readers (the graph tracking already lists them) and re-apply the listener to
  each one that has no entity registration.
- Option 2: refuse a late install with a clear error.

Either way, add the late ordering as a case in `entity_wake.cpp`. The test
currently installs first, deliberately.
