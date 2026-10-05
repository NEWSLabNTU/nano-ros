---
id: 1694
title: "A contract that describes SOME of a native Rust image's endpoints sizes the
  image from the contract alone — the undescribed timer and subscription get no
  slot and the image dies `ExecutorFull` at boot"
status: open
type: bug
area: [cli, codegen, sizing]
severity: medium
found: 2026-10-05
related: [issue-1676, issue-1572, rfc-0100]
---

## What happens

Found while proving issue 1676 on `examples/workspaces/rust` (copied to a
scratch dir). The stock workspace has no contract; its sizing descriptor is not
derived and every pool keeps its crate default, so talker + listener boot and
publish. Adding ONE contract file that states only the talker's rate:

```yaml
version: 1
nodes:
  talker:
    pub:
      chatter: { min_rate_hz: 0.5 }
topics:
  /chatter:
    type: std_msgs/msg/Int32
    pub: [talker/chatter]
```

flips the descriptor to `status = "derived"`, `basis = "contract"`, and the
derivation counts only what the contract describes (`nros sync && nros build
native`, then `nros-cargo.toml`):

| knob | derived | the image needs |
| --- | --- | --- |
| `NROS_EXECUTOR_MAX_CBS` | `0` | 2 (talker's 1 Hz timer, listener's subscription) |
| `ZPICO_MAX_SUBSCRIBERS` | `1` (the C-array floor, from a demand of `0`) | 1 |
| descriptor `subscriber_count` / `callback_slots` | `0` / `0` | 1 / 2 |

The image then refuses at boot:

```
[INFO] nros: session open
nros: application error: ExecutorFull("talker_pkg")
```

The descriptor itself says it is incomplete (`undeclared_endpoints = 1`), and
the derivation sizes from the described part anyway. Describing the timer
(`paths: on_tick: trigger: { timer: { rate_hz: 1 } }`) and the listener's
subscription makes the same image boot (`MAX_CBS = 2`) — so the numbers are
right for what the contract says, and wrong for the image.

## Why it matters

A contract is how an integrator states ONE promise (here a rate floor, which is
what RFC-0052's monitors and issue 1676's `/diagnostics` report act on). Adding
that one promise to a working image should not shrink every pool the contract
did not mention. RFC-0100 D6 already rules that a REFUSED field contributes the
worst case, never zero (issue 1572 for durability); an endpoint the contract
does not describe at all is the same situation one level up, and today it
contributes zero.

## What a fix needs

* When `undeclared_endpoints > 0`, the entity-kind counts the contract cannot
  see must not fall below what the recorded source metadata (the `nros sync`
  sidecars) or the crate default gives — `max(contract, recorded)` per kind,
  the rule `MAX_CBS` already applies to the model's wiring vs the sidecars
  (phase-307 W4), extended to subscriptions/timers the contract never names.
* Or refuse the derivation for an incomplete contract (descriptor `status`
  not `derived`), so the image keeps the defaults it booted with.
* A regression on the scratch-copy shape above: a one-row contract on the
  stock Rust workspace boots and publishes.

Not measured: the C and C++ roads with a partial contract (their entity
inventory reads the register declarations, which may already cover this).
