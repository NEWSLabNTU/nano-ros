---
id: 760
title: "RFC-0074's `ingress` declaration is a ros-launch-manifest schema change, not a nano-ros one — park it until that discussion happens"
status: open
type: enhancement
area: orchestration, rmw
related: [issue-0506, rfc-0074, rfc-0060]
---

## Why this is its own topic

RFC-0074 has two halves and they belong to different repositories.

The **enforcement** half is nano-ros's: a router-side pacing rule, and a budget
on the zenoh-pico read task. Both are measured, and the mechanism is settled
(see #0506 and RFC-0074 — occupancy, `worst_gap = 0.94 x FRAMES + 11 ms`, and
the compile relation from `(rate_hz, burst)` to `(FRAMES, REST)`).

The **declaration** half is not:

```toml
[[subscription]]
topic = "/control/command/control_cmd"
ingress = { rate_hz = 200, burst = 4 }
```

`[[subscription]]` and everything on it is the contract schema, and that schema
is defined by **`ros-launch-manifest`** — a separate repository, consumed here
as a TAG-pinned dependency (`ros-launch-manifest-model` / `-sched`, currently
`v0.1.8`). nano-ros reads `SystemModel` from it; it does not own the field set.

So adding `ingress` is a cross-repo decision. Deciding it unilaterally inside an
nano-ros RFC would produce a field nano-ros writes and nothing else understands
— the shape RFC-0060's two-repository amendment exists to avoid.

## What is NOT blocked by this

Everything measured. The enforcement mechanism does not depend on how the term
is spelled or where it is declared:

* the router rule is emitted from whatever the resolver already knows;
* the read-task budget takes `(FRAMES, REST)`, and any declaration that yields a
  rate and a burst compiles to them by the relation in RFC-0074;
* the two resolve-time constraints (`rate_hz <= 1/c`, and
  `c x burst + floor <= tier slack`) are arithmetic on `c`, not on the schema.

A prototype could carry the numbers out-of-band — an env knob or a board fact —
and lose nothing but ergonomics.

## What the discussion has to settle

1. **Does `ingress` belong on the subscription, or somewhere else?** RFC-0074
   argues the subscription, because two subscriptions on one tier can have very
   different ingress costs and both terms describe what the device absorbs.
   That argument is nano-ros's view of its own runtime, not a schema decision.
2. **Is it a token bucket?** `(rate_hz, burst)` with burst load-bearing, per the
   pacing probe: a cap set AT the offered rate still removed every stall, so
   burstiness is the variable and a bare rate cap is refuted.
3. **What is the default?** RFC-0074 wants `burst` absent to mean a small
   constant (1-4), NOT unbounded — an unstated burst is the hole the whole RFC
   exists to close. A schema whose default is "unbounded" reopens it.
4. **Does it interact with anything rlm already carries?** QoS reliability is
   the near neighbour and is NOT a partial answer — checked in zenoh-pico:
   reliability is a publisher-side field, and `_z_declare_subscriber` has no QoS
   parameter, so a subscriber cannot signal "shed for me" (RFC-0074 open
   question 3).
5. **Who validates it?** The two constraints above need a per-platform
   per-frame cost `c`. Whether that is an rlm-visible board fact or stays a
   nano-ros-side input is part of the same conversation.

## Until then

RFC-0074 keeps the declaration as a PROPOSAL and says so at the point of use,
rather than reading as settled design. The enforcement half stands on its own
evidence and can be implemented against an out-of-band source of the two
numbers if that is ever wanted before the schema lands.
