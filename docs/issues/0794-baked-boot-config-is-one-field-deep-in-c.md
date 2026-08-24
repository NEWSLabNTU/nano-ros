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

## Direction

Not decided here. The narrow fix is to compute `set_flags` from the plan's
resolved identity rather than from the node name alone, and to add the three
missing C accessors. The wider question is whether `EnvRung` should carry a
namespace at all — if it should not, then the ladder has three rungs for one
field and one rung for another, and RFC-0045 should say so.
