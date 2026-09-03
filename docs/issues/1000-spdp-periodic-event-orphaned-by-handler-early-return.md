---
id: 1000
title: "Vendored Cyclone: `handle_xevk_spdp`'s early returns orphan the PERIODIC spdp event — it leaves the heap, is never re-armed, and the participant goes silent forever"
status: open
area: [rmw, third-party]
severity: high
related: [0997]
---

# An event that fires is off the heap until its handler puts it back

**This is a defect in the vendored CycloneDDS fork**
(`third-party/dds/cyclonedds`, `NEWSLabNTU/cyclonedds` @ `d97a71e`), not in
nano-ros code, and it is stock upstream code rather than something the fork
introduced. It is filed here because this is the repository that consumes the
fork and the fork has issues disabled.

It is the mechanism behind [#0997](0997-island-announces-spdp-once-then-lease-expires.md),
where an an536 island stops announcing itself and every DDS peer expires its
lease and deletes it.

## The contract

`handle_xevents` (`src/core/ddsi/src/q_xevent.c:1219`) takes an event OFF the
heap before running it, and marks it unscheduled:

```c
while (earliest_in_xeventq(xevq).v <= tnow.v)
{
  struct xevent *xev = ddsrt_fibheap_extract_min (&evq_xevents_fhdef, &xevq->xevents);
  if (xev->tsched.v == TSCHED_DELETE)
    free_xevent (xevq, xev);
  else
  {
    /* event rescheduling functions look at xev->tsched to
       determine whether it is currently on the heap or not (i.e.,
       scheduled or not), so set to TSCHED_NEVER to indicate it
       currently isn't. */
    xev->tsched.v = DDS_NEVER;
    handle_timed_xevent (thrst, xev, xp, tnow);
  }
}
```

So between `extract_min` and the handler's own `resched_xevent_if_earlier`, the
event exists but is scheduled nowhere. **Re-arming is entirely the handler's
responsibility, and nothing checks that it happened.** A handler that returns
without rescheduling leaks the event out of the queue while leaving it
allocated — no assertion, no warning, no trace.

## The two paths that do exactly that

`handle_xevk_spdp` (`q_xevent.c:951`):

```c
if ((pp = entidx_lookup_participant_guid (gv->entity_index, &ev->u.spdp.pp_guid)) == NULL)
{
  GVTRACE ("handle_xevk_spdp "PGUIDFMT" - unknown guid\n", PGUID (ev->u.spdp.pp_guid));
  if (ev->u.spdp.directed)
    delete_xevent (ev);
  return;
}

if ((spdp_wr = ddsi_get_builtin_writer (pp, NN_ENTITYID_SPDP_BUILTIN_PARTICIPANT_WRITER)) == NULL)
{
  GVTRACE ("handle_xevk_spdp "PGUIDFMT" - spdp writer of participant not found\n", ...);
  if (ev->u.spdp.directed)
    delete_xevent (ev);
  return;
}
```

For a **directed** event (a one-shot reply to someone else's SPDP) this is
correct: `delete_xevent` disposes of it properly.

For the **periodic** event — the one that keeps the participant alive — both
paths simply `return`. The event has already been extracted and stamped
`DDS_NEVER`, so it is now:

* not on the heap,
* not marked for deletion,
* not freed,
* and referenced by nothing that will ever reschedule it.

Permanently orphaned, on the one event whose whole purpose is to repeat.

## Why this is not merely theoretical

[#0997](0997-island-announces-spdp-once-then-lease-expires.md) observed exactly
the end state this produces, read live over gdb on a stalled island:

```
xevents                    = {roots = 0x0}     <- timed-event tree EMPTY
non_timed_xmit_list_oldest = 0x0
non_timed_xmit_list_newest = 0x2170b228        <- tail with no head
terminate                  = 0
cond.tasks                 = {len = 10, cnt = 1}
```

The last SPDP handler logged `(resched 8s)`, and eight seconds later nothing
fired. `tev` then waited with `portMAX_DELAY` — correctly, because the queue was
empty — and the peer expired the lease 10 s after the final announcement.

## What is NOT yet proven, and how to close it cheaply

**That these early returns are the path actually taken.** The mechanism exists,
and it produces precisely the observed state, but no trace line confirms it fired
— because both early exits log through `GVTRACE`, which is the `trace` category, and
the island run used `discovery`. The `trace` firehose over semihosted stdout is
slow enough to perturb the timing of the thing being measured, which is why it
was not simply enabled.

Cheapest confirmation, in order:

1. Route those two `GVTRACE` calls to `GVLOGDISC` (as #0997's investigation
   already did for the "xmit spdp" line) and re-run with `<Category>discovery`.
   A single `handle_xevk_spdp … - unknown guid` line settles it.
2. If neither fires, the orphaning is happening elsewhere and the same audit
   should be run across every `handle_xevk_*`: the contract above says each one
   MUST reschedule or delete on every path, and that is a property nothing
   currently enforces.

## The fix, and the wider point

Narrowly: the periodic event must be disposed of rather than dropped — either
`delete_xevent (ev)` unconditionally on those paths, or reschedule it so a
participant that reappears resumes announcing.

Wider, and worth more: the invariant "a fired event is off the heap until its
handler re-arms or deletes it" is enforced by nothing. A debug assertion at the
bottom of `handle_timed_xevent` — that `tsched` is no longer `DDS_NEVER` unless
the event was deleted — would have caught this at the first occurrence instead
of presenting as a silent, permanent loss of discovery an hour into a run.

Upstream carries the same code, so this is worth sending to
`eclipse-cyclonedds/cyclonedds` rather than only carrying a fork patch. That
call is the maintainer's.

---

# CORRECTION 2026-09-04 — the fix proposed above is UNSAFE, and the contract has a third arm

A full audit of every timed-event handler was run against the vendored fork
(`d97a71e`). It confirms the orphaning *class* and refutes three specific claims
made above. The original text is left intact; this section supersedes it.

## 1. Do NOT apply the fix proposed above — it is a use-after-free

"either `delete_xevent (ev)` unconditionally on those paths" corrupts the heap.
`pp->spdp_xevent` is a raw **owning** pointer that is never cleared, and
`ddsi_unref_participant` deletes it again, unconditionally:

```c
/* src/core/ddsi/src/ddsi_participant.c:688 */
if (pp->spdp_xevent)
  delete_xevent (pp->spdp_xevent);
```

`delete_xevent`'s only protection against a second delete is a pair of
`assert`s (`q_xevent.c:341-342`) — and the embedded Cyclone builds
`CMAKE_BUILD_TYPE:STRING=Release`
(`packages/rmw/cyclonedds/nros-rmw-cyclonedds/build/CMakeCache.txt:58`), so
`NDEBUG` removes them. The second call writes `TSCHED_DELETE` into freed memory
and inserts it into the fibheap. That trades a silent stall for heap corruption
on an RTOS, which is a strictly worse failure.

## 2. The contract is not "reschedule or delete" — there is a third arm

Above it says re-arming is "entirely the handler's responsibility". It is not.
A handler may also **return and leave the event to the entity that OWNS it**,
and upstream relies on this in five places. Every timed event except the
directed SPDP one is owned by a live struct that holds the pointer and deletes
it during teardown: `wr->heartbeat_xevent`, `m->acknack_xevent`,
`pp->pmd_update_xevent`, `pp->spdp_xevent`, and the `XEVK_CALLBACK` events.

So a lookup failure inside a handler means *"the owner is tearing this entity
down and will delete the event"*, and returning is correct.

**This also re-reads `ev->u.spdp.directed`.** It is not a periodic-vs-one-shot
test, as claimed above; it is an **owned-vs-unowned** test. `XEVK_SPDP` is the
only kind used both ways — undirected is stored in `pp->spdp_xevent`
(`ddsi_participant.c:977`), directed has its `qxev_spdp` return value discarded
(`q_ddsi_discovery.c:633`) and therefore must delete itself. The asymmetry the
original text reads as a bug is the design.

Only ONE path in the file orphans an event whose owner is still operational:
`handle_xevk_spdp:968-973`, undirected, reachable when `pp->e.onlylocal` (that
participant gets a periodic SPDP event because `spdp_write` returns 0 for
onlylocal, `q_ddsi_discovery.c:533-536`, and `ddsi_participant.c:967` accepts
`>= 0`). `RTPS_PF_ONLY_LOCAL` is annotated "FIXME: not used, it seems"
(`ddsi_participant.h:165`), so nothing in-tree reaches it today.

## 3. This mechanism CANNOT produce the empty tree #0997 observed

The claim above — that it "produces precisely the observed state" — does not
hold. `handle_xevk_pmd_update` reschedules **unconditionally** at
`q_xevent.c:1097` whenever the participant is found and the interval is finite,
and it is finite: `pp->lease_duration` defaults to 10 s and
`ddsi_participant.c:1172` clamps to it. With only the SPDP event orphaned, the
PMD event is still on the heap and `tev` still wakes.

`xevents = {roots = 0x0}` therefore requires SPDP **and** PMD to be orphaned
together, which means the participant became un-findable in the entity index.
**That is a root cause upstream of these handlers**; the orphaning is the
amplifier that makes it permanent rather than the trigger. The "wider point"
above is right; the narrow causal claim is not.

## 4. The proposed assertion would fire on about half the queue

"Alive and unscheduled" is the designed resting state, not an error. An
`assert (xev->tsched.v != DDS_NEVER)` after the handler would fire on, at least:
every reliable writer's heartbeat in steady state (`q_xevent.c:748-752`, and the
event is *created* at `DDSRT_MTIME_NEVER`, `ddsi_endpoint.c:901-906`);
`handle_xevk_pmd_update` with an infinite interval; `make_and_resched_acknack`
on `AANR_SUPPRESSED_ACK` and `AANR_ACK` (`ddsi_acknack.c:400`, `:417`);
`lifespan_rhc_node_exp` and `instance_deadline_missed_cb` whenever their heaps
drain; and all five legitimate teardown handoffs.

A sound version tests OWNERSHIP, not scheduling: give `struct xevent` an
`owned` bit set by the `qxev_*` constructor and assert
`tsched.v != DDS_NEVER || owned`. That fires on exactly the unowned-and-
unscheduled shape, which is the real defect.

Note also that "a debug assertion would have caught this at the first
occurrence" is **false for the build that shipped**. `q_xevent.c:996-1019`
already contains an `#ifndef NDEBUG` block asserting precisely that an
undirected SPDP event must not hit an empty WHC unless marked for deletion —
and Release compiled it out.

## 5. Unverifiable here

"stock upstream code rather than something the fork introduced" is consistent
with `docs/reference/cyclonedds-fork-delta.md` carrying no `q_xevent.c` row, but
it cannot be checked in this checkout: the submodule is a shallow clone, so
`git log` attributes the whole file to the shallow boundary. Per CLAUDE.md, run
`git remote prune origin && git fetch --unshallow origin` before believing any
history claim about the fork.

## 6. The corrected fix, and what to measure first

The narrow patch is a **reschedule**, not a delete, on the one live-owner path,
plus clearing `pp->spdp_xevent` / `pp->pmd_update_xevent` before deleting them
so the double-delete class is closed structurally. Both are written out in full
in the phase notes; neither is applied yet, because the ACTUAL root cause is
§3's un-findable participant and a fix for an unreached path is not worth a
vendored-fork bump on its own.

Measure first — and this needs a THIRD log line the original list omits:
route `q_xevent.c:962` and `:970` from `GVTRACE` to `GVLOGDISC`, **and add one
to `handle_xevk_pmd_update:1073-1075`, which today returns with no logging at
all.** Then re-run the an536 repro with `<Category>discovery</Category>`:

* SPDP *and* PMD "unknown guid" within ~8 s of each other ⇒ the participant left
  the entity index; hunt the `entidx_remove_participant_guid` caller. This is
  the outcome §3 predicts.
* only "spdp writer of participant not found" ⇒ the `:973` path is real on this
  target and the corrected patch fixes it.
* neither line, and no further `xmit spdp` ⇒ the loss is at INSERT time, not in
  a handler. Instrument `resched_xevent_if_earlier`'s return at `:1062` and look
  at `ddsrt/src/fibheap.c` — the submodule tip `d97a71e` is itself
  "ddsrt: one funnel heap for every port", which is worth suspicion on timing
  alone.
