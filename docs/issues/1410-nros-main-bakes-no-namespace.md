---
id: 1410
title: "`nros::main!` bakes no namespace — a Rust image from a launch file that
  declares one still comes up at `/`"
status: open
type: bug
area: codegen, boot
related: [rfc-0045, rfc-0046, 0794, 1381]
---

## Problem

This is the one site issue 0794 left standing, carved out because
`packages/core/nros-macros/**` was another agent's file at the time (issue
1381).

`nros::main!` bakes the `.nros_boot_config` blob for a Rust image. It passes
four arguments to `BakedBootConfig::new`, and the fourth — the namespace — is
an unconditional `None` (`packages/core/nros-macros/src/main_macro.rs:1425`):

```rust
::nros::BakedBootConfig::new(
    #node_name_opt,
    #locator_opt,
    #domain_opt,
    ::core::option::Option::None,
);
```

So `BOOT_SET_NAMESPACE` is never set on the Rust road, and a Rust image built
from a launch file that puts its node under `/robot1/` comes up at `/`, with
nothing reporting the discrepancy.

## Why it is small

**The value already exists in the same function.** `main_macro.rs:934` computes
each node's namespace from its FQN, for the tier filter's node key (issue 1172,
`NodeIdentity::new(&bare, &namespace)`). It is discarded at the bake and nowhere
else.

**Every other rung is wired.** After issue 0794:

* the C/C++ emitter bakes a launch-declared namespace with its bit set,
  single-node only — `nros-cli-core/src/codegen/entry/mod.rs`,
  `a_launch_declared_namespace_reaches_the_baked_boot_config`;
* the reader honours the bit — `BootConfig::from_baked`;
* the HOSTED rung exists — `EnvRung::namespace` / `$NROS_NODE_NAMESPACE`,
  `namespace_resolves_over_all_three_rungs`.

So the field, the bit, the packer, the reader, the env rung and one of the two
producers all work. This is the other producer.

## Direction

Thread the namespace the macro already computes into the bake, with the same
rule the C/C++ emitter uses: a SINGLE-node image bakes its node's namespace, a
multi-node image bakes none (there is no single node identity to name, and the
blob has one slot). An undeclared namespace must leave the bit CLEAR, so the
reader falls through to the next RFC-0045 rung rather than reading `""` as
"configured to root" — the negative direction is half the test.

Acceptance: a Rust fixture whose launch declares a namespace comes up under it,
asserted on the blob's bits and, where a fixture can run, on the on-wire name.
Mutation-check by clearing the bit.

## Also still open, and a different shape

`nros_boot_config_namespace()` has **no call site**. The generated C/C++ entry
reads `nros_boot_config_node_name()` and nothing else from the blob; its
locator and domain come from the `NROS_ENTRY_LOCATOR` / `NROS_ENTRY_DOMAIN_ID`
compile definitions, and its namespace reaches nothing at all. Closing that
means giving the C-ABI board runners (`run_components` / `run_tiers`, ten board
crates plus the parity ledger) a namespace parameter. That is a bigger change
than this one and should be filed separately when someone takes it; the blob is
at least TRUE now, which is what a post-link patch tool and the accessors need.
