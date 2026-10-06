---
id: 1692
title: "`multihost_bake_emits_only_the_hosts_node` tests a verb that was retired —
  `nros codegen entry --lang rust` refuses since phase-432 W2.4, and an unstamped
  resolver pin refuses it one step earlier"
status: resolved
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

## Resolution

Fixed 2026-10-06 (branch `fix/1692-multihost-bake-surface`).

**Root cause, confirmed.** Both refusals as stated: the bare resolver stamps
`meta.resolver.version` with its own crate version (`0.9.0`), which the model
door has refused since phase-460 W1, and behind it `--lang rust` is retired
(phase-432 W2.4). There is no CLI spelling of a Rust bake left to grep. The C
and C++ verbs exist but bake only `--typed` from a CONFIGURE-produced
`nros-metadata.json` (`--lang c` without it: "non-typed --lang c entry is
retired (phase-257)"), so they could not replace it without a configure either.

**Fix.** The surface that exists for every language is the built image. The
test now runs each per-host fixture `multihost_e2e` boots -- rust, c, cpp and
mixed x robot1/robot2, baked from `nros sync`'s per-host models through each
language's real road -- as a CENSUS producer (`$NROS_CENSUS_OUT`: the entry
constructs every component its bake registered, writes the nodes and exits; no
router, no spin), and asserts robot1 registers `talker` (mixed: and
`heartbeat`) and not `listener`, and robot2 the reverse. `multihost_e2e` proves
robot1 reaches robot2; only this proves robot1 carries no listener. The census
run is one helper now, `nros_tests::census::take`; `workspace_metadata`'s three
census tests used a private copy of the same sequence and go through it too.

**Before / after.** Before: FAIL in 0.16 s, `--lang rust entry is retired`
behind `resolver pin changed (model 0.9.0 != ours bbf9c04496d0)`. After, solo
on fixtures built from this tree: `PASS [2.822s]
multihost_partition_bake multihost_bake_emits_only_the_hosts_node`. Negative
control: robot1's resolver pointed at the all-host
`build_native_workspace_rust_entry` FAILS with `[rust robot1] … native_entry
registers the OTHER host's node \`listener\` -- the per-host partition did not
reach the bake (census nodes: ["talker", "listener"])`.

**Consequence.** The test now needs fixtures, so it is not a candidate for
`.config/lane-admission/gate.txt` (its two fixture-free siblings are already
there); [issue 1654](../1654-census-two-fixture-free-reds-on-main.md) item 2 is
annotated accordingly.

**Sweep.** `git grep -n '"codegen", "entry"' packages/testing` -- this test was
the only caller.

**Not measured.** The four per-host fixtures were built here by
`just native build-workspace-fixtures`; the test was not run inside `test-all`
or a CI lane.
