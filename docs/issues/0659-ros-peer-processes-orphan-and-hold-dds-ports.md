---
id: 659
title: "`PR_SET_PDEATHSIG` covers one level, so a SIGKILLed test leaks its ROS peer's
  grandchildren — they reparent to init and hold DDS ports for days"
status: open
type: bug
area: testing
related: [0573]
---

## Symptom

Four cyclone tests fail together, and not on anything they did:

```
native_example_pubsub::case_4_rust_cyclone
native_example_pubsub::case_5_c_cyclone
native_example_reqresp::case_06_cpp_cyclone_service
native_example_reqresp::case_15_cpp_cyclone_action

listener: ddsi_udp_create_conn: failed to bind to ANY:8650: address in use
[ERROR] nros: RMW session open failed — Backend("rmw_ret error")
```

8650/8651 are domain 5's discovery ports. Something else already had them.

## What had them

Forty `/opt/ros/humble/lib/demo_nodes_cpp/add_two_ints_server` processes, all
with **PPID 1**, ages from minutes to `1-23:05:47` — nearly two days:

```
   PID    PPID    ELAPSED  COMMAND
2361567      1   07:29:18  /opt/ros/humble/lib/demo_nodes_cpp/add_two_ints_server
3128195      1 1-23:05:47  /opt/ros/humble/lib/demo_nodes_cpp/add_two_ints_server
```

PPID 1 is the tell: they were reparented to init, so whatever started them is
long gone and never reaped them.

## Mechanism — the guard covers one level, and says it covers more

Peers are spawned through `Ros2Process` / `Ros2DdsProcess::spawn_bash`, which
does the right things for orderly teardown:

* `set_new_process_group()` at spawn — `setpgid(0,0)` plus
  `PR_SET_PDEATHSIG(SIGKILL)`;
* `Drop` → `kill()` → `kill_process_group()`, which reaches the whole group.

So a test that finishes normally cleans up. The leak is the other path.

The spawned thing is a CHAIN — `bash -c "… && timeout N ros2 run demo_nodes_cpp
…"` — so the process tree is `bash → timeout → ros2 (python) → the C++ node`.
`PR_SET_PDEATHSIG` is set on **bash only**; it is not inherited as a property
that protects descendants. When nextest SIGKILLs the test binary (a per-test
timeout, a killed run, an interrupted sweep) there is no `Drop`, so nothing
issues the group kill. bash dies from its PDEATHSIG; `timeout`, `ros2` and the
node do not, and reparent to init.

`process.rs` states the opposite:

> its descendants (e.g., bash → timeout → ros2).
>
> On Linux, also sets `PR_SET_PDEATHSIG(SIGKILL)` so the child is killed
> when the parent dies — prevents orphans when nextest SIGKILL's the test binary.

The first line names the exact chain that is NOT covered, and the second
promises the exact case that fails. The `setpgid` half is what makes the group
killable; the PDEATHSIG half is what the comment credits, and it stops one
level down.

## Why it bites later, not at the time

An orphan is silent. It holds a domain's discovery ports and does nothing
visible until some future run picks the same domain, and then the failure lands
on an innocent test as `address in use` — a message about the victim, not the
cause. Two days of accumulation on this host produced four such failures in one
sweep, none of them reproducible in isolation.

This is issue 0573's shape one peer over: that one gave `zenohd` a process
group and a `graceful_kill_process_group`, because a router that outlives its
test poisons the next one. The ROS peers got the same spawn-side treatment and
the same teardown-side `Drop`, and both are defeated by the same thing — the
test binary dying without running `Drop`.

## Direction

Not prescribing the shape; the ones worth weighing:

1. **Shorten the chain so PDEATHSIG lands on something that matters.** `bash -c
   "env && exec timeout N ros2 run …"` replaces bash with `timeout`, so the
   PDEATHSIG-carrying process IS the supervisor, and `timeout` kills its own
   child on death. Cheap; does not help when the chain legitimately needs two
   commands.
2. **Make the group self-reaping.** A wrapper that traps and kills its own
   process group on exit (`trap 'kill 0' EXIT INT TERM`) turns bash's death
   into a group kill rather than a single exit.
3. **Sweep leftovers at lane start**, keyed on the recorded pgid rather than on
   a process NAME (see the warning below), so a sweep that begins after a
   killed run does not inherit its debris.

Whichever lands, the acceptance test is not "no orphans after a clean run" —
that already passes. It is: SIGKILL the test binary mid-test, then assert no
descendant survives.

## Scope, and a warning about the obvious remedy

**This issue is about the `demo_nodes_cpp` peers only.** While diagnosing it I
killed processes matching `demo_nodes_cpp|component_node` on the strength of
"they look like abandoned test spawns". That pattern also matched 86
`component_node` processes belonging to a LIVE `play_launch` container tree
(`~/.local/lib/python3.10/site-packages/play_launch/…`, running Autoware
components, parent `component_container` still alive), and roughly 26 of them
were killed before anyone checked parentage.

So: any cleanup added for this issue must key on something that proves the
process is ours and dead-parented — a recorded pgid, or at minimum `PPID == 1`
plus a path under the ROS install — never on a name pattern. The name matched
someone else's running work on the first try.

## Provenance

Found 2026-08-17 while getting tier 1 green after issue 0639. The four cyclone
failures were the visible edge; the orphans had been accumulating for days
across sessions.


## Measured 2026-08-17 — options (1) and (2) do not work, and cannot

Both in-shell shapes were implemented and measured against the acceptance test
this issue specifies: SIGKILL the supervisor, then count descendants.

The probe mirrors the real chain — `bash -> timeout -> <node>`, own process
group, PDEATHSIG on bash only — and SIGKILLs bash, which is exactly what the
kernel does to it when the test binary dies.

| variant | survived the SIGKILL |
| --- | --- |
| `timeout N sleep …` (today) | 1 |
| `trap 'kill 0' EXIT INT TERM HUP; timeout N sleep …` — option (2) | 2 |
| `exec timeout N sleep …` — option (1) | 1 |

**Neither helps, for a reason that rules out the whole family:**

* **Option (2) cannot fire.** `PR_SET_PDEATHSIG` is set to SIGKILL, and SIGKILL
  is uncatchable — bash never runs an EXIT trap because it is not permitted to
  run anything. The trap only protects the paths that already worked (orderly
  exit, SIGTERM), which is the case `Drop` already covers.
* **Option (1) moves the problem one process along.** `exec` makes `timeout` the
  PDEATHSIG carrier, but `timeout` SIGKILLed is just as unable to reap `sleep`
  as bash was. It removes a process from the chain without changing who is
  responsible when the chain's head is killed.

The general statement: **no mechanism running inside the killed process can
survive its own SIGKILL**, and `PR_SET_PDEATHSIG` is a property of one process,
never of a subtree. Any fix has to live OUTSIDE the tree being killed.

That leaves option (3) — sweeping leftovers keyed on a recorded pgid — as the
only sound direction of the three, with the corollary that the recording must
happen at spawn time (the pgid must be durable, because by the time anyone
sweeps, the process that knew it is gone). A `cgroup`-based confinement would
also qualify, for the same reason: it is enforced by something the SIGKILL does
not reach.

Implemented, measured, and REVERTED rather than left in place looking like a
fix: a trap that cannot run is worse than no trap, because the next reader sees
teardown code and stops looking.

### Harness note, since the first two runs lied

An earlier version of the probe passed a unique token as an extra argv word to
`sleep`, which makes `sleep` exit immediately with "invalid time interval" — so
every variant reported zero survivors and all three looked like fixes. The probe
now uses a unique DURATION as its marker and refuses to report a result unless
it can confirm the chain actually started.
