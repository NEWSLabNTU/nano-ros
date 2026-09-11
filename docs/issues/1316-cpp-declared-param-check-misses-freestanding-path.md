---
id: 1316
title: "The declared-parameter boot check (phase-446 W6) is unreachable from an
  embedded C++ node: it sits on the hosted-only `Node::declare_parameter`"
status: open
type: bug
area: api, cpp
severity: medium
related: [issue-1272]
---

## What happens

Phase-446 W6 checks each `declare_parameter` against the contract's `params:`
at boot: a name the contract does not declare, or one declared with another
type, refuses the boot and names node, parameter and contract. On the C++ side
the check is `Node::check_declared_param`, called from
`Node::declare_parameter` (`nros-cpp/include/nros/nros.hpp`).

`Node::declare_parameter` is declared inside the `#ifdef
NROS_CPP_NODE_HOSTED` block of `nros-cpp/include/nros/node.hpp` (the guard
opens at node.hpp:575; the declaration is at :725). A node whose sources also
build for an embedded target cannot take that block, so it cannot call the
facade at all: unqualified `declare_parameter<double>(...)` inside such a
node is "not declared in this scope".

What such a node calls instead is the freestanding forwarder
`nros::detail::node_param_declare` (`node_parameters.hpp`), which reaches the
executor's store through `nros_cpp_node_declare_param_*`
(`nros-cpp/src/params_shim.rs`). That path does NOT consult the declared-param
table. So the check silently does not apply.

## Measured

Autoware Safety Island, four C++ component nodes, 21 declared parameters, the
downstream phase 446 was built for. Its sources build for both native and
Zephyr, so each node carries a private forwarder onto
`nros::detail::node_param_declare` and none of the 21 declarations is checked
against the contract. Replacing the forwarder with the base's
`declare_parameter` does not compile:

```
error: 'declare_parameter' was not declared in this scope
```

## Why it matters

A mis-sized store is exactly what W6 exists to turn into a loud failure, and
phase-446 W4 now SIZES the store from the same `params:` the check reads. An
embedded image is the one that cannot absorb a wrong size, and it is the one
the check does not cover. The gap is invisible: the image builds and boots,
and nothing says the contract was never compared to the code.

## Fix shape

Put the check where every language road passes, rather than on one facade:

- `nros_cpp_node_declare_param_*` in `params_shim.rs` has the node id and can
  read the same generated table `nros_orchestration_ir::declared_params`
  feeds. Checking there covers the C API, hosted C++ and freestanding C++ at
  once, and the Rust path already checks in `node_runtime.rs`.
- If the check stays in C++, the freestanding forwarder needs it too, which
  means the generated table has to be reachable without the hosted block.

Either way a test should build a node through the freestanding path with a
parameter the contract does not declare, and assert the boot refuses.
