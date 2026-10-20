---
id: 1963
title: "Cyclone: a session wake callback installed after a reader exists never fires for that reader"
status: resolved
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

## Likely cause (as filed)

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

## Resolution (2026-10-21)

**Cause, read in Cyclone 0.10.5's source.** The guess above was right.
`dds_entity_init` merges each ancestor's listener into a new entity and marks
every merged field `inherited`. A later `dds_set_listener` on the participant
calls `pushdown_listener`, which applies `dds_override_inherited_listener` to
each child. That replaces a field only when its `inherited` bit is already
set. A reader created while the participant had no data_available handler
never got the bit, so the late listener never reached it. The implicit
subscriber between them has the same problem.

**Fix.** Neither option above. `session_create` now creates the participant
with its data_available listener already attached (`create_session_listener`
in `session.cpp`). Every reader therefore inherits it, whenever it is created.
The listener stays attached until `session_destroy`. Installing or clearing
the wake callback or the graph callback only publishes or nulls its pair, so
neither touches the listener any more, and `ensure_listener` and
`release_listener_if_idle` are gone.

When nothing is installed, an arrival costs one handler call that reads two
null pairs and returns. If the listener cannot be allocated, the session
still opens, and both installs return `NROS_RMW_RET_ERROR` instead of
attaching late.

`session_destroy` now nulls both pairs, deletes the participant (which waits
for any running callback), and then frees the listener. Before this, a graph
callback still installed at destroy kept the listener allocated.

**Test.** `tests/entity_wake.cpp` takes a `late` argument that installs the
session wake after every entity exists. It runs as a second ctest,
`nros_rmw_cyclonedds_entity_wake_late`, and checks the same three properties.
With the old `session.cpp` the late case fails with "2: an unregistered
reader's arrival did not reach the session wake". With the fix, both cases
and the whole ctest suite (37 tests) pass.
