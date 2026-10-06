# Phase 474 -- what the safety island found on the S32K344: violations nobody can see, monitors armed too early

**Status (2026-10-06). IN PROGRESS.** D5 had already landed before this
phase was written (issue 1567, see D5). D1, D2, I1 and I2 landed on branch
`feat/violation-ring-and-arming` (issues 1714, 1715); T4 landed as a host
executor test, its native_sim / QEMU / board runs are open. Every other item is
open. Records the open work the Autoware safety island's RTSS@Work 2026 demo
(simple-autoware-safety-island, phase 8) found in nano-ros while running its
three acts on an NXP S32K344 (MR-CANHUBK344) behind a 921,600-baud UART and an
island gateway router. Nothing below was measured for this document; every
number is quoted from the island's documents named under **Source**.

**Prior:** RFC-0052 W3b (the contract monitors and `drain_violations`), issue
0514 (violations were never drained; resolved by logging at detection),
phase-462 W2 (`silence-runtime`), phase-473 (graph discovery off, latched
reads), issues 1533, 1534, 1567, 1574.

**Source:** in simple-autoware-safety-island at main `73e6b6d`:
`docs/takeover-trace.md` section 10 ("Follow-ups" F1-F5) and section 11
(including "Runtime contract violations"); `docs/roadmap/phase-8-rtss-work-demo.md`,
the "Open:" notes of W1, W2, W8, W8a, W14, W17, W24, W26, W30 and W31;
`docs/boot-through.md` (phase8-W1, W14, W15, W26); `docs/serial-link.md`
section 11 (phase8-W10); `docs/tracing.md` (phase8-W17).

---

## Why

The island ran 13 board acts in phase8-W8 and 9 fresh ones in phase8-W31, all
PASS, on an image whose generated entry bakes `max-latency-runtime` monitors
at 206 ms on `mrm_state` and `takeover_request_state`. After those runs the
island could not say whether any runtime monitor had fired during an act:

- the console the monitor writes to (`log_violation`, `log_warn`) is lpuart0,
  which is not wired on the board; lpuart2 carries the zenoh link;
- the executor's violation ring, read over SWD at bring-up (`w31-bringup`),
  was already full: 8 of 8 slots, all start-up entries, so a later violation
  is not stored;
- no trace marker records a violation.

The island's only evidence is offline: the trace's ENTRY/EXIT brackets show
the longest handler callback that published a monitored topic was 6.86-24.93
ms per run, far below 206. That is a measurement the island had to make
around nano-ros, not one nano-ros reported. A contract monitor whose verdict
cannot leave the board is not yet a safety measurement.

The same runs exposed what the monitors measure (one callback, not the
route the contract charges), monitors that judge start-up as if it were steady
state, and sizing figures (heap, frame_id bounds) that the derivation and the
board disagree on.

## Design

### D1 -- violation reporting on a board without a console

What the tree does today (`packages/core/nros-node/src/executor/`):
`run_contract_monitors` (spin.rs) calls `log_violation` (monitor.rs, a
`log_warn` of "contract violation: <rule> <fqn> measured=... declared=...")
when `report_violations` is set, then pushes the violation into a
`heapless::Vec` of `MAX_VIOLATIONS` = 8. When the Vec is full the push fails
and `monitor_violations_dropped` is incremented, so the ring keeps the FIRST
8 violations since boot. `Executor::drain_violations` empties it, and
`nros_cpp_executor_drain_violations` mirrors it over the C++ FFI; the only
caller of either outside unit tests is the test binary
`packages/testing/nros-tests/bins/contract-monitor-cpp`. The generated Zephyr
entry does not drain it. `violations_dropped()` exists and nothing reports it.

On the island board the result was a ring full of start-up entries and a
console nobody reads. Options to decide between (more than one may land):

1. **SWD drain** -- a fixed-layout, versioned record (like the boot report)
   holding the ring, the dropped count and a sequence number, readable by name
   over a debugger. Cheapest; needs no link; only useful on a bench.
2. **A trace marker per violation** -- rule id, endpoint index, measured and
   declared, written into the image's trace stream when tracing is compiled
   in. Puts the violation on the same clock as the callbacks it judges.
3. **A violations topic** -- a publisher the entry declares (for instance a
   `diagnostic_msgs/DiagnosticArray`, or a compact message), fed by the
   drain. Reaches the host over whatever link the image has; costs an entity
   and link bandwidth, and must not itself be able to starve the link.

Ring policy, whichever channel is chosen: keep the LATEST N, not the first N,
and keep the dropped counter beside it, so a reader sees the most recent
violations and knows how many it missed. Decide whether the entry glue drains
the ring every spin (and into which channel) or whether the ring is the
channel.

**Decided (2026-10-06, I1).** Options 1 and 2 both land; option 3 is
already issue 1635's `/diagnostics` sink and is unchanged. The ring keeps the
latest N with a dropped and a total count. Every stored verdict goes, at
detection, to the SWD record `NROS_VIOLATION_RECORD` (option 1; versioned,
all-`u32`, present with `CONFIG_NROS_BOOT_REPORT`, never drained) and to four
trace markers (option 2; with `CONFIG_NROS_TRACE_CALLBACKS`). The ring stays
for a drain; an opt-in drain-and-report hook
(`CONFIG_NROS_VIOLATION_DRAIN_REPORT`) drains it every spin into the log. The
hook is a build-time executor default rather than template code, for the
reason issue 1635 gave for its sink: the runtime owns every board's spin loop,
so one default reaches the C, C++ and Rust entries.

### D2 -- arm the monitors only after start-up

The 8 entries the W31 bring-up found, all before any act:

| count | rule | endpoint | detail |
| --- | --- | --- | --- |
| 4 | `timer-overrun-runtime` | `timer` | |
| 1 | `release-jitter-runtime` | `spin` | measured=57751 declared=10000 |
| 1 | `silence-runtime` | `/mrm_handler/operation_mode_availability` | declared=500 |
| 2 | `rate-hierarchy-runtime` | the comfortable-stop operator's `clear_velocity_limit` and `max_velocity_candidates` | on-demand topics with a 10 Hz `min_rate_hz` |

None of these is a fault of the running system: registration, the first
join and inputs that have not started yet all look like overruns, jitter and
silence. (The two rate entries are also a contract question -- an on-demand
topic has no rate to keep -- which is the play_launch phase's on-demand-topic
key; see Cross-repo.)

Proposal: a monitor is armed only once the application says its start-up is
over. The island already has the event: its `mrm_handler` runs an INIT/RUN
state machine (phase8-W27), and INIT ends only when every input is
established, not merely heard (phase8-W28). Options:

1. an API the application calls (`arm_monitors()`, or a per-node "running"
   flag) when it reaches its own RUN;
2. a grace period stated in the contract or derived from it (for example the
   largest `max_age` / silence bound after FirstSpin);
3. both: arm at the API call, and arm anyway at the grace deadline so an
   application that never calls it is still monitored.

Violations before arming are counted separately, not discarded, so start-up
trouble stays visible.

**Decided (2026-10-06, I2).** Option 3, from Kconfig and not the contract
(rlm has no key, and inventing one is the play_launch phase's business): the
default stays "armed at the first spin"; `CONFIG_NROS_MONITOR_ARM_ON_CALL`
waits for the application's call, and `CONFIG_NROS_MONITOR_ARM_GRACE_MS` arms
anyway that long after the first spin (0 = no deadline). Arming is per
executor, triggered image-wide by the application's call. Pre-arm verdicts
are counted in `suppressed_before_arm` (executor and SWD record).

### D3 -- the route, not the callback

`max-latency-runtime` measures one dispatch's elapsed time and charges it to
each monitored publisher whose publish count advanced in that dispatch
(`attribute_latency` in `executor/spin.rs`). On the island that bounds the
handler's work inside one callback (longest 24.93 ms in the W31 runs, r03's
EMERGENCY_STOP tick). The contract's 206 ms `call_mrm` is a route: the serial
link (57 ms stated, 67.02 seen), the wait for the handler's 100 ms tick (118
stated), and the work in the tick (31 stated). The runtime monitor therefore
cannot fail on the case the contract budgets: a 206 ms route made of a slow
link and a full tick wait passes a callback-level check with 180 ms to spare.

Decide what a route-level runtime check is in nano-ros: which stamp starts it
(the sample's source stamp, the take, or the trigger edge), where the clock
domains differ (the link term is cross-clock; the island stamps it against
the host offline), and which monitor reports it. This pairs with the
play_launch phase that works on the same findings (F1, transport charged on
the reaction walk): the static walk and the runtime check should charge the
same terms.

### D4 -- a deadline is not a monitor (F3, the nano-ros half)

A node-path `max_latency` becomes BOTH the node's derived deadline (the
realizer's `max_latency_ms`, `nros-orchestration-ir` `rtos_realizer.rs`) and
a `max-latency-runtime` monitor row in the generated entry. So a budget
cannot be stated where a cost happens without changing scheduling: the
comfortable-stop operator serves `operate` after the handler's tick and
publishes the velocity limit 0.85-1.18 ms after the handler returns
(takeover-trace section 10), but a `max_latency` on that path would become a
2 ms runtime monitor and a 2 ms deadline in the image. The island states no
number there and folds the cost into `call_mrm`'s work term.

Decide how a contract says "this is a budget to check" separately from "this
is a deadline to schedule by" -- a separate key, or a rule for which
`max_latency` the realizer may take -- so the two can be stated at different
places. The rlm/play_launch half of F3 (a service edge has no transport or
queueing key) is the play_launch phase's.

### D5 -- skip `externals` publishers when deriving the image (DONE)

phase8-W8a found nano-ros composing the contract's external
`/availability_gate/availability` publisher into the image's entity
inventory: the transient-local count was refused, queryables derived to 2,
and the comfortable-stop operator failed `create_publisher_in (code=-3)` on
QEMU. The island worked around it with `CONFIG_NROS_MAX_QUERYABLES=4`.

Resolved in nano-ros by issue 1567 (`da272e419`: an endpoint on an external
side of a topic whose node is not in `structure.nodes` is skipped), with its
"not fixed here" residue in issue 1572 (`00fbcb405`: a refused
transient-local count sizes for the worst case). The island moved its pin to
`da272e419` in phase8-W14 and retired the workaround (phase8-W13; the table
derives to 4). Kept here only so the island's open note has an answer; no
work remains.

## Implementation

### I1 -- the violation channel and the ring policy

Implement D1's decision: the ring keeps the latest `MAX_VIOLATIONS`, the
dropped counter is reported with it, and the generated Zephyr entry drains
the ring into the chosen channel (SWD record, trace marker, topic). Whatever
the channel, `violations_dropped()` stops being a number nothing reads.

**Result (2026-10-06).** Landed as decided under D1. The ring
(`monitor::ViolationChannel`, `executor/monitor.rs`) evicts its oldest entry
when full, numbers entries per executor and counts evictions. Knobs:
`NROS_EXECUTOR_MAX_VIOLATIONS` (default 8: 24 B a ring slot, 28 B a record
slot on a 32-bit target, so 192 B per executor plus 40 + 224 B for the record)
and `NROS_VIOLATION_DRAIN_REPORT` (default off). SWD layout, version 1, all
little-endian `u32`: header `magic "NRVR" (0x4e525652), version, capacity,
slot_words (7), total, head, dropped, suppressed_before_arm, armed, reserved`,
then `capacity` slots of `seq (1-based, 0 = empty, written last), rule code,
fqn FNV-1a, measured, declared, fqn_addr, fqn_len`; newest entry in slot
`(head + capacity - 1) % capacity`. Rule codes are `monitor::RULE_IDS` index +
1 (append only). Trace markers: 21 `seq << 8 | rule code`, 22 fqn hash, 23
measured, 24 declared. C++ FFI: `nros_cpp_executor_set_violation_drain_report`,
`nros_cpp_executor_violation_counts`.

For the island's `w31-logs/tools/vscan.py`: its heuristic still finds ring
entries (a `Violation` is unchanged: rule `&str`, fqn `&str`, measured,
declared, 24 B), but the ring is now a rolling window and can be drained, and
the "FULL: later violations are dropped" note no longer holds. Read
`NROS_VIOLATION_RECORD` by symbol instead
(`read-violation-record.py --addr-only`, then decode the dump): it needs no
scan. The island's marker ids 1-31 overlap nano-ros's 16-24 if it installs
`nros_set_trace_sink` into its own tracer; it must offset one block.

### I2 -- arming

Implement D2's decision in the executor (an armed flag per monitor table, or
per node) and in the generated entry (where the grace deadline is baked).
Pre-arm violations go to a separate counter.

**Result (2026-10-06).** Landed as decided under D2. API:
`nros_monitors_arm()` (C), `nros::arm_monitors()` / `nros_cpp_monitors_arm()`
(C++), `nros::monitor::request_monitor_arming()`, `Executor::arm_monitors`,
`set_monitor_arming`, `monitors_armed`, `violations_suppressed_before_arm`,
`nros_cpp_executor_set_monitor_arming`. Arming reopens every rate/age/silence
window, discards latency/age measured before it, clears the release-jitter
statistics, and re-reports a stack low-water mark already past its minimum
(stack depth reached at start-up is a real fault). The generated C/C++
entries' monitor-install comment states the knobs. For the island: set
`CONFIG_NROS_MONITOR_ARM_ON_CALL=y` (and a grace backstop) and call
`nros::arm_monitors()` where the handler logs `INIT_DONE`.

### I3 -- trace hooks the island's measurements need (F4)

The island's trace keeps no per-sample take marker for `kinematic_state`,
`operation_mode_state` and `control_cmd`, so the link hop of those inputs is
not measured; and the emergency operator's 30 Hz timer ticks are kept one in
ten, so its tick jitter is not measured (takeover-trace section 10, F4). The
island's marker table and sampling policy are generated by the island's own
tool from its contract (`src/safety_island_tracing/gen_markers.py`); nano-ros
owns the dispatch-level hooks (`executor/callback_trace.rs`, register /
start / end per dispatch, `trace-callbacks` feature). The nano-ros part:

- a per-sample take hook that carries the sample's identity and, where the
  message has one, its source stamp, so an image can trace takes of chosen
  subscriptions without hand-placed markers;
- a configurable sampling rate for timer ticks (every tick, 1 in N, or on
  change), per timer, so a 30 Hz tick can be traced in full when its jitter
  is the measurement.

### I4 -- a bounded `frame_id` for the host metadata probe

`nros sync` prints "no producer" for the source metadata of the island's
four components (phase8-W26, still as before; `docs/boot-through.md`
phase8-W1). Two causes, per the island: two probes build and then halt at
`declare_parameter (code=-16)` because capabilities are not lowered into the
probe (issue 0543, since resolved; the island saw it on an older pin), and
two fail to build on the host on an unbounded `header.frame_id` (Odometry,
VelocityReport). The image's tables come from the launch model, so the image
is not affected; the probe is. Give the probe a bound for `frame_id` (from
the contract or the type-bound sizing of phase-403) so it can build these
components, and re-run on the current pin to confirm the capability half is
gone. Related: issue 0939 (the probe links the node name, not a target).

### I5 -- the heap: one derivation the board and the report agree on

Three numbers, from three different images; they must not be compared as if
they were one:

| figure | image | where |
| --- | --- | --- |
| HEAP HEADROOM REFUSED, "set `CONFIG_NROS_ZEPHYR_HEAP_SIZE` >= 133952" (peak 109,376 at capacity 123,392, floor 24,576) | phase8-W1 QEMU image: 4 nodes, parameter store, heap 122,880 | boot-through.md phase8-W1 |
| FirstSpin peak 77,160 of capacity 102,912 (heap configured 102,400), headroom ok, 25,752 spare | phase8-W31 board image (3 nodes), joined to the container Autoware through the gateway | takeover-trace.md section 11 |
| `HEAP EXHAUSTED` in `_z_slice_init` from `zpico_read`, 7.3-8.0 s after boot when Autoware joined, at 102,400; the runs then used 1 MiB | phase8-W17 QEMU image over TCP, straight into the stock router and behind the gateway (runs w17-qb1, w17-qb2) | tracing.md phase8-W17 |

So the 133,952 ask is stale for the board image (phase8-W8a shrank the image;
its QEMU peaks were 74,856-76,080 and the heap went to 102,400). The live
problem is the third row: the headroom check judges the peak at FirstSpin,
and on QEMU the heap ran out after FirstSpin, in the read path, when a large
host graph started sending. The island's explanation is that over TCP nothing
paces Autoware's traffic the way the board's UART does.

Work: (a) reproduce the third row on the current pin with graph discovery off
(phase-473 W1), to separate a graph burst from sample traffic; (b) make the
heap the image derives cover the read path's worst case for the image's
subscriptions and link (or bound `zpico_read`'s allocation so it cannot grow
with the remote graph), so the configured heap and the derived one agree and
both hold after FirstSpin; (c) say in the boot report which figure the
headroom line judged (FirstSpin peak or running peak), and why capacity reads
102,912 for a configured 102,400.

## Test / check

### T1 -- the island dropped its zenoh session when a host peer joined a plain router

phase8-W8a recorded it on QEMU at any heap. phase8-W15 found the cause and it
is configuration, not version: zenoh-pico measures router silence against
min(router lease, own lease) = 10 s, while a stock `rmw_zenohd` (`lease:
60000`, `keep_alive: 2`) keepalives an idle link every 30 s; the CLOSE (reason
5, EXPIRED) is decided at about OPEN+20 s and reaches the wire with the next
byte the router sends, which is why a host peer's join looked like the
trigger. Fixed with a 60 s lease: the island states
`CONFIG_NROS_ZENOH_LEASE_MS=60000`, and nano-ros moved the Zephyr default to
60000 in issue 1574 (`58541dc14`, PR #1461).

What remains open:

- reproduce on current main with the island's explicit lease lines removed
  (the default alone), a stock router and host peers joining, and close
  W8a's note on that run;
- the `min()` itself is a zenoh-pico deviation from zenoh's lease semantics
  (a lease is how long the OTHER side may wait); the W15 handoff calls it an
  issue candidate, not filed. File it, or record why the 60 s default is the
  answer;
- under QEMU's `guestfwd` a reconnect never comes back (one host TCP
  connection for QEMU's lifetime); a harness artifact, not a board one, but
  any QEMU lane that tests reconnect must know it.

### T2 -- `island_trace_cost_max` read 0 in two runs

phase8-W8's open note. The counter is the island's, not nano-ros's: a DWT
CYCCNT bracket around each marker write in
`src/safety_island_tracing/include/island_trace.h`, read over SWD by name.
Listed here because the board's figure for the cost of a trace write is the
only one either repository has, and I3 adds writes. Find why a run that wrote
markers reads a maximum of 0 (bracket compiled out, the symbol resolved to a
different copy, or the readout racing the reset), and fix it in whichever
repository owns the cause.

### T3 -- joining while inputs already flow (issues 1533, 1534)

Status on origin/main (2026-10-01): **both issue files read `status: open`.**
They are tracked as files under `docs/issues/`, not as GitHub issues
(`gh issue view 1533` resolves to nothing).

- [Issue 1533](../issues/1533-zenoh-pico-read-task-stops-on-one-rejected-message.md)
  (the read task stopped for good on one rejected message): its fix is
  on main -- zenoh-pico `52f60b79`, pinned by PR #1389 (`10b2782be`), and
  carried forward in the current pin `e28ff603`. The file still says "prepared
  on the fork, not yet pinned"; its status is stale and should be resolved
  and archived after a check that the pin contains the change.
- [Issue 1534](../issues/1534-zephyr-tx-flush-task-outranks-the-read-task.md)
  (the tx-flush task outranked the read task): PR #1443 (`8d59adba7`) gave
  the flush task the read band and made Zephyr serial TX interrupt-driven.
  Still open per the file: nano-ros does not state the priority the MAIN
  thread registers at (`CONFIG_MAIN_THREAD_PRIORITY`, default 0, above every
  transport band), and the 1 KiB RX ring default was measured at its edge.

The island works around both by configuration (phase8-W10, `serial-link.md`
section 11): main registers at `CONFIG_MAIN_THREAD_PRIORITY=5` against the
read task's 4, and `CONFIG_NROS_ZENOH_SERIAL_RX_RING_BYTES=4096` (the issue
file says 2048; the island's board conf states 4096). Before W10 the
workaround was join order: the island joined before the inputs flowed
(phase8-W2). Close 1534 with a derived main priority, or a priority-plan
check that refuses main above the read band, and a measured RX-ring default.

### T4 -- an end-to-end violation test on the board

Once I1 and I2 land: build an image with a deliberate overrun (a handler that
busy-waits past a monitored `max_latency`, after arming), run it on the
S32K344 with lpuart0 unwired, and see the violation through D1's channel with
the right rule, endpoint and measured value, and no start-up entries ahead
of it. The same test on QEMU or native_sim guards it in CI; the board run is
the acceptance.

**Result (2026-10-06).** Host half landed:
`t4_an_overrun_after_arming_is_the_one_stored_violation` (`nros-node`
executor tests, run by `node-std-tests` and `test-unit`) drives the real
`max-latency-runtime` rule on the MockSession executor with an INIT/RUN node
that arms on entering RUN, then overruns once: nothing stored and the
suppressed count advancing before arming, exactly one violation after it
(rule, endpoint, measured >= declared), the drain hook's line, the four
markers, and the ring keeping the latest when overfilled. NOT done: the
native_sim / QEMU fixture. It needs a new contracted Zephyr workspace image
(bringup, contract, `nros sync`, west build, router, `nros-tests` case) --
estimated past half a day in this environment, so stopped here. The cheapest
route is a variant of `workspace-zephyr-cpp-derived-tiers` with
`CONFIG_NROS_MONITOR_ARM_ON_CALL=y` and `CONFIG_NROS_VIOLATION_DRAIN_REPORT=y`,
asserting `contract violation #1: max-latency-runtime` on the console. The
board run stays the island's.

**Board result (2026-10-06, the island's phase9-W4, image at this phase's
PR head).** The channel works on the S32K344: `armed` reads 0 before the
inputs are established and 1 after (the handler arms when every input is
established, not at INIT_TIMEOUT, because INIT always times out on the board
before Autoware starts), start-up noise lands in `suppressed_before_arm`
(5-8 entries), the record is read by symbol into every act's
`violations.txt`, and a stored violation is four markers in the island's
trace (written at 256 + id, 277-280, beside the island's own 1-31). A
commanded 250 ms overrun of the handler's tick after RUN (`ISLAND_DEBUG_
OVERRUN`, written over SWD) shows as a 250.65 ms callback in the trace
followed by `timer-overrun-runtime` x4 and `release-jitter-runtime`
246918 us; an HPC-loss act afterwards stored nothing during the act
(VERDICT PASS). The gate as written ("exactly one `max-latency-runtime`
verdict, ring empty after RUN") did NOT pass, on three counts that are this
runtime's, now I6-I8 below. Readouts verbatim in the island's
`docs/takeover-trace.md` section 12.

### I6 -- `max-latency-runtime` never judges a timer-fired callback

The 250 ms overrun produced no `max-latency-runtime` verdict at all, on a
handler whose `on_timer` path carries a 206 ms monitor. Reading of the
executor (unconfirmed by a unit test yet): timers fire in the spin sweep,
and that sweep measures nothing, so `attribute_latency` never sees a timer
dispatch. Every contracted path on the island is timer-driven or a service
handler invoked from one, so on this image the contract's latency rows are
never judged at run time; the host T4 test passes because its overrun is a
subscription callback. Fix: time the timer dispatch like a take-driven one
and charge it to the publishers whose count advanced; add the timer case to
T4's host test (it must fail before the fix). D3 (the route) stays separate;
this is the callback half not working for timers.

### I7 -- the /diagnostics reporter runs on the spin thread

The first stored violation halted the board 60 ms later (fatal 35,
`K_ERR_ARM_USAGE_ILLEGAL_EPSR`): the reporter builds its message in a 512 B
stack buffer on the spin thread and overflowed the 16 KiB main stack into
the idle thread's. The island raised `CONFIG_MAIN_STACK_SIZE` to 24 KiB
(high-water 18,780 B) to get past it. The reporter also costs 15-20 ms of
spin per verdict over the serial link, which itself trips
`release-jitter-runtime` and `timer-overrun-runtime`, so one verdict breeds
more. Fix: size the main stack from the reporter's need in the derivation
(or move the reporter's buffer to `.bss`), and report off the spin path
(deferred to the next idle slot, or coalesced), so a verdict does not cost
the tick that is being judged.

### I8 -- the first rate window after arming reads short

Four `rate-hierarchy-runtime` verdicts appear once, right after arming, at
9984-9990 mHz against 10000 (0.03-0.16 % short) and never recur. Arming
reopens the rate windows mid-period, so the first window counts one period
less than a full window. Fix: start the first window at the next sample
after arming (or allow one period of tolerance on the first window only).
With I6-I8 in place the island's ring should read EMPTY after RUN apart
from the silence verdict that needs an epoch clock (island phase 9 W3).

## Cross-repo

- **play_launch** has a parallel phase on the same findings: F1 (charge
  transport on the reaction walk and in a reported fault's detection), F2 (a
  release-jitter key for a timer trigger), F3 (a transport or queueing key on
  a service edge), and a key for an on-demand topic, which also answers D2's
  two `rate-hierarchy-runtime` entries. D3 and D4 here should land against
  the same vocabulary.
- **simple-autoware-safety-island** has its own phase 9, which consumes this
  phase's items (the violation channel, the arming, the trace hooks) and the
  play_launch phase's keys, and re-runs the board acts on them.

## Order

D1 before I1 and D2 before I2; I1 and I2 before T4. D3 waits on the
play_launch phase's F1 decision. D4, I3, I4, I5, T1, T2 and T3 are
independent of each other.

## What this phase does not do

- Change the island's contract or budgets; that is the island's phase 9.
- Change rlm or play_launch; their keys are the play_launch phase's.
- Re-measure anything: every figure above is quoted, with its image named.

## Acceptance

- [ ] D1, D2, D3, D4 each decided and written into this document.
      (D1 and D2 decided 2026-10-06; D3 and D4 open.)
- [x] I1: the generated Zephyr entry drains violations into the chosen
      channel; the ring keeps the latest entries and reports the dropped count.
      (Ring keeps the latest N with total/dropped; SWD record, markers 21-24,
      opt-in drain hook; issue 1714.)
- [x] I2: no start-up violation is recorded as a running one on the island
      image. (Mechanism landed, default unchanged; the island image opts in
      with `CONFIG_NROS_MONITOR_ARM_ON_CALL=y` and `nros::arm_monitors()` at
      `INIT_DONE`; issue 1715.)
- [ ] I3: a take hook and a per-timer sampling rate exist and the island
      traces its F4 hops with them.
- [ ] I4: the host metadata probe produces metadata for all four island
      components.
- [ ] I5: the derived heap and the configured heap agree, and hold after
      FirstSpin under a host graph on QEMU.
- [ ] T1: the plain-router join reproduced on current main with the default
      lease and held; the `min()` question filed or answered.
- [ ] T2: `island_trace_cost_max` reads the bracket's maximum in every run.
- [ ] T3: issue 1533 resolved and archived; issue 1534 closed with a derived
      main-thread priority and a measured RX-ring default.
- [ ] T4: a deliberate overrun on the S32K344 is reported through D1's
      channel. (Host executor e2e landed; the board run of 2026-10-06 shows
      the overrun as timer-overrun and jitter verdicts and markers, not as
      `max-latency-runtime`: I6. native_sim / QEMU fixture open.)
- [ ] I6: a timer-fired callback that overruns its `max_latency` produces a
      `max-latency-runtime` verdict, on the host test and on the S32K344.
- [ ] I7: the reporter neither overflows the main stack nor costs the spin
      thread a tick per verdict; the island's 24 KiB workaround can go.
- [ ] I8: no rate verdict in the first window after arming on a stream
      publishing at its declared rate.
