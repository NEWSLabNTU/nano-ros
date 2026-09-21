---
id: 1402
title: "The entity inventory compares two different populations — components
  from `nros-metadata.json` (every REGISTERED one) against declarations from the
  resolved model (only LAUNCHED ones) — so components an image never launches
  void its sizing"
status: open
type: bug
area: [build, cmake, cli]
severity: high
found: 2026-09-21
related: [1390, 1388, 1389, 0460, 0196, phase-412]
---

## What this issue said first, and why that was wrong

Filed as: *"the `nros_ws_runtime` umbrella carries two declared facts and none
of the entity BUDGET knobs `_nros_entity_budget_env` exists to deliver."* The
observation was right. The mechanism was not, and none of the four candidate
causes it listed was the answer.

`_nros_entity_budget_env` is CORRECT. It returns empty at its third early
return, `NROS_ENTITY_INVENTORY_STATUS STREQUAL "derived"`, because the inventory
refused — and the configure SAYS so, at the inventory layer:

```
nros: entity sizing not available this configure --
  NROS_EXECUTOR_MAX_CBS keeps its configured value.
  <reason>
```

So the delivery road abstained by design and announced it. I measured an OUTPUT
(the umbrella's env) and inferred a mechanism without checking the INPUT.

## The real defect, measured

`examples/workspaces/cpp`, threadx-linux, `entity_inventory.json`:

```
status: refused
reason: 4 of 6 components in this image declare no entities:
    action_client_pkg::fib_client   action_server_pkg::fib_server
    service_client_pkg::add_client  service_server_pkg::add_server
components: fib_client absent, fib_server absent, listener STATED,
            add_client absent, add_server absent, talker STATED
```

Contracts are AUTHORED for exactly the four that report `absent`:

```
examples/workspaces/cpp/src/demo_bringup/launch/action_client.contract.yaml
                                               action_server.contract.yaml
                                               service_client.contract.yaml
                                               service_server.contract.yaml
```

and they DO declare entities. `action_client.contract.yaml`:

```yaml
nodes:
  fib_client:
    paths:
      on_timer:
        trigger: { timer: { rate_hz: 1 } }
        output: []
actions:
  /fibonacci:
    client: [fib_client/fibonacci]
```

`service_server.contract.yaml` declares `add_server` with an `add_two_ints`
service. The inventory reports both nodes `absent`.

The two components that ARE `stated` — `talker` and `listener` — are exactly the
two declared in `system.contract.yaml`. **That is not a contract-reading failure;
see the measured mechanism below.**

## Why the cost is out of proportion to the cause

The inventory refuses ALL-OR-NOTHING, and correctly — its own reason says why:

> Deriving over only the components that did would publish a slot count smaller
> than the image needs, and a short `NROS_EXECUTOR_MAX_CBS` fails entity
> creation at boot.

So four unread files cost the WHOLE workspace its entity sizing. Every image in
it takes unnarrowed executor defaults, which is a correctness-preserving but
oversized outcome, and it is invisible unless someone reads the configure line.

It also corrects issue 1390's reasoning. That issue argued the `nros_ws_runtime`
umbrella takes the unnarrowed default because it is compiled per CARGO ROOT and
can carry no single image's narrowing. True in general — but on this workspace
there was no narrowing to carry at all, for a reason with nothing to do with
cargo roots. 1390 stays wontfix on its own merits; this is why its measurement
looked the way it did.

## What is NOT established

* **The mechanism.** Whether the per-stem contracts are missed by the resolver,
  by stem/name matching (`action_client.contract.yaml` declares node
  `fib_client`, and the component is `action_client_pkg::fib_client`), or because
  this deploy's launch set does not include those stems. Not yet investigated —
  that is the next step and deliberately not guessed at here.
* **Whether any in-tree workspace derives.** Of the three inventory fragments on
  this host, zero are `derived`. The other build dirs were deleted, so that is
  "none here", not "never" — but it does mean the DELIVERING path currently has
  no positive control on this machine.
* **Whether `mixed` has the same cause.** Its refusal is 7 of 7 components, and
  its bringup's contract set has not been compared against its component list.

## Direction

Find why the four per-stem contracts do not reach the inventory, fix that, and
then a `derived` inventory becomes the positive control the budget road has been
missing. A gate is premature until the mechanism is known.

## CORRECTED 2026-09-21 — the mechanism, measured

The section above names three candidates and says the mechanism was not
investigated. It has been now, and the answer is the THIRD one: *"because this
deploy's launch set does not include those stems."*

Contracts ARE read. "Only the system-stem contract is being consumed" is not the
defect, and the heading above that says so is wrong.

### What the image actually resolves

`examples/workspaces/cpp/src/demo_bringup/system.toml`:

```toml
[image.threadx]
board = "threadx-linux"
```

No `launch` key — unlike its siblings, which name one
(`[image.native_service_server]` has `launch = "service_server.launch.xml"`). So
it falls back to the file's `default_launch = "system.launch.xml"`, whose
sidecar `system.contract.yaml` declares exactly `talker` and `listener`.

Those are exactly the two components the inventory reports `stated`. The other
four — `fib_client`, `fib_server`, `add_client`, `add_server` — are REGISTERED in
the workspace, so they appear in `nros-metadata.json` and compile into the
image, but they are not in this image's LAUNCH TREE and therefore carry no
declaration. Correctly: nothing launches them here, so they create no entities
at runtime.

Their per-stem contracts (`action_client.contract.yaml` and the rest) are not
read for this image because the launch files they sit beside
(`action_client.launch.xml`, …) are not in its tree. That is the channel working
as designed, not failing.

### The defect: two populations, one comparison

`nros ws entity-inventory` takes its COMPONENT list from `nros-metadata.json` —
every component `nano_ros_node_register()` wrote, i.e. everything REGISTERED —
and its DECLARATIONS from the resolved model, i.e. only what is LAUNCHED. The
completeness check compares the second against the first.

For an image whose launch tree names a subset of the workspace's components,
every non-launched component scores `absent`, and the all-or-nothing refusal
then voids the sizing for the whole image. Four components that are not in this
image's topology cost it its entity sizing.

The refusal itself is right and its reason is right —

> Deriving over only the components that did would publish a slot count smaller
> than the image needs, and a short `NROS_EXECUTOR_MAX_CBS` fails entity
> creation at boot.

— but that argument is about components the image RUNS. It does not apply to
one that is merely linked in.

### What is NOT established, and is a real choice

**Which side should move.** Two defensible answers, and this issue does not pick
one:

1. **Narrow the population.** Count only components the image's launch tree
   instantiates. Matches the runtime question the sizing asks. Needs the
   inventory to know the launch tree's node set, which it has through the model
   it already reads for declarations.
2. **Keep the population, split the verdict.** Distinguish "declared nothing"
   from "not launched here" in the reason, and refuse only on the former. A
   component compiled into an image but never launched may itself be a mistake
   worth surfacing — it costs flash — just not one that should void the sizing.

(2) preserves a signal (1) discards. (1) is simpler and cannot mis-classify. The
choice wants a maintainer, not a guess: guessing the mechanism twice is how this
issue reached its third revision.

**Whether `mixed` has the same cause.** Its refusal is 7 of 7, not 4 of 6, so it
may be a different shape — its `[image.*]` blocks and contract set have not been
compared.

**Whether any image in the tree resolves a launch tree covering every registered
component.** If none does, no in-tree image can produce a `derived` inventory,
and the delivering path has no positive control anywhere — which is consistent
with all three fragments on this host being `refused`, and is the sharper
version of that observation.

### Record

Filed on an inferred mechanism (a delivery gap in `NanoRosEntityFacts.cmake`),
rewritten on a second inferred mechanism (per-stem contracts unread), corrected
here on a measured one. Both wrong guesses came from reading an OUTPUT — the
umbrella's environment, then the `absent` list — and inferring a cause without
checking the INPUT that produced it. The candidate list in the previous revision
is the only reason this converged rather than shipping twice.
