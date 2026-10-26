---
id: 1140
title: "Two predicates ask whether a contract was authored, and they disagree on a timer-only contract"
status: resolved
area: orchestration
severity: low
found: 2026-09-06
resolved: 2026-10-24
related: [0973, 0900, 1120, 1121, 1649, RFC-0060, phase-412, phase-460, phase-486]
---

**Home: [phase-460](../../roadmap/archived/phase-460-formal-checks-on-the-contract-chain.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

# `describes_wiring` reads `structure` only; `from_model` also reads `contracts.node_paths`

Issue 0973 established that endpoint wiring in a resolved `SystemModel` is
AUTHORED — it appears only when a `<stem>.contract.yaml` resolves for the scope
— and that every consumer must therefore be able to tell "no contract was
authored" from "the contract says zero". Two consumers implement that test, and
they do not implement the same test.

| site | predicate |
| --- | --- |
| `nros_cli_core::cmd::entity_facts::describes_wiring` | `structure.{topics,services,actions}` non-empty |
| `nros_cli_core::entity_inventory::EntityInventory::from_model` | the same three **plus `contracts.node_paths`** |

`from_model` says why it includes the fourth, and the reason is right: the
contract expresses a timer as `paths: … trigger: { timer: { rate_hz: N } }`,
which the resolver flattens into a `contracts.node_paths` entry with an empty
`input`, NOT into `structure`. "A component whose only callback is a timer
describes real wiring."

`describes_wiring` does not, so a system whose contract declares only timers
reads to it as if nobody authored anything — and it abstains on a service-server
count the contract does state (as zero).

## Measured 2026-09-06

A one-node contract with a single timer path and no topics, services or actions,
beside `examples/workspaces/cpp/src/demo_bringup/launch/system.launch.xml`:

```
$ nros-launch-resolve tmp/timeronly/launch/system.launch.xml -o model.yaml
structure: topics 0  services 0  actions 0
contracts.node_paths 1

$ nros ws entity-inventory --metadata … --model model.yaml
NROS_EXECUTOR_MAX_CBS=1            <- derived; the timer is seen
NROS_EXECUTOR_ACTION_CLIENTS=0

$ nros ws entity-facts --model model.yaml
NROS_DECLARED_INFRA_QUERYABLES=none
                                   <- NROS_DECLARED_SERVICE_SERVERS is ABSENT
```

Same model, same authored contract, two different answers to "did anyone state
this system's wiring?".

## Why it is low severity today

No such contract exists in the tree: all 5 `*.contract.yaml` (the cpp workspace)
declare topics, services or actions alongside their timer paths, so both
predicates agree on all 5. This is a trap for the next contract author, not a
live defect.

## Cost when it fires — READ from the cmake, not measured

No timer-only contract exists to measure, so this half is traced rather than
observed. `entity_facts` abstaining is not free — `NanoRosEntityFacts.cmake` sets
`NROS_ENTITY_SERVERS_UNKNOWN` for the WHOLE configure the moment one entry
abstains (the shared staticlib holds the largest, and an unknown is not smaller
than anything), so `nros-zpico-build` falls back to `UNDECLARED_HEADROOM`
instead of the number the contract stated. One timer-only system in a workspace
therefore costs every image in it the queryable headroom phase-392 W5 exists to
delete.

## Work

1. Give the two consumers ONE predicate. `from_model`'s is the correct one — a
   contract that states only timers still states the system — so lift it to a
   shared helper (`ros_launch_manifest_model` side or `nros_cli_core`) and have
   `describes_wiring` call it, rather than adding a second spelling of the
   `node_paths` test. Second spellings of this exact test are how the repo got
   here.
2. Reconcile the stale prose while there: `EntityInventory::merged_per_kind_max`'s
   doc comment still says "The MODEL has no timer entity", which
   `from_model`'s own comment 160 lines above records as read-once and untrue.
3. A regression test with a timer-only model, asserting both consumers answer.

Found while resolving issue 0973 (2026-09-06). Filed rather than fixed there:
0973's change is documentation, and this one changes what a build derives.

## Resolved 2026-10-24: the second predicate was deleted with its verb

Re-checked against `main` at `8df999fc28`. Of the two predicates in the table,
only `from_model`'s is left.

* `describes_wiring` lived in `nros_cli_core::cmd::entity_facts`.
  `0be85e37c1` (issue 1649, 2026-10-05) retired `NROS_DECLARED_SERVICE_SERVERS`,
  the carrier this issue measured going missing, and moved it onto the sizing
  descriptor. `e79e8cb887` (phase-486 W9b, 2026-10-13) then deleted
  `nros ws entity-facts` and `cmd/entity_facts.rs` outright. Nothing on `main`
  spells `describes_wiring`.
* `EntityInventory::from_model` (`entity_inventory.rs`) is now the one
  "was a contract authored?" test. It reads the four inputs this issue named,
  plus `contracts.node_params` (issue 1436). Its timer-only regression test,
  `a_model_with_only_timers_is_not_mistaken_for_an_unauthored_one`, already
  exists. That covers work items 1 and 3, without a shared helper: one site
  needs no helper.
* Work item 2 (the stale prose) is done in this change.
  `merged_per_kind_max`'s doc no longer says the model has no timer entity. It
  now says the model sees only the timers a contract declares.

While re-checking, one more spelling of an emptiness test turned up:
`qos_agreement::check_model` returns early on `structure.topics.is_empty()`. It
is NOT a second answer to this issue's question. It asks whether a contract
states anything about a topic, which is the only thing a `qos_overrides.*`
parameter can disagree with. But its comment claimed `from_model` "abstains on
exactly this model", which has been false since `node_paths` and `node_params`
joined `from_model`'s test. The comment now states its own reason.
