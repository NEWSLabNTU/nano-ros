---
id: 302
title: "`nros codegen entry --lang rust` emits an entry with NO params, remaps, identity or QoS overrides — it silently drifted from the canonical `nros::main!`"
status: open
type: bug
severity: medium
area: codegen, cli
related: [issue-0255, rfc-0046]
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
