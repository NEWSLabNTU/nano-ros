---
id: 1000
title: "Vendored Cyclone: a fired event is off the heap until its handler re-arms it, and `handle_xevk_spdp` has return paths that do neither — a latent leak, NOT the cause of #0997"
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

## MEASURED 2026-09-03 — these paths do not fire, so this is NOT #0997's cause

The section below asked for a cheap confirmation. It was done, with counters
rather than tracing (tracing this handler over semihosted stdout hangs the
island during discovery, so the instrument changed the outcome — see **#1004**).
Four `volatile unsigned` counters were added to `handle_xevk_spdp` and read over
the QEMU gdb stub:

```
enter        = 3      handler ran three times
unknown_guid = 0      <- first early return NEVER taken
no_writer    = 0      <- second early return NEVER taken
resched      = 2      periodic branch re-armed twice
xevents.roots= (nil)  event tree empty, as in #0997
```

**Both early returns are measured at zero.** So the mechanism this issue
describes is real in the code and did not happen: it is not what silences the
participant in
[#0997](0997-island-announces-spdp-once-then-lease-expires.md). The title and
the "Why this is not merely theoretical" section overstated it, and both are
corrected rather than deleted so the reasoning stays inspectable.

`enter = 3` against `resched = 2` is the remaining thread: one invocation left
without re-arming through a path that is neither of these two. The directed
branch legitimately does that — `delete_xevent` on its last use — so this may be
entirely ordinary, and counters splitting directed from periodic were written to
settle it.

**They have not been read, because the image stopped booting**
(**#1004**, `1004-an536-image-fails-to-boot-transport-error.md`, filed in PR #262 — not linked because it has not landed on `main` yet): `create_subscription`
returns `TransportError` in 3 of 3 runs at the current pin. That has to be fixed
before anything here can be confirmed or refuted further.

The counter read above is itself weak evidence and should be repeated: it
predates the gated runner, and that run never had a publisher match, so it did
not reproduce #0997's scenario. What it does establish is narrow and sufficient
for the correction above — in a run where the handler executed three times,
neither early exit was taken.

### What this issue is still worth

The invariant is the durable part, and it is untouched by the refutation:

> A fired event is off the heap until its handler re-arms or deletes it, and
> **nothing enforces that**.

`handle_xevents` extracts the event and stamps `tsched = DDS_NEVER` before
calling the handler, so any handler path that returns without rescheduling or
deleting leaks the event out of the queue while leaving it allocated — silently.
The two `handle_xevk_spdp` paths below do exactly that for a periodic event, and
remain worth fixing on their own merits as a latent leak, whether or not they
are ever hit in practice.

The assertion proposed at the end of this issue is worth more than the narrow
fix, and this episode is the argument for it: the reason those paths could be
suspected for a day is that nothing would have said if they HAD fired.

## What was NOT yet proven, and how it was closed (superseded above)

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
