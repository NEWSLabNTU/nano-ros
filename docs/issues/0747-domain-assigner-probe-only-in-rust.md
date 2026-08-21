---
id: 747
title: "#0707's probe-and-step landed in ONE of the three domain assigners —
  the C++ and shell mirrors still collide, and `check-rmw-cyclonedds` pays for it"
status: open
type: bug
area: testing, rmw
related: [issue-0580, issue-0703, issue-0707, issue-0659]
---

## Symptom

`check-rmw-cyclonedds` failed inside `just check` three times in one session on
2026-08-21, on a DIFFERENT test each time, and passed solo immediately after
every one:

```
 7 - nros_rmw_cyclonedds_service_roundtrip (Failed)
15 - nros_rmw_cyclonedds_ros2_srv_e2e      (Failed)
 4 - nros_rmw_cyclonedds_pubsub_smoke      (Failed)
```

That is issue 0703's shape exactly — but 0703's cause is fixed and this is not
it. The third run printed the reason:

```
1787283434.368083 [5] nros_rmw_c: ddsi_udp_create_conn: failed to bind to ANY:8650: address in use
open failed
```

Domain **5**, port `7400 + 250*5 = 8650`. 0703 was about domains ≥ 102 walking
into the ephemeral range (32768+), and its fix capped the modulus at 101. Domain
5 is nowhere near that. This is a plain collision: something else on the box was
already on domain 5.

`open failed` is `create_session` returning non-OK, so the test dies before it
does anything it was written to test — which is why the failing test rotates and
why none of them implicates its own subject.

## Cause: one of three assigners learned to probe

`nros_test_domain.h` says the scheme is shared — "one scheme, three languages,
rather than a third invention" — and it no longer is.

| assigner | picks | probes for a busy domain? |
| --- | --- | --- |
| Rust `nros_tests::unique_ros_domain_id` | slot-based | **yes** — `domain_avoiding_busy` reads `/proc/net/udp` and steps to the next free block |
| C++ `nros_test_domain()` | `getpid() % 101 + 1` | no |
| shell `nros_unique_ros_domain_id` | slot, else `$$ % 101 + 1` | no |

Issue 0707 added probe-and-step because a filtered or solo nextest run is global
slot 0, so an orphan sitting on the first candidate made it collide every time.
That fix went into the Rust assigner only. The other two kept picking blind —
which is the repo's recurring failure mode, a fix landing at the site that
reported it rather than across the class.

And blind picking is worse than it looks here. `getpid() % 101` has **101
buckets**, `just check` fans out 32-way and spawns hundreds of short-lived
processes, and Linux hands out PIDs sequentially — so any two participants whose
PIDs differ by a multiple of 101 land on the same bus. A collision inside one
sweep is not bad luck, it is the expected case.

## Fix shape

Give the C++ and shell assigners the same probe-and-step the Rust one has: read
`/proc/net/udp` for `7400 + 250*D` and advance to the next free domain, bounded,
falling back to the blind pick when nothing is free (0707's own contract — a
caller with no domain has nowhere to go).

The three then agree again, which is what the header already claims. Worth doing
in one commit across all three so the next reader does not find a fourth state.

**Do not** answer this by serializing `check-rmw-cyclonedds` against the rest of
`just check`: the suite is 20 s and the collision is with whatever else on the
machine picked the same number, which serializing one lane does not prevent.

## Reproduce

Run `just check` on a busy host and watch `check-rmw-cyclonedds`; it went 3 red
in ~6 in-sweep runs and 0 red in 3 solo runs on 2026-08-21. The bind error names
the domain, so a red that prints `failed to bind to ANY:<port>` is this issue and
a red that does not is something else.
