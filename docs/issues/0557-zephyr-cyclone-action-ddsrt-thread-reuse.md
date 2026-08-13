---
id: 557
title: "Zephyr Cyclone action images fail at boot with `tid … is in use!` and rc=-100 — the readiness timeout hides an immediate failure"
status: open
type: bug
severity: high
area: rmw, zephyr, testing
related: [issue-0371, issue-0445, phase-350]
---

## Symptom

`zephyr::example_e2e::case_17_cyclonedds_c_action_e2e` and `case_18` (C++) fail
SOLO on a fully green `lane=all` fixture build — so this is not sweep
contention, which is how both were previously carried.

The verdict reads as a timeout:

```
[cyclonedds/c/Action] action-server didn't reach readiness
  (`Waiting for action goals`) within 60 s
```

The guest output says otherwise — it fails IMMEDIATELY and the 60 s is just the
harness waiting for a marker that will never arrive:

```
*** Booting Zephyr OS build v3.7.0 ***
<inf> cyclonedds: session_create: domain=29 entering
<inf> cyclonedds: cyclone: started application thread 3595786652
<err> os: tid 0x581fa0 is in use!      <- x6, consecutive tids
<inf> cyclonedds: session_create: calling dds_create_participant
<inf> cyclonedds: session_create: dds_create_participant returned 49379019
nros zephyr entry: run_components failed rc=-100
```

Issue 0445's shape at the harness level: a self-explaining terminal verdict
(`didn't reach readiness`) replaces the runtime result, and the real error is
four lines up.

## What the signals mean

`tid %p is in use!` is Zephyr's own `kernel/dynamic.c` — a dynamic thread stack
being reused while still live. Six of them, at consecutive tids, i.e. a pool of
threads, not one stray.

The cyclonedds submodule is pinned at

```
a09babf3 ddsrt: Zephyr-native sync backend — k_mutex/k_condvar instead of pooled pthreads
```

which is exactly the code that changed how ddsrt creates threads and
synchronisation primitives on Zephyr, and it landed today. Related: issue 0371
found the root cause of an earlier Zephyr Cyclone crash to be "the Zephyr
pthread mutex pool" — the same seam this commit rewrites.

`rc=-100` is the entry's own failure code from `run_components`.

## Not investigated further, deliberately

The suspect is a vendored FORK commit authored hours ago, in an active
migration. The last two times this session paused on that author's in-flight
work rather than patching it, they landed the fix themselves within the hour
(the RFC-0073 clock rename, then #548). Reporting beats a competing patch inside
their fork.

What a fix needs to establish first: whether the six `tid in use` errors are
fatal to participant creation or incidental, and whether `dds_create_participant`
returning `49379019` (a handle, not an error) means the failure is downstream of
it — the entry reports rc=-100 AFTER the participant is created.

## Reproduce

```
cargo nextest run -p nros-tests --test zephyr example_e2e::case_17_cyclonedds_c_action_e2e --no-capture
```

Both cases, C and C++, fail identically. `zephyr/rust` Cyclone action
(`case_16`) is worth checking as a control — if it passes, the fault is
language-path specific rather than in the backend.
