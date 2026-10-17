# Phase 507 — XRCE and action clients recover from what they cannot see

**Status (2026-10-17). Opened to give four open issues a home; nothing landed
under this number yet.**

## Why this phase exists

Four open defects share one shape: a client sends something and then waits on
an answer that is never coming. Three are on XRCE, where the Agent sits between
the client and the graph and a lost message is invisible to the client:

- a goal sent blind and dropped by the Agent is never sent again (1930);
- a session open fails under load and passes 20 of 20 solo (1932);
- entity creation from two threads has no guard, on top of a client library
  that is not thread-safe (1734, issue 1711's class on zenoh).

The fourth (1756) is the native async action client: its goal is accepted and
the awaited result never resolves. That client is backend-neutral, so its fix
is in the same code an XRCE action client runs.

## Work items

- [ ] [#1930](../issues/1930-xrce-action-goal-sent-blind-is-lost.md): an XRCE
      action goal is confirmed or re-sent.
- [ ] [#1756](../issues/1756-native-async-action-result-never-resolves.md): the
      async action client resolves its result, or times out with an error.
- [ ] [#1932](../issues/1932-xrce-session-open-fails-under-full-sweep-load.md):
      an XRCE session open retries within its budget, and the failure under load
      is explained.
- [ ] [#1734](../issues/1734-xrce-concurrent-entity-creation-unguarded.md): the
      XRCE shim serialises entity creation per session, as the zenoh shim does
      since issue 1711.

## Acceptance

The XRCE action cell and the native async action cell pass 20 of 20 in a full
tier-1 sweep, and a test creates XRCE entities from two tiers sharing one
session.

## Issues homed here (2026-10-17)

| issue | why it belongs here |
| --- | --- |
| [#1734](../issues/1734-xrce-concurrent-entity-creation-unguarded.md) | The XRCE shim has no guard for entity creation from more than one thread, the check-then-act that issue 1711 fixed for zenoh. |
| [#1756](../issues/1756-native-async-action-result-never-resolves.md) | The native async action client gets its goal accepted and never resolves the awaited result. |
| [#1930](../issues/1930-xrce-action-goal-sent-blind-is-lost.md) | An XRCE action client sends its goal blind, and a goal the Agent drops is never sent again. |
| [#1932](../issues/1932-xrce-session-open-fails-under-full-sweep-load.md) | Two XRCE clients failed to open their Agent session under full-sweep load, and both pass 20 of 20 solo. |
