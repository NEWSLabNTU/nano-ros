---
id: 737
title: "Both `freertos-posix` cells publish and deliver nothing — and no recipe
  built their fixtures, so the lane reported green on binaries that did not exist"
status: open
type: bug
area: testing, platform, rmw
related: [phase-370, issue-0405]
---

## Two defects, and the first hid the second

**1. The rows had no producer.** `examples/fixtures.toml` gained
`workspace-c-freertos-posix` and `workspace-cpp-freertos-posix` with
`platform = "freertos-posix"`, but `just/freertos.just` only ever calls
`workspace-fixtures-build.sh freertos …`. The manifest builder selects rows by
platform, so `freertos-posix` matched nothing: `just freertos build-fixtures`
finished green having built neither. This is issue #405's exact shape, and the
gate that names it (`matrix_fixture_coverage::
every_fixture_token_is_producible_by_the_module_that_owns_it`) was RED on main
saying so.

Fixed here — `just freertos build-fixtures-posix`, a sibling of
`build-examples` rather than a line inside it, because `build-examples` IS the
ARM QEMU lane (hard-fails without lwIP, cross-compiles every leaf) and this
board is host cc + host sockets with no emulator. `workspace-fixtures-build.sh`'s
lane-env guard now reads `freertos|freertos-posix)` so a direct call still fails
loud rather than deep-panicking.

**2. With the fixtures actually built, both cells fail.** The talker publishes;
nothing is received:

```
thread 'freertos_posix_c_entry_delivers_over_cyclonedds' panicked at
packages/testing/nros-tests/tests/freertos_posix.rs:79:9:
freertos-posix-c: no `Received:` line — nothing was delivered.
Waiting for messages
[talker_pkg] sent: 0
[talker_pkg] sent: 1
... through sent: 7
```

The C++ cell fails identically (`Published: 0..7`, no `Received:`). Both are
in-process entries, so this is a participant that publishes into a graph its own
subscriber is not in — the discovery/domain half, not the transport.

## Not caused by the change that found it

Found while landing phase-359 W10's clock ruling, which is not responsible:
stash the `packages/core` + `packages/api` edits, `just setup-cli`, rebuild the
two fixtures, run — **2 failed** on upstream `main` too.

## Why this contradicts the phase doc

`docs/roadmap/phase-370-freertos-posix-board-cyclone.md` says W3 LANDED and
that both cells "build on a plain Linux host and deliver `/chatter` end to
end". They build. They do not deliver. The likely explanation is that the
bring-up run was a hand-invoked build in a shell whose env differed from the
lane's (a domain id, a `CYCLONEDDS_URI`, or an interface selection) — worth
recovering, because whatever that env was is what the cell needs and the lane
does not supply.

The acceptance to add once fixed: the cell must pass through
`just freertos build-fixtures` + `cargo nextest run -p nros-tests --test
freertos_posix` and nothing else, so the next reader cannot repeat the
hand-build.

## Reproduce

```
just freertos build-fixtures-posix
cargo nextest run -p nros-tests --test freertos_posix
```

## Second host, 2026-08-21 — defect 2 does NOT reproduce; defect 1 is confirmed mine

**Defect 1 is exactly right and the diagnosis of how it happened is right too.**
The rows went in with no recipe, and phase-370's acceptance was recorded from a
hand-invoked `workspace-fixtures-build.sh freertos-posix c`. That is the
"hand-build in a shell whose env differed" this issue suspected — it was not an
env difference, it was a build nobody else could run. The gate that names the
class was red and I did not see it, because
`matrix_fixture_coverage` is a nextest target and I had been running `just
check`, which does not include it. A green `just check` said nothing about it.

**Defect 2 does not reproduce here**, through the recipe this issue added:

```
just setup-cli
rm -rf examples/workspaces/{c,cpp}/build-workspace-fixtures-freertos-posix
just freertos build-fixtures-posix                        # RC=0
cargo nextest run -p nros-tests --test freertos_posix --run-ignored all
  2 tests run: 2 passed, 0 skipped
```

Four arms, all delivering:

| arm | result |
| --- | --- |
| fresh fixtures via `build-fixtures-posix`, then the test | 2/2 pass |
| same, with the #740 cmake fix REVERTED | 2/2 pass |
| same, under 64 spinners on 48 cores (load ~68) | 2/2 pass |
| the binary bare: `env -u ROS_DOMAIN_ID -u CYCLONEDDS_URI ./freertos_posix_entry` | `Received: 0,1,2…` |

The second arm was a specific hypothesis worth killing: #740 (the config-header
mirror invisible to Makefiles) makes a TU compile against the in-tree STUB
header, and issue 0268 records that exact stale-sizes path producing "memory
corruption that surfaces as unrelated runtime failures (freertos C:
register_subscription -1)". It would have explained a publish-but-never-receive
perfectly. It is not the cause — reverted, still passes.

Host: `ROS_DOMAIN_ID`, `CYCLONEDDS_URI`, `RMW_IMPLEMENTATION` all unset;
`libddsc.so.0` from `/opt/ros/humble`; loopback plus three physical interfaces.

## What changed rather than an argument from "works for me"

The tests now print the inputs that could differ between hosts when they fail —
the domain asked for, the ambient `ROS_DOMAIN_ID` / `CYCLONEDDS_URI` /
`RMW_IMPLEMENTATION`, and the reminder that both nodes share ONE participant, so
a total absence of `Received:` is a participant/domain question and not a
transport one. An unreproducible red costs a second investigation unless the
first leaves its evidence behind.

**The `#[ignore]`s are LEFT IN PLACE.** Two passing runs on one host is not
grounds for lifting a park that someone set from a real failure on another, and
"works for me" is the weakest reason to restore coverage. What is needed is the
failing host's output with the diagnostics above.

## Parked, not left red

Both tests carry `#[ignore = "issue 0737 …"]`. That is not the usual "a red is
better than a skip" trade being dodged: these cells were never green in a lane —
their fixtures had no producer, so they SKIPPED, and turning that into a hard
tier-1 red for everyone would be charging the whole repo for a defect the fix
above merely made visible. Un-ignore with the delivery fix, in the same commit.

## Third data point, 2026-08-21 — it DOES reproduce, and DDS is not the layer

Same host as the original report, after the other host could not reproduce it.
Ran that host's exact sequence — `just setup-cli`, `rm -rf` both
`build-workspace-fixtures-freertos-posix` dirs, `just freertos
build-fixtures-posix` (RC=0), `cargo nextest run --run-ignored all` — and both
cells still fail, 2/2. So it is not build state, and it is not the `#740` cmake
mirror either (that hypothesis was already killed on the other host).

**Ruled out here, each measured:**

* `ROS_LOCALHOST_ONLY=0` is the one relevant variable this host sets and the
  other does not. Running the binary bare with it unset changes nothing.
* Network interface selection. Cyclone auto-picks `enp7s0` here ("selected
  arbitrarily from: enp7s0, br-babe48341e69, docker0, tap0"), which is a
  plausible-sounding culprit and is not one: forcing
  `<NetworkInterface name="lo" multicast="true"/>` also delivers nothing.
* Cyclone itself reporting a problem — `Verbosity=warning` prints nothing at
  all beyond the app's own output.
* The library. `ldd` resolves `libddsc.so.0` to
  `/opt/ros/humble/lib/x86_64-linux-gnu/libddsc.so.0`, same as the other host.

**And DDS is doing its job.** With `<Category>discovery,trace</Category>` the
writer and reader are created in one participant and MATCH LOCALLY, and the
samples are written:

```
new_writer(guid 1100c32:ff222e09:fb3499db:203, (default).rt/chatter/std_msgs::msg::dds_::Int32_)
new_reader(guid 1100c32:ff222e09:fb3499db:304, (default).rt/chatter/std_msgs::msg::dds_::Int32_)
match_reader_with_writers(rd …:304) scanning all wrs of topic rt/chatter
  reader_add_local_connection(wr …:203 rd …:304)
  writer_add_local_connection(wr …:203 rd …:304)
…
write_sample …:203 #1: ST0 rt/chatter/std_msgs::msg::dds_::Int32_:{0}
write_sample …:203 #2: ST0 rt/chatter/std_msgs::msg::dds_::Int32_:{1}
```

Same participant GUID prefix on both endpoints, matching QoS (reliable,
volatile, `data_representation=2(0,2)`), a local connection established before
the first write, and every sample delivered into the reader's history.

**So the sample reaches the reader and the application never takes it.** That
moves the question off the transport entirely — off domains, interfaces,
multicast, `CYCLONEDDS_URI` and the participant — and onto the listener node's
own scheduling inside the FreeRTOS POSIX simulator. The talker task keeps
running (`sent: 0..N` throughout); the listener prints `Waiting for messages`
once and never again. One task runs, its sibling does not.

That also explains why the other host cannot reproduce it: a task that never
gets scheduled and a task that does are the same binary and the same
configuration, and the difference is timing. It is the #0623/#0636 family —
what the boot path leaves runnable — not a delivery bug.

**Next step**, and it is cheap: print the listener task's spin count the way
`realtime_tiers` made the tiers report on themselves (`alive — N spin(s), M
timer(s) fired`). "Never scheduled" and "scheduled but taking nothing" are
different defects and nothing currently distinguishes them.

Keeping `#[ignore]`: it still fails here.

## Board-side correction, 2026-08-21 — this cell is SINGLE-tier, so it is not the 0623/0636 family

The trace above is decisive about the layer, and the conclusion it draws from it
is the one place to redirect: "It is the #0623/#0636 family — what the boot path
leaves runnable." That family is about the BOOT TIER outranking the tiers it
spawned. This entry has no tiers.

Measured on the artefact, not inferred:

```
$ grep -n 'run_tiers\|run_components' …/freertos_posix_entry_nros_main_generated.cpp
66:    return ::nros::board::FreertosBoard::run_components(
           NROS_ENTRY_LOCATOR, nros_boot_config_node_name(&NROS_BOOT_CONFIG),
           &__nros_entry_setup);

$ NROS_ENTRY_SPIN_MS=4000 ./freertos_posix_entry | grep -iE 'tier|spawn'
(nothing)
```

`run_components`, not `run_tiers`: ONE executor on ONE FreeRTOS task. The board's
`main` creates a single app task and starts the scheduler; nothing else is
spawned. So the talker and the listener are two CALLBACKS in one executor, not
two tasks — there is no second task to be starved, no boot tier, and no
priority relationship between them for the boot path to get wrong.

(The 0636 gap on FreeRTOS is real and still open — `freertos_run_tiers.c:397`
does take `&tiers[0]`, which on a bigger-is-more-urgent kernel is the most
urgent tier. It simply is not on this cell's path.)

That moves the question one layer further in than the trace already moved it:
the sample is in the reader's history, and the single executor servicing both
callbacks runs the timer (`sent: 0..N` keeps printing) and never drains the
subscription. So the next probe is inside `nros_cpp_spin_once` on this port —
whether the subscription's readiness path works when the FreeRTOS POSIX port's
signal-driven tick is what wakes the task — rather than anything about task
priorities.

Stated from the board side because that is what this host can contribute: it
cannot reproduce the failure, but it can say exactly what shape the image is.

## 2026-08-21, third host-side pass — the layer again, one step further in

The board-side correction above is right and my "#0623/#0636 family" reading was
wrong: `run_components`, one executor on one FreeRTOS task, two callbacks. There
is no second task to starve. Withdrawn.

**And my other claim was wrong too, in the more useful direction.** I wrote "the
sample reaches the reader and the application never takes it". It DOES take it.
With `<Category>rhc,trace</Category>`:

```
write_sample …:203 #1: ST0 rt/chatter/std_msgs::msg::dds_::Int32_:{0}
rhc_store d7d46a33…,d09b2498… si 0 has_data 1: new instance
update_conditions_locked(…) - inst 1 nonempty 1 … new 1 samples 1 read 0
take_w_qminv(…) - inst 1 nonempty 1 … new 1 samples 1+0 read 0+0
take: returning 1
take_w_qminv(…) - inst 1 nonempty 0 … samples 0+0
take: returning 0
```

Written, stored, condition updated, **taken**. Then polled empty every 10 ms
thereafter. Every DDS layer does its job.

**So the callback is the suspect, and it is never called.** Patching the
listener's `on_raw` to print unconditionally on entry and rebuilding through the
lane recipe: the probe string is in the binary (`strings | grep -c` = 2) and
never prints, while the talker's timer callback in the SAME executor prints
throughout. One executor dispatches its timer and not its subscription, on a
subscription whose `nros_cpp_subscription_register` returned 0 (the entry prints
`Waiting for messages`, which is gated on `rc == 0`).

That is the whole remaining window: between `dds_take` returning 1 and
`add_arena_subscription_c_callback`'s trampoline.

## Reasons this stayed inconsistent between hosts — and the fix for the class

The environment axis was never found, and the search was not cheap. Ruled out by
measurement on the failing host: OS and ROS (both hosts Ubuntu 22.04 / Humble),
`ros-humble-cyclonedds 0.10.5-2jammy`, the resolved `libddsc.so.0`,
`ROS_LOCALHOST_ONLY`, interface selection (forcing `lo` changes nothing), domain
occupancy (the neighbouring Autoware graph has since exited — zero sockets on
74xx — and it still fails), and build state (wiped and rebuilt through the lane
recipe).

**The reason it cost so much is not the environment. It is that the failure had
no voice.** The listener's callback was:

```c
if (std_msgs_msg_int32_deserialize(&msg, data, len) != 0) {
    return;
}
```

From outside the process, a silent `return` there and a message that never
arrived are the SAME observation: no `Received:` line. So every layer underneath
had to be cleared by hand — discovery, matching, QoS, the RHC, the domain, the
interface, the library, the build — before the callback itself could even become
a suspect. Two hosts each did a version of that. One line of output would have
skipped all of it, and would have made both hosts' runs directly comparable
instead of merely contradictory.

That is the avoidable part, so it is now gated:

* All four example listeners say what they dropped and how many bytes it was.
* The gate `check-no-silent-sample-drop` (fast line) found **five more** sites
  the same day — two C action servers, two C service servers and a C++ action
  server, all returning a reject with no output. Fixed together, per the
  fix-the-CLASS rule.
* Falsified: deleting the print from one site turns the gate red at that line.

The rule it enforces is not "handle the error" — dropping is often right. It is
*say so*. These are EXAMPLES, which makes it worse twice: users copy them, and
tests grep them. A silent arm in code a test reads for its verdict is the same
defect as a test reporting PASS on an unmet precondition, one level out.
