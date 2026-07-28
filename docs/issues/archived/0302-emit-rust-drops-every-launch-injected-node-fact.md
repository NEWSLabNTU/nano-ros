---
id: 302
title: "`nros codegen entry --lang rust` emits an entry with NO params, remaps, identity or QoS overrides — it silently drifted from the canonical `nros::main!`"
status: resolved
type: bug
severity: medium
area: codegen, cli
related: [issue-0255, rfc-0046]
resolved_in: "issue-0302 (converged + parity gate)"
---

## Finding (2026-07-28, while fixing QoS overrides on the C and Rust paths)

`packages/cli/nros-cli-core/src/codegen/entry/emit_rust.rs` emits one bare
`::<pkg>::register(runtime)?;` per node and nothing else:

```rust
// emit_rust.rs — the whole per-node body
writeln!(register_calls, "            ::{}::register(runtime)?;", pkg)
```

The canonical emitter — the `nros::main!` proc-macro — sets four pieces of
per-node state on `runtime` before each of those same calls:

| set by `nros::main!` | what it carries | emitted by `emit_rust`? |
|---|---|---|
| `runtime.params` | the node's resolved parameters | **no** |
| `runtime.remaps` | launch `<remap>` rules (issue 0255) | **no** |
| `runtime.node_identity` | launch `<node name= namespace=>` (RFC-0046) | **no** |
| `runtime.qos_overrides` | per-topic QoS overrides (issue #52) | **no** |

So an entry produced by `nros codegen entry --lang rust` runs every node with
default parameters, no remaps, the node's own hardcoded name, and no QoS
overrides — from the same model that configures all four when the entry is
built through the proc-macro. `grep 'runtime\.' emit_rust.rs` returns nothing.

## Why it went unnoticed

`emit_rust`'s own doc comment says the proc-macro "remains the canonical
compile-time emitter" and that this emitter exists for "tooling that wants to
pre-bake the macro expansion (e.g. for byte-level diffs against the proc-macro
output)". A byte-level-diff tool that has silently stopped matching its
reference is the failure this file was written to detect, and nothing checks
it. Its three unit tests assert only that the register calls appear.

The four features arrived over four separate phases (264 W4a params, 268 W1
identity, 305 W3 remaps, 211.H QoS); each wired the proc-macro and left this
emitter behind. That is the same drift pattern as the duplicated parameter
matcher and the thrice-copied `ParamValue` renderer (RFC-0050 §"Semantics ship
with the schema"), one layer up: two emitters for one plan, no gate.

## Impact

Bounded but real. No in-tree fixture builds a Rust entry through the CLI verb
(every Rust example uses `nros::main!`), so nothing in the repo is currently
wrong because of this. A user who takes the documented "pre-bake the expansion"
path gets a silently degraded entry, and any future consumer of the verb
inherits four missing features at once.

## Direction

Two honest options; pick one rather than leaving the emitter half-alive.

1. **Converge.** Port the four bakes into `emit_rust`, and add the gate the
   file's own rationale implies: a test that emits both ways for one plan and
   asserts the per-node state matches. Without the gate the fifth feature drifts
   the same way.
2. **Retire.** If nobody pre-bakes expansions, delete `emit_rust` and make
   `nros codegen entry --lang rust` a removal error pointing at `nros::main!`
   — the same treatment phase-296 R-code gave the launch arms. Dead code that
   carries stale domain knowledge is worse than absent code, which is exactly
   what the `plan_from_launch` retirement found.

Retirement is the cheaper answer if the verb has no users; converge only if
something actually consumes it.

## Resolution (2026-07-28) — converged, not retired

The issue offered retire-or-converge and said retirement was cheaper "if the
verb has no users". It has one: `multihost_partition_bake.rs` drives
`nros codegen entry --lang rust --host <id>` to prove the per-host partition.

Two facts settled it toward converging:

- `Plan::for_host` has **no** unit test. The integration test's own doc claims
  it "complements the unit-level `Plan::for_host` test" — that test does not
  exist, so retiring the emitter would have removed the only coverage of the
  partition pipeline.
- The four bakes are ~40 lines against a plan that already carries all four
  fields (`params`, `remaps`, `qos_overrides`, `name`/`namespace`).

So `emit_node_state` now writes what `nros::main!` writes, in the same order,
before each register call.

**The reset discipline is the load-bearing part.** All four fields are written
unconditionally, including the empty case, because `runtime` is reused across
nodes: a node with no params must CLEAR the previous node's rather than inherit
them. Emitting only non-empty state would leak between nodes and still pass a
naive "does the output contain the value" test.

Also switched these literals from the raw-string form (`r#"…"#`) that
`quote_str` uses for paths to plain `LitStr`-style quoting, since the whole
stated purpose of this emitter is being byte-diffable against the macro, and
the macro emits plain literals.

### The gate

Two tests, which are the point of the fix — without them the fifth feature
drifts exactly like the first four:

- `every_node_gets_the_full_runtime_state_reset` — asserts the populated values
  AND that both nodes emit all four assignments, so a bare node is reset rather
  than skipped.
- `state_is_emitted_before_the_register_call` — ordering; after the call would
  configure the next node, or nothing.

462 cli-core tests pass.

## Found while fixing: the integration test is stale

`multihost_partition_bake::multihost_launch_bakes_per_host_entries` fails, and
has since phase-296 R4 — it drives `--launch`, which that phase removed in
favour of `--model`. Verified pre-existing by stashing this change and
re-running: identical failure.

So the verb's only consumer has been broken for some time, which is why nothing
caught the four-feature drift. Rewriting it against `--model` is worth doing;
it is the only thing exercising `Plan::for_host` end to end.

## Also fixed in passing

Five user-facing recipes still told users to run `play_launch resolve` —
stragglers from issue 0285's rename — in `cmd/codegen.rs`, `cmd/plan.rs`,
`cmd/codegen_system.rs` and `nros-build/src/lib.rs`. They now name
`nros-launch-resolve`.
