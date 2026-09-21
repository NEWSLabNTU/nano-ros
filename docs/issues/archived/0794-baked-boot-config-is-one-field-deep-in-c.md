---
id: 794
title: "The baked boot config carries four fields and the C/C++ emitter sets one
  — a launch-declared namespace, domain or locator never reaches a C image"
status: resolved
type: bug
area: build, codegen, boot
related: [rfc-0045, rfc-0046, phase-379, phase-266, 1050, 1410]
---

## Problem

RFC-0045 resolves boot config over a ladder — explicit argument, hosted
environment, baked default — and the baked rung is the `.nros_boot_config` blob
(`packages/platform/nros-platform-api/src/boot_config.rs`). It defines four
fields and four set-bits:

```rust
pub const BOOT_SET_NODE_NAME: u16 = 1 << 0;
pub const BOOT_SET_DOMAIN:    u16 = 1 << 1;
pub const BOOT_SET_LOCATOR:   u16 = 1 << 2;
pub const BOOT_SET_NAMESPACE: u16 = 1 << 3;
```

The **reader** handles all four —
`packages/core/nros-node/src/executor/types.rs:1172` branches on
`BOOT_SET_NAMESPACE` and the others alongside it.

The **writer** sets one. `packages/cli/nros-cli-core/src/codegen/entry/mod.rs:217`:

```rust
let (set_flags, node_name) = if plan.nodes.len() == 1 {
    ...
    ("NROS_BOOT_SET_NODE_NAME", escaped)
} else {
    ("0", String::new())
};
```

`domain_id`, `locator` and `namespace_` are emitted as `0` / `""` / `""` with
their bits clear. And **`BOOT_SET_NAMESPACE` is set by nothing anywhere** —
`nros::main!` passes `None` unconditionally, and `EnvRung` has no namespace field
at all, so the namespace reaches no rung of the ladder in any language.

So a C or C++ image built from a launch file that declares a namespace, a domain
or a locator gets none of them through the baked rung. RFC-0046 makes launch
authoritative for node identity; today it is authoritative for the node's name
and nothing else.

## Why it matters

The namespace is part of a node's identity on the wire — every topic it
publishes is prefixed by it. A device whose launch file puts it under
`/robot1/` and which comes up at `/` is not the node the system model describes,
and nothing reports the discrepancy: the field, the bit, the packer and the
reader all exist and work, so a reader of the code sees a complete feature.

RFC-0045's follow-on `nros config patch` tool would patch three fields no C image
reads.

## Also: C exposes one of the four readers

`nros_boot_config_node_name` is the only accessor in the C API. There is no
locator, domain or namespace reader, so even a hand-baked blob could not be
consulted from C. Rust's `BootConfig::from_baked` reads all four.

## Evidence

* `packages/platform/nros-platform-api/src/boot_config.rs:21-24` — four bits.
* `packages/cli/nros-cli-core/src/codegen/entry/mod.rs:214-232` — one bit set.
* `grep -rn BOOT_SET_NAMESPACE --include='*.rs' packages/core/nros-macros packages/cli`
  — no matches; nothing sets it.
* `packages/core/nros-node/src/executor/types.rs:1172` — the reader that would
  honour it.
* `scripts/api-parity.py --topic boot`, and the `gap` row on
  `rust:BOOT_SET_NAMESPACE` in `docs/reference/api-parity-ledger/boot.json`.

## Partially fixed 2026-08-25

**The namespace is now produced and readable.** `emit_boot_config_static` sets
`NROS_BOOT_SET_NAMESPACE` and bakes `namespace_` whenever the plan node carries
one, and `nros_boot_config_namespace()` reads it back. Both directions are
pinned by `a_launch_declared_namespace_reaches_the_baked_boot_config`, which
also asserts the negative case — an undeclared namespace must leave the bit
CLEAR, so the reader falls through to the next rung rather than reading an empty
string as "configured to root". Mutation-checked: dropping the bit fails it.

**All four C accessors now exist** — `nros_boot_config_{node_name,namespace,
locator,domain_id}`. `domain_id` takes an out-parameter and returns `bool`
rather than using a sentinel, because 0 is a valid domain (the same reason
`NROS_DOMAIN_ID_EXPLICIT_ZERO` exists on the init path).

## Still open: domain and locator have no producer

This is a scope statement, not an oversight. **Neither exists anywhere in
`Plan`** — `domain_id` appears exactly once in the whole `nros-cli-core` crate,
as the hardcoded literal in the emitter. So the emitter cannot bake what it is
never told.

The reason they are harder than the namespace is that they are properties of the
**image**, not of a node: a namespace belongs to one node and the plan already
carries it per node, while a domain and a locator are one per session. Wiring
them means deciding where they come from — `system.toml`, the board, or a CLI
flag — and that decision has not been taken.

Their accessors return NULL / `false` until then, which is the honest answer:
the alternative is an empty string that reads as "configured empty".

## Direction

* Decide where a baked domain and locator come from, then thread them into
  `Plan` and set their bits.
* **The ladder is uneven and RFC-0045 does not say so.** `EnvRung` has no
  namespace field at all, so the namespace has two rungs (explicit, baked) where
  the domain has three (explicit, env, baked). Either that is deliberate — a
  namespace is identity and should not be overridable by the environment — or it
  is an accident. Whichever, the RFC should state it.

## Resolution (2026-09-21)

### What was re-measured first

Two of the three claims in **Problem** had already been fixed and one had not,
so the work narrowed:

| claim as filed | state on `main` 2026-09-21 |
| --- | --- |
| the writer sets ONE bit | **false** — it set two; the namespace landed 2026-08-25 |
| the reader handles all of them | **true**, and there are FIVE now: issue 1050 appended `rmw` (bit 4, layout version 2) |
| `BOOT_SET_NAMESPACE` has no producer anywhere | **half true** — the C/C++ emitter produces it; `nros::main!` still does not |
| `EnvRung` has no namespace field | **true** |
| domain / locator have no producer | **true**, and `rmw` makes three |

### Before

A bringup whose `system.toml` declares `domain_id = 7` and `locator =
"tcp/10.0.2.2:7447"` and whose launch pushes `/robot1`. `nros-launch-resolve`
puts all three in the model, under the field the model schema itself calls
"RFC-0045 baked rung on embedded":

```yaml
execution:
  deploy:
    /robot1/talker:
      target: linux
      domain: 7
      locator: tcp/10.0.2.2:7447
      rmw: zenoh
```

`nros codegen entry --lang c --typed --model …` emitted:

```c
.set_flags  = NROS_BOOT_SET_NODE_NAME | NROS_BOOT_SET_NAMESPACE,
.domain_id  = 0,
.node_name  = "talker",
.locator    = "",
.namespace_ = "/robot1",
.rmw        = "",
```

The emitter READ `execution.deploy` — for the board slice, and for nothing
else. "The decision has not been taken" was wrong: the model had taken it, per
node, and the plan dropped it.

### After

```c
.set_flags  = 0 | NROS_BOOT_SET_NODE_NAME | NROS_BOOT_SET_NAMESPACE | NROS_BOOT_SET_LOCATOR | NROS_BOOT_SET_DOMAIN | NROS_BOOT_SET_RMW,
.domain_id  = 7u,
.node_name  = "talker",
.locator    = "tcp/10.0.2.2:7447",
.namespace_ = "/robot1",
.rmw        = "zenoh",
```

### The two questions that had to be answered, not assumed

**Multi-node.** The old writer degraded the WHOLE blob to `0` when
`plan.nodes.len() != 1`. That is right for half of it and wrong for the other
half, because the five fields are two different kinds:

* `node_name` and `namespace` are per NODE. An image running three nodes has no
  single node identity, so both bits stay clear and the runner keeps its
  `"node"` fallback — unchanged.
* `domain`, `locator` and `rmw` are per SESSION, and an image opens one. They
  now survive a multi-node plan, but only **unanimously**: every deployed node
  must declare the same value. A partial declaration ("A says 7, B says
  nothing") or a contradiction leaves the bit clear, because the blob has one
  slot and picking a winner would bake a fact some node contradicts. That is
  not a silent drop — `BakedSession::conflicts` names each such field in a
  comment in the generated TU, which is where a reader of the emitted entry is
  already looking.

**Empty vs unset.** Never `""` with the bit set. An empty locator is issue
0330's "absent — let the backend discover" and an empty selector is issue
1050's "unset, not a backend named `\"\"`", so both normalise to `None` BEFORE
the fold, in one place. `Some(0)` for the domain is the opposite case and is
kept: domain 0 is a real domain, which is the whole reason the blob carries
presence bits.

### The ladder

`EnvRung` gained `namespace`, `try_resolve_with` resolves it `env > baked >
""` like every other field, and `nros::env::env_rung` fills it from
**`$NROS_NODE_NAMESPACE`** (empty = unset, as for `$NROS_NODE_NAME`).
`$ROS_NAMESPACE` is deliberately NOT folded in, for the reason `rmw_selector`
does not fold in `$RMW_IMPLEMENTATION`. The asymmetry was not a decision about
identity — `node_name` is identity too and always had its rung. RFC-0045 now
STATES the ladder and the full variable set, which is what the Direction above
asked for.

### Tests, each mutation-checked

| test | mutation that kills it |
| --- | --- |
| `a_launch_declared_session_reaches_the_baked_boot_config` | drop ` \| NROS_BOOT_SET_DOMAIN` from the template → `missing \`NROS_BOOT_SET_DOMAIN\`` |
| `baked_session_folds_only_what_every_deployed_node_agrees_on` | make the fold take the first declared value instead of demanding unanimity → `left: Some(7), right: None` |
| `a_multi_node_image_keeps_its_session_rung_and_drops_its_identity` | restore the pre-fix "clear everything when `len() != 1`" → fails |
| `a_session_value_too_long_for_its_c_field_is_refused` | (budgets are per field now: `locator[96]`, `rmw[32]`, not one 63) |
| `a_launch_declared_session_reaches_the_plan_and_the_blob` (integration) | the end-to-end road, model → plan → blob |
| `namespace_resolves_over_all_three_rungs` | revert the reader to `baked.namespace.unwrap_or("")` → `left: "/baked", right: "/from_env"` |
| `try_resolve_namespace_env_rung` | make the edge never report the variable present → `left: "/baked", right: "/robot1"` |

24 entry goldens moved by exactly one line each (`set_flags` gained its `0 |`
prefix, so each of five facts contributes one independent term).

### Deliberately left, and filed

* **`nros::main!` still passes `None` for the namespace** —
  `packages/core/nros-macros/src/main_macro.rs:1425`. That crate was another
  agent's (issue 1381), so it was not touched. Filed as **issue 1410**: the
  value is already computed 500 lines above the bake, for the tier filter's node
  key. The Rust road already bakes the domain and the locator.
* **`nros_boot_config_namespace()` has no call site.** The generated C/C++
  entry reads only `nros_boot_config_node_name()`; its locator and domain reach
  the runner through `NROS_ENTRY_LOCATOR` / `NROS_ENTRY_DOMAIN_ID`, and its
  namespace reaches nothing. So on that path the blob is now TRUE — which is
  what RFC-0045's post-link patch tool and the `nros_boot_config_*` accessors
  need — without yet being the delivery mechanism. Closing it means a namespace
  parameter on the C-ABI board runners across ten board crates; recorded in
  issue 1410 and in RFC-0045 rather than glossed.
* **The baked domain is not range-checked at bake time.** `DOMAIN_ID_MAX` lives
  in `nros-node`, which `nros-cli-core` does not depend on; mirroring the
  constant would be a second authored copy of a cap `BootConfigError::
  DomainIdRange` already enforces, loudly, at boot.
