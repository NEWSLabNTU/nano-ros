---
id: 1339
title: "`buffer:` and a path's `trigger:` are stated, validated, reasoned about — and dropped by the SystemModel schema"
status: resolved
area: cli, launch, contract
severity: medium
found: 2026-09-12
related: [1256, 1372, 0760, phase-454, phase-457, RFC-0100]
resolved: 2026-09-28
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

---

# RESOLVED — 2026-09-28

## What the upstream half turned out to be

The issue's "What would fix it" asks for two additions to `ros-launch-manifest`
and for the resolver to write them. **Both had already landed before this issue
was closed, and before it was even filed in its current form:**

* `SubContract::buffer: Option<BufferContract>` and
  `PathContract::trigger: Option<sched::EffectiveTrigger>` were added by rlm
  commit `9563b54` — *"feat(phase-52 R1): the model carries every fact the
  checker resolves"* — first released in **rlm v0.1.37**, which is BELOW the
  `v0.1.40` this tree already pins. The commit is explicit about the substitute
  this issue names: *"The timer's rate lives HERE and nowhere else: a consumer
  that recovers it from a publisher's `min_rate_hz` is guessing."*
* `play_launch`'s resolver lowers both
  (`resolve/src/ros/model_builder.rs`, `sub_contract` and `path_contract`), at
  the commit this tree already pins.

So **no rlm tag was cut and no submodule pin was moved.** The nano-ros pin
(v0.1.40) and the play_launch pin already carry everything; the schema table in
the body above describes rlm v0.1.35 and is the state at filing, not the state
at closing. `PathContract` is also where `sync`, `min_latency_ms`, `max_jitter_ms`
and `miss` arrived, which retires a second "when those travel" caveat in
RFC-0100 D9's margin argument.

phase-457 W1 had already flipped the two tripwires the Acceptance section names
(`the_model_does_not_carry_*_yet` → `the_model_carries_*`), so what remained was
only the CONSUMER half.

## What actually changed here

Two reads in `EntityInventory::from_model`
(`packages/cli/nros-cli-core/src/entity_inventory.rs`):

1. `buffer: None` → `buffer: sub_buffer_of(ep)`, one translation from
   `model::BufferContract` into `queue_depth::BufferDiscipline`. An absent key
   stays `None` and is NOT defaulted to `Latest`.
2. `drain_rate_by_node` reads `contracts.node_paths.<p>.trigger` and matches
   `EffectiveTrigger::Timer { rate_hz }`, instead of taking the `min_rate_hz`
   of whatever the input-empty path publishes. The substitute is **retired, not
   superseded**: a model older than rlm v0.1.37 carries no trigger, such a path
   is `Unclassified`, and the endpoint reports `NoDefault::NoDrainRate` rather
   than a depth derived from a number nobody stated. `NoDrainRate`'s prose moved
   with it — it used to advise stating the substitute.

The timer ENTITY count (`input.is_empty()`, ~20 lines below the drain-rate
derivation) was deliberately **not** narrowed to `Timer`, and now says so in a
comment. It is a pool size, so its over-count (a `once` or `spontaneous` path
reads as a timer) is the safe direction; narrowing it is an under-size that
needs its own measurement.

## The transition, watched

At `HEAD` before the change, all six tests in
`tests/contract_queue_buffer_reaches_the_model.rs` passed, including
`the_reason_names_the_discipline_because_both_rates_did_arrive`, which asserted
`Err(NoDefault::NotAQueue)` — both rates live, discipline unreadable.

With the two reads wired, that one test went **RED**, alone:

```
assertion `left == right` failed: both rates arrived, so the missing fact is the
discipline: /chatter: depth 6 derived from 50 Hz in / 10 Hz drained (derived_from_rates)
  left: Ok(6)
 right: Err(NotAQueue)
```

That is RFC-0100 D9's arithmetic — `ceil(50 / 10) + 1` — from a real contract
through the real resolver, which is the case the wave's unit tests could only
reach over hand-built rows. The test is now
`the_endpoint_gets_its_depth_because_all_three_facts_arrived` and asserts the
depth, the `NROS_ENTITY_DERIVED_DEPTH_COUNT 1`, and the row's discipline.

## Bytes: none moved, and the reason CHANGED

No image's sizing moves. But the old proof was **structural** — "no contract on
earth can produce a `queue` endpoint" — and that proof is gone with the schema
gap. What holds now is weaker and is therefore MEASURED rather than asserted:
**no contract in this tree writes `buffer:` at all**, so `depth_default`'s first
rung returns `NotAQueue` for every endpoint in every image and the derived-depth
list stays empty. New test: `no_shipping_contract_derives_a_depth`, which sweeps
every `examples/**/*.contract.yaml` and fails the day one states a discipline —
at which point the byte change is intended and must be re-measured against a
build.

The same inversion is written into RFC-0100 D9 and phase-454 W8, because both
recorded the structural claim as the finding.

## Gates

* `cargo test --manifest-path packages/cli/Cargo.toml` — 95 test binaries green.
* `tests/contract_queue_buffer_reaches_the_model.rs` — 7 tests (was 6).
* Two unit tests in `entity_inventory.rs`: the positive
  (`the_model_supplies_the_discipline_and_both_rates_a_queue_default_divides`,
  whose model states a `min_rate_hz: 10` beside a `trigger` of 25 Hz so the
  assertion distinguishes the two sources) and its negative control
  (`a_model_older_than_the_trigger_field_yields_no_drain_rate`).

## Left open

Issue **1372** is the same substitute on the SCHEDULING side
(`mapper_input::pub_rate_hz` → the realizer's period). It reads as already
resolved — `pub_rate_hz` no longer exists in `nros-orchestration-ir`, and
`mapper_input_and_report` delegates to rlm's own `derive` crate — but that was
phase-457 W2's doing, not this issue's, and 1372 should be closed by whoever
measures it.
