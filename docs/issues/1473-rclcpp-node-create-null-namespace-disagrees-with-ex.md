---
id: 1473
title: "`nros_cpp_node_create(ns = NULL)` forces the ROOT while
  `nros_cpp_node_create_ex(namespace_len = 0)` INHERITS the executor's
  namespace — one C++ FFI, two meanings for \"unset\""
status: open
type: bug
area: api
related: [rfc-0045, rfc-0089, 1434, 1443, 1456]
---

## Problem

`NodeBuilder::build()` in `nros-node` already implements "the caller named no
namespace ⇒ inherit the executor's":

```rust
// packages/core/nros-node/src/executor/node_record.rs
if let Some(ns) = self.namespace {
    ns_buf.push_str(ns)…
} else {
    ns_buf.push_str(self.executor.namespace.as_str())…
}
```

The two C++ FFI entry points reach that builder with opposite answers for the
same input:

* `nros_cpp_node_create_ex` leaves `.namespace()` UNCALLED when
  `opts.namespace_len == 0`, so an unset namespace INHERITS the executor's.
* `nros_cpp_node_create` substitutes the literal `"/"` for a NULL `namespace`
  and then always calls `.namespace("/")`, so an unset namespace OVERRIDES the
  executor's with the root.

Both are documented as "NULL / unset", and they are one FFI over one builder.

## Why it is filed separately from 1456

Issue 1456 is fixed and did not need this settled: the launch identity now
reaches an `rclcpp`-shape component through its `NodeHandle`, which carries the
node's OWN namespace, so nothing in the generated-entry path depends on
inheritance either way. Making `nros_cpp_node_create`'s NULL inherit was
weighed as a candidate fix for 1456 and rejected on its merits — it reaches the
namespace only, not the name, and the executor's namespace is the launch
namespace only for a SINGLE-node plan (`boot_config_view` emits identity facts
only when `plan.nodes.len() == 1`), so a multi-node plan's executor sits at the
root and a component in one would still be misplaced. RFC-0089's 2026-09-24
section records that reasoning.

What is left is the inconsistency itself, which is about the C ABI's contract
rather than about launch: a caller reading both signatures cannot predict which
namespace a node lands in, and the difference is invisible until an executor is
opened at a non-root namespace (which `nros::init(…, node_namespace)` has been
able to do since issue 1434).

## Not measured

Read off `packages/api/nros-cpp/src/lib.rs` (`nros_cpp_node_create` ~line 2076,
`nros_cpp_node_create_ex` ~line 2226) and `node_record.rs`'s `build()`. No
image was built that opens a namespaced executor and then calls the 4-arg form
with NULL; the claim to hold is "the two functions compute different
namespaces for the same stated input", which the source states.

## Direction

Decide which meaning "unset" has at this ABI and make both spellings agree —
then say so where RFC-0045 states the unset-versus-root distinction. Whichever
is chosen, the 4-arg form's doc-comment (`"namespace — … or NULL for \"/\""`)
or the `_ex` form's has to move with it.
