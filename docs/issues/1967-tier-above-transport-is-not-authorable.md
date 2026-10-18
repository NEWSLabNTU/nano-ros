---
id: 1967
title: "A tier cannot name the band it crosses: `above = \"transport\"` is not authorable"
status: open
type: enhancement
area: [codegen]
severity: low
found: 2026-10-19
related: [rfc-0079, issue-1426, issue-1508]
---

## The gap

RFC-0079 section 6 says a tier crosses into a reserved band only by naming
it:

```toml
[tiers.safety]
above = "transport"
```

That key cannot be written. `[tiers.*]` deserializes into rlm's
`ros_launch_manifest_sched::TierDef`, which is `deny_unknown_fields` and has
no `above`, so a `system.toml` carrying it is refused at parse.

What the resolver does instead (PR #2312,
`derive::lift_supervised_above_transport`) reaches the rule from a fact the
contract already states: a node with a `supervise:`d path is allocated above
the transport band. That is the only road. A tier whose members supervise
nothing, but which the author wants above the transport, has no spelling;
and a tier that IS lifted is lifted by a side effect of a contract key, not
by anything written on the tier. The Autoware safety island
(`[tiers.mrm]`, transport `[4, 4]`) is lifted today only because
`mrm_handler/call_mrm` is supervised.

## Design

**The key.** `above` on the tier HEAD (never a platform sub-table: it is a
system fact, not one kernel's number), typed as rlm's `ReservedBand`, a
closed set whose one member is `transport`. A misspelt or unknown band is a
parse error in rlm naming `transport`, so it can never leave a tier silently
in the pool. Tier-relative order (`above = "<tier>"`, RFC-0079 section 3
rule 1) is not part of this; it is a different relation and stays
unimplemented.

**What it allocates.** A tier is one thread at one priority, and an
authored, unplaced tier already takes its most urgent member's rank
(issue 1426 rule 2). `above = "transport"` lifts exactly that rank: the
tier's most urgent member(s) join the set the #2312 lift allocates above
the transport band. So the lift is unchanged in every other respect:

* lifted ranks keep their relative order, the least urgent adjacent to the
  band (`PriorityPlan::slots_above_transport`), equal ranks share;
* one more priority beyond the most urgent lifted rank is reserved and
  recorded as `supervisor_priority`;
* no room for the ranks: nothing is lifted and a degradation says so; room
  for the ranks but not the supervisor: lifted, with a degradation.

The tier's other members are not lifted one by one: they would each take a
slot above the band for a rank no thread runs at.

**With the implicit (supervised) lift.** The two are one rule with two
sources, so they are unioned and cannot conflict: a supervised member lifts
its tier as before, and `above` lifts a tier with no supervised member. A
tier that has both is lifted once. There is no key that keeps a supervised
member's tier below the transport.

**With an authored priority.** Rule 1 is unchanged: a
`[tiers.<name>.<rtos>] priority` for this target wins, and `above` does not
move it. The boot report already prints a tier at or above the transport.

**A plan without a transport band.** `above` names a band the target's plan
does not reserve (a port that declares no band): the tier is allocated in
`pool.app` as before, and a degradation names the tier, the band and the
target. Not a build error, for the reason RFC-0079 section 5's 2026-09-27
amendment gives: a derivation that cannot place declines and says so, it
does not abort, and the same `system.toml` serves several targets.

## Island

`src/safety_island_bringup/system.toml` would write

```toml
[tiers.mrm]
spin_period = "10ms"
above = "transport"
```

and allocate the same numbers it does today: the tier's most urgent member
is `mrm_handler` (rank 5 in `pool.app [5, 14]`), the one rank lifted, so
the tier spins at 3 and the supervisor gets 2, transport `[4, 4]`.

## Blocked on

An rlm release carrying `TierDef::above` (rlm `7e01f1e` on main, not yet
tagged), then nano-ros's rlm pin bump; the island's own resolve also goes
through the vendored play_launch's rlm pin
(`packages/cli/nros-launch-resolve`), so play_launch needs the same tag
before a bringup can write the key end to end.
