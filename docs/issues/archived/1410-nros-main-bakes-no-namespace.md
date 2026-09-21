---
id: 1410
title: "`nros::main!` bakes no namespace — a Rust image from a launch file that
  declares one still comes up at `/`"
status: resolved
type: bug
area: codegen, boot
related: [rfc-0045, rfc-0046, 0794, 1381, 1434]
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

`nros_boot_config_namespace()` has **no call site**. → now **issue 1434**, and
it is broader than this note said: see the Resolution below.

## Resolution

**The fix.** The namespace the macro already computes reaches the bake, with the
C/C++ emitter's rule and no second one. `main_macro.rs` grew two named functions
beside `deploy_overlay_tokens`:

* `baked_namespace(&node_instances, &node_namespaces) -> Option<String>` — the
  rule. A single-node image yields that node's namespace (the one issue 1172's
  `NodeIdentity` is built from, so the blob names the node exactly as the entry
  creates it); zero or several nodes yield `None`, which is
  `boot_config_view`'s `if plan.nodes.len() != 1 { return Ok(view) }` said in
  Rust. An empty string normalises to `None` BEFORE the bake — 0794's
  normalise-before-the-fold — so `BakedBootConfig::new` can never set the bit
  over `""`.
* `boot_config_static_tokens(&overlay, namespace)` — the emission, lifted out of
  an inline block so the emitted TOKENS are testable. That was the whole defect:
  the value existed, was correct, and was never threaded into the call. A test
  on the rule alone cannot see that.

**Measured, on a real image.** `examples/workspaces/features`, image
`native_rust_remap`, whose `rust_remap.launch.xml` declares
`namespace="/island"`. The blob read out of the linked ELF (symbol
`NROS_BOOT_CONFIG`, `.rodata`):

```
BEFORE                                      AFTER
.set_flags  = 0x05                          .set_flags  = 0x0d
  (NODE_NAME | DOMAIN)                        (NODE_NAME | DOMAIN | NAMESPACE)
.node_name  = "remap_talker"                .node_name  = "remap_talker"
.namespace_ = ""                            .namespace_ = "/island"
```

and the same pair in the macro expansion
(`cargo +nightly rustc -- -Zunpretty=expanded`):

```rust
// before
::nros::BakedBootConfig::new(::core::option::Option::Some("remap_talker"),
    ::core::option::Option::None, ::core::option::Option::Some(0u32),
    ::core::option::Option::None);
// after
::nros::BakedBootConfig::new(::core::option::Option::Some("remap_talker"),
    ::core::option::Option::None, ::core::option::Option::Some(0u32),
    ::core::option::Option::Some("/island"));
```

**Tests** (`baked_boot_config_namespace_tests` in `main_macro.rs`), asserting on
the emitted `BakedBootConfig::new` arguments by POSITION, each with the mutation
that kills it:

| Test | Mutation that kills it |
| --- | --- |
| `a_single_node_image_bakes_its_declared_namespace` | restore the unconditional `None` (the original defect) → `left: None, right: Some("/island")` |
| `a_root_node_bakes_the_root_namespace_like_the_c_emitter` | same → `left: None, right: Some("/")` |
| `an_undeclared_namespace_bakes_none` | force the value (`unwrap_or_default()` instead of the empty filter) → `left: Some(""), right: None` |
| `a_multi_node_image_bakes_no_namespace` | flip the multi-node arm (`node_instances.first()` instead of the one-element pattern) → `left: Some("/island"), right: None` |

**The root namespace is a VALUE here, not an absence**, and that is parity, not
a choice: `plan_from_model` derives `/` from an unnamespaced FQN and
`boot_config_view` bakes it with the bit set. Measured on the C side —
`native_c_params_entry_nros_main_generated.cpp`, from a launch file declaring no
namespace, reads `.set_flags = NROS_BOOT_SET_NODE_NAME | NROS_BOOT_SET_NAMESPACE`
and `.namespace_ = "/"`. `/` and `""` are the same namespace at runtime
(`names.rs` collapses the root; `node_identity_hash` normalises `""` to `/`), so
matching costs nothing and keeps two producers of one blob from disagreeing.

## What this did NOT change, measured

**The on-wire name, because it was already right.** The issue's headline said a
Rust image "still comes up at `/`"; that is true of the BAKED RUNG and not of
the node. `nros::main!` emits `runtime.node_identity = Some(("remap_talker",
"/island"))` per registered node (issue 1172) and `NodeRecord` creates the node
with it — visible in the same expansion, before and after. So no on-wire
difference was observable here, and none is claimed.

**The primary SESSION's namespace still reaches no runner.** Every board that
reads the blob writes `namespace: None` into the `ExecutorConfig` it resolves
(six sites across freertos / threadx / mps2-an385 / esp32-qemu), and
`nros-board-linux` maps only `node_name` off the overlay. That is issue **1434**
— filed with the measurement, and deliberately not built here: closing it means
a namespace parameter on the C-ABI board runners across ten board crates plus
the parity ledger, which is a different shape of change from this one. What this
issue bought is a blob that is TRUE, which is what the accessors and RFC-0045's
post-link patch tool need.
