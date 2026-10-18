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
(issue 1426 rule 2). `above = "transport"` lifts exactly that rank above
the transport band (`PriorityPlan::slots_above_transport`): lifted ranks
keep their relative order, the least urgent adjacent to the band, equal
ranks share; no room for them, nothing is lifted and a degradation says so.
The tier's other members are not lifted one by one: they would each take a
slot above the band for a rank no thread runs at.

**Explicit only (decided 2026-10-19).** The key is the ONLY way a tier goes
above the transport. #2312's implicit lift (a supervised member moved its
tier) is removed: a supervised node whose tier does not name the transport
stays below it, and the derivation records a note (a degradation whose
reason starts `note:`), not an error and not a move.

**The supervisor.** It is the reaction path and publishes on its own, so its
reserved priority (`supervisor_priority`) is above the transport in both
cases: one beyond the most urgent lifted rank, or adjacent to the transport
when nothing is lifted. It is reserved only when the contract supervises
something. `check-tier-priority-plan-image.py` accepts "supervisor above the
transport, tier below" as well as "supervisor above tier above transport".

**With an authored priority.** Rule 1 is unchanged: a
`[tiers.<name>.<rtos>] priority` for this target wins, and `above` does not
move it.

**A plan without a transport band.** The tier is allocated in `pool.app` as
before, and a degradation names the tier, the band and the target. Not a
build error, for the reason RFC-0079 section 5's 2026-09-27 amendment gives:
a derivation that cannot place declines and says so, and the same
`system.toml` serves several targets.

## Island

Today (no key, implicit lift removed): `mrm` runs at 5, below the transport
`[4, 4]`; the supervisor at 3. The island's
`BUILD_ASSERT(NROS_ENTRY_TIERS_MOST_URGENT_PRIORITY <
NROS_PRIORITY_PLAN_RESERVED_TRANSPORT_LO)` in `island_supervisor.c` fails on
that image until it either drops the assert or writes the key.

With the key, `src/safety_island_bringup/system.toml`:

```toml
[tiers.mrm]
spin_period = "10ms"
above = "transport"
```

allocates what #2312 did: the tier's most urgent member `mrm_handler`
(rank 5) is the one rank lifted, so the tier spins at 3 and the supervisor
gets 2.

## Blocked on

The resolver half is in (`derive::lift_above_transport`,
`derive::derive_tiers_with_named`), but `derive::names_the_transport`
answers `false` until an rlm release carries `TierDef::above` (rlm
`7e01f1e` on main, not yet tagged) and nano-ros's rlm pin moves; the bump
replaces its body with `t.above == Some(ReservedBand::Transport)`. The island's own resolve also
goes through the vendored play_launch's rlm pin
(`packages/cli/nros-launch-resolve`), so play_launch needs the same tag
before a bringup can write the key end to end.
