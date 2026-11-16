# Phase 570 — every derived value records why it has that value

**Status (2026-11-17). PROPOSED: W1–W4 are open, and nothing is built.**
Split out of [phase-569](phase-569-discoverability.md) W3 by the review of
2026-11-17, because it changes the producers of the sizing descriptor, not
discovery. It implements the reason column of
[RFC-0133](../design/0133-discoverability.md) D2, which stays Draft until the
user approves it. Owner: unassigned; the manager assigns it once the RFC is
approved.

## Why

`nros configure` writes `build/nros/sizing/<entry>.toml` with the values the
build derived for an image: pool sizes, heap, stack, enabled features. It
records each value, not where the value came from. When a value is wrong, the
user (and the next contributor) has to reread the deriving code to learn
whether it came from a knob, the platform or board ladder, a plan fact, or a
measured floor.

The week of 2026-11-16 showed the cost. The values that went wrong, and needed
someone to trace them by hand, were the pthread condition-variable and mutex
pools, the executor backing, the parameter store slots, the Zephyr heap, and
the stack setting against the floor the stack check measured.

phase-569 W3's `nros image show <image> --derived` prints a reason column.
This phase fills it.

## Work items

### W1 — the descriptor carries a reason per value

- **Status:** OPEN

Each derived value in the sizing descriptor gains a `because` entry. It is
structured, not prose, so the CLI can render it and a test can check it:

- `knob`: the knob name and the rung it was read from (image, board,
  platform, default);
- `plan`: the plan fact and its count (for example, the number of
  subscriptions on a node);
- `floor`: a measured floor and the margin applied to it;
- `sum`: the values it adds up, each with its own `because`.

The descriptor's schema version goes up by one, and the reader accepts the old
version with no reasons. The `because` entry is written by the code that
derives the value, at the point where it derives it; no later pass
reconstructs it.

Acceptance: the reader round-trips a descriptor with reasons and one without;
the schema test pins the four forms.

### W2 — the first wave

- **Status:** OPEN

These producers write a `because` first, in this order: the pthread
condition-variable and mutex pools, the executor backing, the parameter store
slots, the Zephyr heap, and the stack setting against the measured floor.

Acceptance: for a configured image of each platform that has these values,
`nros image show <image> --derived` prints a reason for each of them, and the
reason names the knob, plan fact or floor that a reader of the deriving code
would name.

### W3 — the entity pools

- **Status:** OPEN

Then the entity pools: publishers, subscriptions, services, clients, actions
and timers, for each RMW that sizes them.

Acceptance: as W2, for every entity pool.

### W4 — the count only goes down

- **Status:** OPEN

A test counts the derived values that carry no `because`, per platform, and
fails if the count rises above a committed ratchet. When the count reaches
zero, a value without a reason is refused at write time instead.

Acceptance: adding a derived value with no reason fails the test; the
ratchet file shows the count after W2 and after W3.

## Open questions

None. The order of W2 is the manager's call, recorded in RFC-0133
"Decisions", and the user may revise it.
