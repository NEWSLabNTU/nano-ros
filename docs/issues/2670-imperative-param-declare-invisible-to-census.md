---
id: 2670
title: "A Rust parameter declared through `Executor::node_mut(..).declare_parameter` reaches no census hook, so a contract that omits it sizes the store short with no error"
status: open
type: bug
area: [params, testing]
severity: medium
found: 2026-11-17
related: [1419, 1421, 2589, 2647, 2650, phase-478]
---

**Home:** [phase-478](../roadmap/phase-478-island-developer-experience-findings.md),
"Issues homed here (2026-11-17)". Found while checking whether the metadata
probe can see parameter descriptions (issue 2650's follow-up). This is the
UNDER direction, so it is filed ahead of that work.

## What the code does

The parameter store's slot count, `NROS_MAX_PARAMETERS`, is derived from the
CONTRACT: `ParamDeclarations::from_model` reads `contracts.node_params`, each
node's `params:` list (`nros-cli-core/src/entity_inventory.rs`). The guard
against a contract that lists fewer parameters than the code declares is the
host census. Its `param-missing-in-contract` verdict (`entity_census.rs`) is
an error and cannot be waived, because a store sized short fails at boot.

The census only knows about a declaration that calls
`nros::census_hooks::on_param_declare`. There are three call sites, read on
2026-11-17:

| Declare path | Census hook |
| --- | --- |
| C `rclc` declare (`nros-c/src/parameter.rs`) | yes |
| C++ ffi declare (`nros-cpp/src/params_shim.rs`) | yes |
| Rust declarative entity (`nros/src/node_runtime.rs`, the `EntityKind::Parameter` arm reached by `declare_parameter_with_default` in `nros/src/node.rs`) | yes |
| Rust `Executor::node_mut(id).declare_parameter::<T>(name)` (`nros-node/src/executor/node.rs`) | **no** |
| Rust `Executor::node_mut(id).declare_parameter_with_descriptor(..)` | **no** |
| Rust `Executor::declare_parameter(name, value)` (`nros-node/src/executor/spin.rs`) | **no** |

`nros` re-exports `Executor`, so the last three are public user API. In-tree,
`nros-tests/bins/param-two-node-talker` declares all of its parameters that
way. nros-node sits below `nros` and cannot call `census_hooks`, so no hook
can be added at that call site as it stands.

## Why it matters

Take an image whose contract lists `params:` for its nodes, and whose code
also declares one or more parameters through those paths that the contract
does not list. The census runs the code, sees none of those declarations, and
reports no `param-missing-in-contract`. The store is then sized from the
contract alone, and the extra declarations hit "store full" at runtime. The
build said nothing.

With no `params:` in the contract, `ParamDeclarations` is `Absent` and the
store keeps its default (32) or the board's knob, so nothing is under-sized
on that road. `param-two-node-talker` states `knobs.params.max_parameters`
itself.

## Not yet done

This is a reading, not a reproduction. The next step is a fixture that
declares one parameter through `node_mut(..).declare_parameter` and lists a
different one in `params:`, then runs `nros ws census` on it. That would show
the missing `param-missing-in-contract` row, and boot would then fail with
store full.

## Possible shape

- Give nros-node's declare paths a census seam: a hook that nros-node calls
  and `nros` installs, the way the trace sink is installed, so the hook does
  not depend on which crate the call starts in.
- Or route the three paths through `nros`'s declarative entity path, which
  already records.
- Either way, the same seam is where issue 2650's follow-up would record a
  description's length.
