---
id: 1339
title: "`buffer:` and a path's `trigger:` are stated, validated, reasoned about — and dropped by the SystemModel schema"
status: open
area: cli, launch, contract
severity: medium
found: 2026-09-12
related: [1256, 0760, phase-454, RFC-0100]
---

# What is wrong

The launch contract states three facts the RFC-0100 D9 queue-depth default is
built from. Two of them do not reach nano-ros, because the **SystemModel schema
has no field for them** — not because nano-ros fails to read them.

| fact | contract key | manifest type | SystemModel type | arrives? |
| --- | --- | --- | --- | --- |
| publish rate | `topics.<t>.rate_hz` | `TopicDecl` | `TopicContract::rate_hz` | **yes** |
| publish rate | `<n>.pub.<ep>.min_rate_hz` | `EndpointProps` | `PubContract::min_rate_hz` | **yes** |
| drain rate | `<n>.paths.<p>.trigger.timer.rate_hz` | `Trigger::Timer` | — `PathContract` has no `trigger` | **no** |
| discipline | `<n>.sub.<ep>.buffer` | `EndpointProps::buffer` | — `SubContract` has no `buffer` | **no** |

Measured on `ros-launch-manifest` **v0.1.35** (the pin across every nano-ros
crate; it is also the newest tag) with the pinned `nros-launch-resolve`, over
`packages/cli/nros-cli-core/tests/fixtures/queue_buffer/`. The contract states
`buffer: queue` on a `state: true` subscription and
`trigger: { timer: { rate_hz: 10 } }` on the path that drains it; the resolved
model carries:

```yaml
contracts:
  sub_endpoints:
    /listener/chatter:
      state: true            # the sibling key on the SAME endpoint DID travel
      qos: { history: keep_last }
  node_paths:
    /listener/drain:
      output: [/listener/status]   # and nothing else
```

# Why this is not "an unused key"

The resolver does not ignore these. It **parses** them, **validates** them
(`buffer` outside `state: true` is a parse-time error) and **reasons** about
them — on this very fixture it emits

> `[queue-drain-rate] warning: node 'listener' timer path 'drain' rate_hz (10)
> is less than the sum of its 'buffer: queue' subscriptions' producer rates
> (50, from ["chatter"]) — the queue will accumulate backlog every period`

which is the publish rate divided by the drain rate, joined per node, with the
discipline selecting the population. Every input RFC-0100 D9's default needs is
already computed at layer 2 and then discarded at the model boundary. Nothing
needs inventing upstream; it needs **carrying**.

This is issue 1256's shape one layer further out. 1256 was "the contract states
four QoS policies and `from_model` reads one"; this is "the contract states a
fact, the resolver acts on it, and the artifact nano-ros reads has no field for
it". A declaration that is legal to write, legal to resolve, acted upon, and
then dropped is one the author believes they made.

Compare issue 0760, which is the same boundary from the other side: a schema
that belongs to `ros-launch-manifest` rather than to nano-ros.

# Consequence

phase-454 W8 lands the whole derivation — the ladder, the arithmetic, both
diagnostics — and it is **inert on every image in this tree**, because no
contract can produce a `queue` endpoint in a SystemModel. The wave ships the two
rates wired (`EntityDecl::{publish_rate, drain_rate}`, read from
`contracts.topics.<t>.rate_hz` and from the `min_rate_hz` of what a node's timer
paths publish) and `EntityDecl::buffer` permanently `None`, so
`queue_depth_defaults()` reports `NoDefault::NotAQueue` for every endpoint.

The cost is not only the missing default. The drain rate nano-ros *can* see is a
**substitute**: the `min_rate_hz` promise made by whatever the timer publishes,
which is the convention `nros_orchestration_ir::mapper_input::pub_rate_hz`
already uses for a periodic path's fire rate. It is not the authored timer rate,
it is absent for a drain timer that publishes nothing, and the resolver itself
calls such a `min_rate_hz` *"redundant and can be deleted"* — advice that, taken,
removes the only spelling of the drain rate that survives.

# What would fix it

In `ros-launch-manifest` (and the resolver that writes the model):

1. `SubContract` gains `buffer: Option<Buffer>`, emitted where the endpoint
   states one.
2. `PathContract` gains the effective trigger — at minimum the timer rate.
   `PathDecl::effective_trigger()` already computes it; today the model keeps
   only the `input`-empty/non-empty distinction that falls out of it, which is
   also why `from_model` counts `once` and `spontaneous` paths as timers
   (documented there, over-counting in the safe direction).

Then in nano-ros, both edits are one line each, and both are marked:

* `EntityInventory::from_model`'s `buffer: None`, with this issue number beside
  it, becomes a read of the new field.
* the `drain_rate_by_node` derivation reads the path's own rate instead of its
  output's promise.

# Acceptance

`packages/cli/nros-cli-core/tests/contract_queue_buffer_reaches_the_model.rs`
holds two tripwires that go **red** the day this closes, each naming what to
wire:

* `the_model_does_not_carry_the_buffer_discipline_yet`
* `the_model_does_not_carry_a_paths_trigger_rate_yet`

Closing the issue means: those two are deleted, the two reads above are live,
and a contract stating `buffer: queue` with both rates derives a depth end to
end — the case phase-454 W8's unit tests currently exercise over hand-built
rows because no contract can reach them.
