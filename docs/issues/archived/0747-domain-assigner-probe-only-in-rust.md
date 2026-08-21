---
id: 747
title: "#0707's probe-and-step landed in ONE of the three domain assigners —
  the C++ and shell mirrors still collide, and `check-rmw-cyclonedds` pays for it"
status: resolved
resolved: 2026-08-21
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

## RESOLVED 2026-08-21 — all three assigners probe again

Both blind pickers gained the Rust one's probe-and-step: read `/proc/net/udp`
(+`udp6`) for `7400 + 250*D`, advance to the next free domain, bounded, and fall
back to the first candidate when nothing is free — issue 0707's contract, in its
own words, because a caller with no domain has nowhere to go.

Determinism is kept where it was still free: with nothing squatting the answer is
bit-identical to the old scheme, so a reproduction by hand lands where it used
to. An explicit `ROS_DOMAIN_ID` is never stepped away from — pinning one is how
you get ON a particular bus, and the interop scripts export it so their helper
meets the `ros2` CLI there.

The FIRST candidate still differs between the three (Rust partitions slots into
blocks of 4; C++ and shell fold a slot-or-pid), and that is left alone. What was
load-bearing and missing is "do not take a bus somebody is already on".

### Falsified in both directions, in both languages

Not asserted from reading. A harness binds the first candidate's SPDP port and
checks the picker moves, and separately checks it does NOT move when nothing is
bound:

```
C++   : OK: 71 busy -> stepped to 72
C++   : OK: 72 busy -> stepped to 75
shell : OK: 31 busy -> stepped to 32
```

The second C++ line is worth keeping: it stepped *past two more* occupied
domains, and a third run skipped its own precondition because domain 73 was
already taken. On a host quiet enough to be running only this, three of ~101
domains were in use — which is the collision premise measured rather than argued.

### The portability trap, and why it would have been silent

The shell probe first compared ports with `strtonum("0x" ...)`. That is a **gawk
extension**, and Ubuntu's default awk is mawk:

```
$ echo "0: 00000000:21CA" | mawk '... strtonum("0x" a[2]) ...'
mawk: line 2: function strtonum never defined
```

awk then exits non-zero, `nros_domain_busy` answers "not busy", and every domain
reads as free — the probe would have been dead on exactly the hosts CI runs,
while passing here because this box has gawk. It compares the port as uppercase
hex TEXT instead, which /proc emits in fixed 4-digit form, and that is verified
under mawk directly:

```
mawk: sees busy 41 OK
```

`check-rmw-cyclonedds` 17/17.

## The sweep the fix prompted — a second, unrelated ungrouped racer

Re-running tier 1 after the assigner fix surfaced a different in-sweep-only
failure, and it is worth separating rather than filing as more of the same:

```
nros-tests::qos_override_e2e a_ros2_peer_sees_the_overridden_publisher_profile
  qos_override_e2e.rs:166: ros2 did not discover the nros publisher on <topic>
```

Passes solo. NOT this issue's class — that test is Rust, so it already had
0707's probe, and no domain collided. It is issue **0312**'s symptom one binary
over: a stock `ros2 topic info` peer intermittently missing an endpoint inside
the settle window when pairs run concurrently, which is exactly what the
`native-qos-discovery` group (`max-threads = 1`) exists to prevent.

Grepping for that peer finds exactly two files in the tree —
`workspace_features_e2e` and `qos_override_e2e` — and the group had one of them.
So the class is two members with one protected, the same shape as phase-373 W1:
a test racing on a resource, sitting outside the group that exists for it.

`qos_override_e2e` now joins, by extending the group's existing override rather
than adding a second spelling. Verified with `show-config test-groups` (W1's
lesson: a filter parsing cleanly is not evidence its group applies), and no
earlier override matches the binary.
