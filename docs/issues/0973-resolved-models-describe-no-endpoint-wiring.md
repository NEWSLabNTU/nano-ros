---
id: 973
title: "No resolved SystemModel describes endpoint wiring — 0 of 119 carry topics, services or actions"
status: open
area: orchestration
severity: medium
found: 2026-09-01
related: [0900, RFC-0060, RFC-0063]
---

# `structure` carries placement, never endpoints

Measured across every resolved model in the tree:

```
resolved models        : 119
describe ANY wiring    :   0
have an actions block  :   0
have an action CLIENT  :   0
```

Every model's `structure` holds `scopes` and `nodes` and nothing else. The
schema has room for more — `ros_launch_manifest_model`'s `Structure` declares
`topics: BTreeMap<String, TopicWiring>`, `services` and `actions` (both
`ServiceWiring`, which carries `server` AND `client` lists) — and all three are
empty everywhere.

The sharp case: `examples/workspaces/rust/build/nros/models/demo_bringup/
action_client_model.yaml` resolves `action_client.launch.xml`, whose single node
`/fibonacci_client` IS an action client. Its `structure` names the scope and the
node, and has no `actions` block at all.

## Why this is not obviously a bug

`entity_facts.rs` already anticipates it. `describes_wiring()` tests
`!structure.topics.is_empty() || !services.is_empty() || !actions.is_empty()`,
and `a_model_that_describes_no_wiring_abstains_on_the_app_count` asserts that a
model without wiring makes the consumer ABSTAIN rather than report a zero. So
the code is correct for this state; the question is whether the state is
intended.

Two readings, and the difference matters:

* **Intended** — a launch file declares nodes, and endpoint wiring is a
  different layer that only some resolves produce. Then the abstain path is the
  permanent normal case, and anything wanting endpoint counts must get them
  elsewhere (the per-component sidecar, which issue 0900 extended for exactly
  this reason).
* **A gap** — the resolver can derive wiring and does not, or does and drops it
  somewhere between layer 2 and the written model. Then `describes_wiring` is
  guarding a bug rather than a design.

Nothing in the tree distinguishes these, which is itself the problem: a
consumer cannot tell "this system has no services" from "this model does not
say".

## What it currently costs

Issue 0900 wanted the action-client count to size the executor arena — 74,240
bytes against 16,384, ~56.5 KiB of TASK STACK per image. The model is the
natural source and abstains on all 119, so that path delivers nothing today. The
fallbacks are a per-component sidecar (which cannot cover cross-compiled
components — they are unprobeable by construction) and a post-link `nm` gate
(landed, and reporting 181 over-budgeted images).

More generally, any future sizing that wants "how many X does this image have"
meets the same wall.

## ANSWERED 2026-09-03 — reading 1. Wiring is AUTHORED, and nobody authors it

Traced through the resolver rather than inferred from the models.

`model_builder.rs` DOES populate `structure.topics` and `structure.services`
(and, since R1-P2, `structure.actions`). It fills them from `ManifestIndex`,
which `manifest_loader` builds by reading a **`<stem>.contract.yaml` sitting
beside each launch file**. Endpoint wiring is a separate authored artifact, not
something derived from the launch XML.

Counted in this tree:

```
*.launch.xml            : 93
*.contract.yaml         :  0
```

So the models are empty because **nothing declares wiring**, not because the
resolver drops it. `describes_wiring()` is guarding a design, and the abstain
path is the permanent normal case until someone authors contracts.

**Reading 2 was true once and is already fixed.** `manifest_loader.rs:255`
records it: "Previously the loader silently dropped `actions:` — the model's
`structure.actions` was always empty." That is the exact bug this issue
suspected, found and repaired by R1-P2 before this issue was filed. Looking for
it again would have been a second search for a closed defect — which is why this
was traced to the loader instead of stopping at the empty models.

### What that means for the consumers

Any sizing that wants "how many X does this image have" from the MODEL needs
contract files to exist first. That is a product decision — 93 of them, hand
written, describing endpoints that the component sidecar already records — and
issue 0900 took the other route for exactly this reason: the per-component
sidecar now carries action and service CLIENTS, and it needs no contract.

So the recommended action is item 3 below (write the design down), NOT item 2.
Item 2 is struck.

## Work

1. ~~Decide which reading is right.~~ **Done: reading 1, evidenced above.**
2. ~~If wiring IS meant to be produced: find where it is lost.~~ **Struck.**
   Nothing is lost; the input does not exist. The one real instance of this
   (dropped `actions:`) was fixed by R1-P2.
3. **This is the action.** Say so in RFC-0060 and in `describes_wiring`'s doc
   comment, so the abstain path reads as the designed outcome rather than as a
   symptom, and consumers stop being written against it. Name the input a user
   would have to author (`<stem>.contract.yaml`) so "the model does not say" is
   actionable rather than merely true.

Found while implementing issue 0900. Filed rather than pursued: the answer
changes the resolver's contract, and 0900 had already been mis-scoped three
times by inferring one level too shallow.
