---
id: 794
title: "The baked boot config carries four fields and the C/C++ emitter sets one
  — a launch-declared namespace, domain or locator never reaches a C image"
status: open
type: bug
area: build, codegen, boot
related: [rfc-0045, rfc-0046, phase-379, phase-266]
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
