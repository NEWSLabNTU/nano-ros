---
rfc: 0142
title: "Executor trace timeline: one event stream, one viewer format"
status: Stable
area: runtime-api
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: [phase-578]
amends: []
supersedes: []
superseded-by: null
---

# RFC-0142 — Executor trace timeline

**Status:** Stable (ruled 2026-11-26). Drafted the same day for phase-578
W11. All three questions were ruled as drafted (see "Rulings" below). Nothing
is built yet, and no code is written until phase-575 W5 (budget enforcement
and deadline monitors on FreeRTOS) lands, because that work item writes the
emitters for D1's new events. Tracked by phase-578 W11.

## Summary

A user should be able to record what an image's executor and scheduler did:
- which callback ran when, and on which task;
- when each scheduled job was released and when it finished;
- when a task ran out of budget and was throttled;
- when a deadline was missed.

Then they should be able to open the recording on a timeline in a viewer that
already exists.

Most of the pieces exist, but nothing joins them:
- The executor already emits most of these events through one seam.
- Each platform already has a way to record them.
- One platform already converts its recording for a viewer.

This RFC picks the event set (D1) and one viewer format, Perfetto (D2). It
then adds:
- one host-side converter from each platform's recording to that format (D3);
- one clock rule (D4);
- the cost rule (D5);
- an acceptance test that checks the result with the viewer's own reader, not
  with our decoder (D6).

## What exists (main, 2026-11-26)

**The event stream.** `nros_node::executor::callback_trace`, behind the
`trace-callbacks` feature, emits `(marker_id, arg)` pairs, two `u32`s each,
through one installed sink (`set_trace_sink`; C `rclc_set_trace_sink`). The ids
are frozen and append-only:

| id | event | arg |
| --- | --- | --- |
| 1..=15 | application markers (`rclc_trace_marker`, `rclcpp::trace_marker`) | the app's own |
| 16 | callback registered | `handle << 8 \| kind` |
| 17 | (retired name chunk, reserved) | |
| 18 / 19 | callback start / end | `handle` |
| 20 | name chunk | `handle << 24 \| 3 bytes` |
| 21..24 | violation, then its fqn hash, measured, declared | `seq << 8 \| rule code`, ... |
| 25..27 | sample taken, then its stamp (sec, nanosec) | `handle << 24 \| take seq`, ... |

A violation's rule code indexes `monitor::RULE_IDS`, which is also
append-only. It already has `deadline-miss-runtime` (code 4) and
`timer-overrun-runtime` (code 8). So a deadline miss the executor's monitor
detects is already in the stream, timestamped at detection, on the same clock
as the dispatches it judges.

**The recorders, one per platform:**
- **Zephyr:** the default sink is `sys_trace_app_marker`, carried by a
  patch on our Zephyr branch, so the events land in Zephyr's CTF stream next to
  its own thread switches. `nros-tests` decodes that stream (`ctf.rs`) and
  `zephyr_ctf_markers.rs` asserts on it.
- **FreeRTOS:** with `NROS_TRACE=1` the MPS2-AN385 board builds
  Tonbandgeraet, which records task switches, queue operations and ISRs into a
  RAM snapshot. The snapshot is dumped over semihosting when the run ends
  (`trace_dump.c`), and `just freertos trace` converts it with `tband-cli` to a
  Perfetto file. No board installs an executor sink there, so the timeline
  shows tasks but no callbacks.
- **Bare metal and POSIX:** no recorder.

**The viewers:**
- FreeRTOS reaches Perfetto, but only for task-level events.
- Zephyr's CTF stream has no in-tree path to any viewer.
- The decoder that turns the marker ids into named callback slices lives in a
  downstream project, not here.

So today:
- a FreeRTOS user sees task switches with no callbacks;
- a Zephyr user has callbacks in a file that only a test reads;
- nobody can see a budget being throttled, because nothing enforces one yet
  (phase-575 W5).

## Decisions

### D1 — the event set: reuse the stream, add four ids and one rule

Every event goes through the existing sink with the existing two-word shape.
There is no second seam, and no event is redefined.

**Already there, used as is:**
- callback register, name, start and end (16, 18, 19, 20);
- sample taken (25..27);
- application markers (1..15);
- **deadline miss:** a violation with rule `deadline-miss-runtime`, with
  `measured` and `declared` in microseconds.

Phase-575 W5's deadline monitor must report a miss through the executor's
existing violation path, not through a new event. Then the trace, the log line
and `/diagnostics` all get the same miss from one report, as every other rule
already does.

**New ids, appended:**

| id | event | arg | emitted |
| --- | --- | --- | --- |
| 28 | job released | `job seq` (low 32 bits) | when a scheduled task's job is released: the start of its deadline window |
| 29 | job complete | `job seq` | when that job's last callback returns |
| 30 | budget exhausted (throttled) | `job seq` | when the budget server stops the task |
| 31 | budget replenished | `job seq` | when the server lets it run again |

**New rule, appended to `RULE_IDS`:** `budget-overrun-runtime` (code 18). It
is reported as a violation, with `measured` the time the job ran and
`declared` its `budget_us`. Events 30 and 31 draw how long the task was
throttled; the violation is the instant a person reads and the log reports.
Phase-575 W5 owns both emitters; this RFC owns only the ids and their meaning.

**Which task an event belongs to.** Events 28..31 carry no task id. They are
emitted on the scheduled task's own thread, and the converter (D3) attributes
an event to the thread that was running at its timestamp. That thread is known
from the recorder's own context-switch events (Zephyr's thread switches,
Tonbandgeraet's task switches). This is how the existing `handle` events are
already disambiguated across several executors, where the same slot index
recurs in each.

A recorder with no context switches (bare metal) has a single executor, so
nothing needs attributing. If a later recorder has threads but does not record
switches, it must add a task id before it is supported, rather than guess.

**Not in the set, deliberately:**
- OS-level events (queue operations, ISRs, semaphores). The recorder already
  captures these; repeating them through our sink would only double the cost.
- RMW-internal events (zenoh-pico reads, lease ticks). That is a different
  layer, and it would need its own RFC.

### D2 — the format: Perfetto

The output a user opens is a Perfetto trace, in the protobuf `TrackEvent`
form, viewed at ui.perfetto.dev or with `trace_processor`. The recording stays
whatever each platform's recorder writes (D3); Perfetto is only what the
converter writes.

| | Segger SystemView | CTF | Perfetto |
| --- | --- | --- | --- |
| Viewer | Segger's desktop app; free for non-commercial use only, licence needed for commercial use | Trace Compass (Eclipse desktop) or babeltrace2 (text only) | browser, nothing to install; `trace_processor` for scripted queries |
| Recording side | Segger's target library and RTT, which needs a J-Link probe. QEMU has no RTT, so the acceptance run cannot use it | Zephyr writes it natively; FreeRTOS has no writer | none needed; converted on the host |
| Shows our callbacks as slices | yes, through user modules | only with a custom Trace Compass XML analysis written for our ids | yes: slices, instants, counters and flow arrows are built in |
| Already used here | no | Zephyr recording, test-only | FreeRTOS, through `tband-cli` |
| Licence for us | target library under Segger's terms, viewer proprietary | open | open (Apache-2.0) |

**Perfetto, as ruled.**
- It is the only one of the three that a user can open with nothing installed.
- It is already where the FreeRTOS path ends.
- It draws a callback as a slice, a deadline miss as an instant and a sample
  taken as a flow arrow into its callback, with no viewer-side configuration.

SystemView is ruled out by the probe it needs and by its licence for
commercial users. CTF is a good recording format, and Zephyr keeps writing it,
but as a viewing format it needs a desktop IDE and a custom analysis before it
shows anything of ours.

### D3 — one converter, one reader per recorder

`nros trace convert <recording> -o <out>.pftrace` is a new CLI verb that does
three things:
1. A reader per recorder extracts the platform's own tracks (threads or tasks,
   with their switches) and our marker pairs, all with timestamps on the
   recorder's clock.
2. A single decoder turns the marker pairs into named tracks: one per
   callback, nested under its thread, plus job windows, throttled intervals,
   violations and take arrows.
3. A single writer emits the Perfetto protobuf.

**The readers:**
- **CTF (Zephyr):** promote the reader `nros-tests` already has (`ctf.rs`),
  which decodes against the building tree's TSDL metadata, to a library crate
  that both the CLI and the tests use.
- **Tonbandgeraet (FreeRTOS):** the FreeRTOS board installs a sink that
  records each pair as a Tonbandgeraet value marker
  (`tband_valmarker(marker_id, arg)`), so the pairs travel in the same
  snapshot as the task switches. The reader decodes Tonbandgeraet's binary
  format itself.

  Two constraints on that reader:
  - `tband-cli` is GPL-3.0 and is not linked or vendored into the CLI.
  - Tonbandgeraet's evaluation records its format as still subject to change.
    So the reader is pinned to the submodule's commit and is tested against a
    recording captured with that commit.

  If decoding it ourselves proves larger than its value, the fallback is
  `tband-cli`'s own Perfetto output, merged with ours: Perfetto traces are
  sequences of packets, so two files on one clock can be concatenated. W11's
  first step measures which of the two is smaller.

The decoder is the in-tree copy of what the downstream project does today. It
keeps the existing rules from `callback_trace.rs`:
- a name chunk binds by its handle;
- a 22..24 or 26/27 without the event it follows is dropped, never guessed;
- an id the decoder does not know is an error, not a skip.

### D4 — one clock: the recorder's

Every event's timestamp is the one the recorder put on it: Zephyr's CTF
timestamp, or Tonbandgeraet's cycle counter. The converter never mixes clocks
and never re-times an event.

The executor judges a deadline on its own clock, and the trace shows the
violation at the moment it was detected. So the trace is evidence of what the
executor decided, not a second judgement of it.

Under QEMU, guest time per instruction depends on host load unless icount is
fixed (issue 2971). A trace's durations are therefore meaningful only from a
run under a fixed shift. The acceptance test uses the `_measured` launchers,
which return `FixedIcount`.

### D5 — cost: nothing unless asked for

The new ids sit behind the same `trace-callbacks` feature as the rest, so an
image built without it carries no call sites. With it on and no sink
installed, each event costs one atomic load and one branch, as the existing
ones do.

The FreeRTOS value-marker sink is compiled only under `NROS_TRACE=1`. The
Tonbandgeraet snapshot buffer, 16 KiB on MPS2-AN385 today, is the only RAM the
feature adds there, and it shows in the size report `nros build` prints
once phase-578 W1 lands.

### D6 — acceptance checks the viewer's own reader

The phase's acceptance is "a QEMU run's trace opens in the chosen viewer and
shows an injected deadline miss."

Checking that with our own decoder would prove only that our writer and our
reader agree. So the test opens the converted file with Perfetto's
`trace_processor_shell` and queries it, asserting that:
- a slice exists for the callback;
- a job window encloses it;
- a `deadline-miss-runtime` instant falls after the window's deadline.

**Ruled:** provision `trace_processor_shell` through
`nros-sdk-index.toml` as a pinned prebuilt, one binary for each host
architecture. It is fetched by `nros setup` like any other tool and never
downloaded by the test.

**Not chosen:** checking the file against Perfetto's published protobuf
schema only. That needs no binary, but a file can parse against the schema and still
show nothing in the viewer.

The run itself: a FreeRTOS MPS2-AN385 fixture (phase-575 W5's board) whose
scheduled task has a deadline its callback is built to miss. It runs under
the fixed-icount launcher, and the fixture is built in the build stage like
every other.

A Zephyr run of the same image follows, with the same assertions through the
CTF reader. That is the check that two recorders produce one timeline.

## Rulings (2026-11-26)

The user ruled all three questions as drafted, relayed by the manager session.

1. **A budget overrun is its own violation rule, `budget-overrun-runtime`**
   (appended to `RULE_IDS`, code 18), not only the throttle interval. It then
   reaches the log line and `/diagnostics` the way a deadline miss does, and
   events 30 and 31 still draw how long the task was throttled.
2. **The viewer format is Perfetto.** SystemView and CTF are not viewer
   formats here; Zephyr keeps CTF as its recording format, read by D3.
3. **The SDK provisions `trace_processor_shell`** as a pinned prebuilt per
   host architecture in `nros-sdk-index.toml`, fetched by `nros setup`. The
   acceptance test queries the converted trace with it and never downloads
   it itself.

Phase-575 W5's owner was told about ids 28..31 and the new rule, since that
work item writes their emitters.

## Not decided here

- **A run-time verb** (`nros run --trace`) that captures and converts in one
  step. It would follow once D3 exists. Capture today is per platform
  (`NROS_TRACE=1`, `CONFIG_TRACING_CTF`).
- **Streaming capture.** RTT, or Tonbandgeraet's streaming backend over a
  serial line, for runs longer than a RAM snapshot.
- **Bare-metal and POSIX recorders.**

## Changelog

- 2026-11-26 — Ruled: all three questions as drafted (a budget overrun is
  its own rule, `budget-overrun-runtime`; Perfetto; `trace_processor_shell`
  provisioned by the SDK for acceptance). Draft to Stable; work is phase-578
  W11, after phase-575 W5.

## Related

- RFC-0002 (the execution model: `budget_us`, `deadline_us`,
  `deadline_policy`).
- RFC-0016 (RTOS scheduling and the Tonbandgeraet validation).
- RFC-0017 (the sporadic budget timer).
- RFC-0102 (the logger hierarchy, which a violation also reaches).
- Phase-575 W5 (the budget and deadline emitters).
- Phase-578 W11 (this work item).
- Issue 2641 (the application marker block).
- Issue 2766 (the Zephyr CTF marker patch).
- Issue 2971 (guest time under QEMU icount).
