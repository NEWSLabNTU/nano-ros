---
id: 1692
title: "`multihost_bake_emits_only_the_hosts_node` tests a verb that was retired —
  `nros codegen entry --lang rust` refuses since phase-432 W2.4, and an unstamped
  resolver pin refuses it one step earlier"
status: open
type: bug
area: [testing, codegen]
severity: medium
found: 2026-10-05
related: [1651, 0427, phase-432, phase-460]
---

## What fails

`nros-tests::multihost_partition_bake multihost_bake_emits_only_the_hosts_node`,
deterministically (CI run 37252649866, and locally solo in 0.16 s, no fixture
involved). It hits two refusals in sequence:

1. `Error: codegen entry: SystemModel …/robot1_model.yaml is stale: resolver
   pin changed (model 0.9.0 != ours bbf9c04496d0)`. The test runs
   `nros-launch-resolve` itself, which stamps its own crate version; only
   `nros sync` re-stamps the `play_launch` pin (`stamp_resolver_pin`, issue
   0427), and `codegen entry --model` has verified provenance at the door since
   phase-460 W1 (2026-09-22). Dropping `meta.resolver` (an unpinned model is
   unverifiable, not stale) clears this one — measured.
2. Behind it: `Error: --lang rust entry is retired (phase-432 W2.4): a Rust
   entry is emitted by the nros::main!() proc-macro at compile time.` The test
   asserts on the text of a generated `main.rs` from that verb, so there is no
   one-line fix: the assertion has to move to whatever now carries per-host
   partitioning for Rust (the `nros::main!` expansion), or to the C/C++ verb.

The two sibling tests in the file (`resolving_with_host_arg_partitions_the_model`,
`per_host_resolves_partition_and_carry_their_binding`) pass, so the
partitioning itself is still covered at the model level.

Nothing ran this test between the verb's retirement and now: `test-all` had
not run in CI since 2026-06-17 (issue 1651).

## Acceptance

The test asserts per-host partitioning through a surface that exists, or is
deleted with a note naming the test that covers it instead.
