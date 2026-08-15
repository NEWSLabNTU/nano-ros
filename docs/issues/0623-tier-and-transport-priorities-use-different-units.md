---
id: 623
title: "Tier priorities are RAW per-RTOS and transport priorities are NORMALISED
  0-31, and they land in the same FreeRTOS scheduler with nothing saying so"
status: open
type: bug
area: boards, platform
related: [issue-0506, phase-358, phase-364, issue-0579]
---

## Symptom

A system author writes a real-time tier at FreeRTOS priority 5, against a
transport band the board's config calls `16`, and reasonably concludes the tier
is *below* transport. It is above. The zenoh read task ends up starved, frames
are dropped, every publisher stalls on lwIP retransmission, and the island
freezes for 1–3 s at a time under inbound load.

This is not hypothetical — it is written down in a CONSUMER's config file,
`nano-ros-rt-eval`'s `src/demo_bringup/system.toml`, because that is where it
was paid for:

```toml
# FreeRTOS: RAW fixed-priority units (0-7 with configMAX_PRIORITIES=8,
# higher = more urgent), kept below the transport band at 4 (tcpip_thread,
# zenoh read/lease, net poll) so transport I/O is never starved. The old
# values (5/4/2) sat ON TOP of the transport tasks because the board maps
# its normalized transport priorities (16) down to FreeRTOS 4, not raw 16;
# under inbound load the starved RX drain dropped frames and every
# publisher stalled on lwIP retransmission timeouts (1-3 s island-wide
# freezes in the load sweep).
```

Nothing in nano-ros records any of that. The knowledge lives in one consumer's
comment, and the next consumer starts from zero.

## Cause: two vocabularies, one scheduler

| value | units | source |
| --- | --- | --- |
| `[tiers.<name>.freertos] priority` | **RAW FreeRTOS** (0–7) | authored, used verbatim |
| `zenoh_read_priority` / `zenoh_lease_priority` / `poll_priority` | **normalised 0–31** | `FreertosScheduling`, mapped down |

`TierSpec::priority` says so in its own doc — *"**Raw per-RTOS** task priority —
the value passed straight to the native spawn call … the `*_priority_for`
mappers in this module are a separate utility for authors who prefer a
normalized 0–31 scale; the codegen path uses the raw value verbatim."*

And `Config::to_freertos_priority` maps the other one:

```rust
(n as u32 * 7 * 2 + 31) / 62      // 16 -> 4
```

Both numbers are then passed to `xTaskCreate` in the same priority space. The
defaults make the collision the DEFAULT: `app_priority: 12` (→ 3),
`zenoh_read_priority: 16` (→ 4). Their own comment shows three unrelated
provenances — app from the old `APP_TASK_PRIORITY=3`, poll from CLAUDE.md's
"poll task priority ≥ 4" pitfall, and zenoh read/lease chosen to match
*zenoh-pico's own* default of `configMAX_PRIORITIES/2`. Nobody compared them to
each other, because the two live in different files in different units.

This is phase-364 W5's finding one layer up. That work normalised the platform
ABI's priority band precisely because *"the same number meant 'run me first' on
one board and 'run me last' on another, with nothing in the ABI recording which
convention a port used."* Same defect, between the tier table and the board's
transport knobs.

## What this is NOT

**Not "lower the read task below the tiers".** That is the obvious fix and it is
the configuration that caused the freezes. The two failure modes sit on opposite
sides:

* transport above tiers → tiers miss deadlines under inbound load (issue 0506);
* tiers above transport → RX starves → lwIP retransmission → 1–3 s island-wide
  freezes, which costs far more than the deadline it was protecting.

So this issue does not prescribe an ordering. Either can be right for a given
system. What is never right is choosing one by accident because the two numbers
are quoted in different units.

## Fix (this commit): report the collision, in one vocabulary

`report_tiers_above_transport` in `nros-board-freertos`'s `run_tiers_entry`
compares each tier's raw priority against the transport band's FLOOR
(`min(zenoh_read, zenoh_lease, poll)` after mapping) and prints both effective
values in FreeRTOS units when a tier meets or exceeds it:

```
nros: tier priority meets the transport band (FreeRTOS units):
  transport: zenoh_read 4, zenoh_lease 4, net_poll 4 (floor 4)
  tier `high` at 5 >= 4 — this tier PREEMPTS transport I/O
  Intended? then nothing to do. If not: tier priorities are RAW FreeRTOS
  units, transport priorities are normalised 0-31 mapped DOWN (16 -> 4). …
```

The FLOOR is the right comparison: the lowest-priority transport task is the
first to starve, so it decides whether transport makes progress at all.

Deliberately a report, not an error — a hard-RT tier that must preempt transport
is a legitimate design. The point is that the reader no longer has to know the
mapping, or that there is one.

## Still open

The report makes the collision visible; it does not remove it. The durable fix
is one vocabulary for both — either put the transport knobs on the same raw
per-RTOS footing as tier priorities, or move tier priorities onto the normalised
band with `NROS_PLATFORM_PRIORITY_RAW(n)` as the escape hatch (phase-364 W5
already built exactly that vocabulary, and the `*_priority_for` mappers already
exist for authors who want it).

That is a config-schema change across every RTOS board and their `system.toml`
files, so it wants its own work item rather than riding on a diagnostic.

Related: **only the FreeRTOS board is covered here.** ThreadX (0–31,
lower = more urgent) and Zephyr (negative = cooperative) have the same two
vocabularies meeting in one scheduler, and the same report would apply. Not
written blind — those boards' transport-priority paths were not examined for
this commit.

## Found by

Phase-358 W3, asking what it would mean to "bound the read task priority" after
establishing that the drain-budget premise did not survive contact with the
image (#506). The answer turned out to be that the priority is already bounded
and configurable — and that the number authors compare it against is quoted in
a different unit.
